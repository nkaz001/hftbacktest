use std::{io, io::ErrorKind};
use std::iter::Peekable;
use bus::{Bus, BusIntoIter, BusReader};
use tracing::{error, info, info_span};

use crate::backtest::{
    data::{read_npy_file, read_npz_file, Data, NpyDTyped},
    BacktestError,
};

#[derive(Copy, Clone)]
pub enum EventBusMessage<EventT: Clone> {
    Item(EventT),
    EndOfData,
}

pub struct EventBusReader<EventT: Clone + Send + Sync> {
    reader: Peekable<BusIntoIter<EventBusMessage<EventT>>>,
}

impl<EventT: Clone + Send + Sync> EventBusReader<EventT> {
    pub fn new(reader: BusReader<EventBusMessage<EventT>>) -> Self {
        Self {
            reader: reader.into_iter().peekable()
        }
    }

    pub fn peek(&mut self) -> Option<&EventT> {
        self.reader.peek().and_then(|ev| match ev {
            EventBusMessage::Item(item) => Some(item),
            EventBusMessage::EndOfData => None,
        })
    }

    pub fn next(&mut self) -> Option<EventT> {
        self.reader.next().and_then(|ev| match ev {
            EventBusMessage::Item(item) => Some(item),
            EventBusMessage::EndOfData => None,
        })
    }
}

pub trait TimestampedEventQueue<EventT> {
    fn next_event(&mut self) -> Option<EventT>;

    fn peek_event(&mut self) -> Option<&EventT>;

    fn event_time(value: &EventT) -> i64;
}

pub trait EventConsumer<EventT> {
    fn is_event_relevant(event: &EventT) -> bool;

    fn process_event(&mut self, event: EventT) -> Result<(), BacktestError>;
}

fn load_data<EventT: NpyDTyped + Clone + Send>(
    filepath: String,
) -> Result<Data<EventT>, BacktestError> {
    let data = if filepath.ends_with(".npy") {
        read_npy_file(&filepath)?
    } else if filepath.ends_with(".npz") {
        read_npz_file(&filepath, "data")?
    } else {
        return Err(BacktestError::DataError(io::Error::new(
            ErrorKind::InvalidData,
            "unsupported data type",
        )));
    };

    Ok(data)
}

#[tracing::instrument(skip_all)]
pub fn replay_events_to_bus<EventT: NpyDTyped + Clone + Send + 'static>(
    mut bus: Bus<EventBusMessage<EventT>>,
    mut sources: Vec<String>,
) {
    for source in sources.drain(..) {
        let source_load_span = info_span!("load_data", source = &source);
        let _source_load_span = source_load_span.entered();

        let data = load_data::<EventT>(source);

        match data {
            Ok(data) => {
                info!(
                    records = data.len(),
                    "found {} events in data source",
                    data.len()
                );

                for row in 0..data.len() {
                    bus.broadcast(EventBusMessage::Item(data[row].clone()));
                }
            }
            Err(e) => {
                error!("encountered error loading data source: {}", e);
                // TODO: handle as an error.
                break;
            }
        }
    }

    bus.broadcast(EventBusMessage::EndOfData);
}
