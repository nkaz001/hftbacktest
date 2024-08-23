use std::{fs::File, io, io::ErrorKind, iter::Peekable, num::NonZeroUsize};

use bus::{Bus, BusIntoIter, BusReader};
use tracing::{error, info, info_span};
use zip::ZipArchive;

use crate::{
    backtest::{
        data::{npy::NpyReader, read_npy_file, read_npz_file, Data, NpyDTyped},
        BacktestError,
    },
    types::Event,
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
            reader: reader.into_iter().peekable(),
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

#[tracing::instrument(skip(bus))]
pub fn replay_event_file<EventT: NpyDTyped + Clone + Send + 'static>(
    path: String,
    bus: &mut Bus<EventBusMessage<EventT>>,
) -> std::io::Result<()> {
    if !path.ends_with(".npz") {
        todo!("Only .npz is supported in this branch")
    }

    let mut archive = ZipArchive::new(File::open(path)?)?;
    let mut reader = NpyReader::<_, EventT>::new(
        archive.by_name("data.npy")?,
        NonZeroUsize::new(512).unwrap(),
    )?;

    loop {
        let read = reader.read(|event| {
            bus.broadcast(EventBusMessage::Item(event.clone()));
        })?;

        // EOF
        if read == 0 {
            break;
        }
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
pub fn replay_events_to_bus<EventT: NpyDTyped + Clone + Send + 'static>(
    mut bus: Bus<EventBusMessage<EventT>>,
    mut sources: Vec<String>,
) {
    for source in sources.drain(..) {
        let source_load_span = info_span!("load_data", source = &source);
        let _source_load_span = source_load_span.entered();

        replay_event_file(source, &mut bus).unwrap();
    }

    bus.broadcast(EventBusMessage::EndOfData);
}
