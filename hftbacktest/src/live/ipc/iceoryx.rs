use std::{
    collections::{HashMap, hash_map::Entry},
    marker::PhantomData,
    rc::Rc,
    string::FromUtf8Error,
    time::{Duration, Instant},
};

use bincode::{
    Decode,
    Encode,
    config,
    error::{DecodeError, EncodeError},
};
use iceoryx2::{
    port::{LoanError, ReceiveError, SendError, publisher::Publisher, subscriber::Subscriber},
    prelude::{Node, NodeBuilder, ServiceName, ZeroCopySend, ipc},
};
use thiserror::Error;

use crate::{
    live::{
        BotError,
        Instrument,
        ipc::{
            Channel,
            config::{ChannelConfig, MAX_PAYLOAD_SIZE},
        },
    },
    prelude::{LiveEvent, LiveRequest},
    types::BuildError,
};

#[derive(Default, Debug, ZeroCopySend)]
#[repr(C)]
pub struct CustomHeader {
    pub id: u64,
    pub len: usize,
}

#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("BuildError - {0}")]
    BuildError(String),
    #[error("{0:?}")]
    ReceiveError(#[from] ReceiveError),
    #[error("{0:?}")]
    LoanError(#[from] LoanError),
    #[error("{0:?}")]
    SendError(#[from] SendError),
    #[error("{0:?}")]
    Decode(#[from] DecodeError),
    #[error("{0:?}")]
    Encode(#[from] EncodeError),
    #[error("{0:?}")]
    FromUtf8(#[from] FromUtf8Error),
}

pub struct IceoryxBuilder {
    name: String,
    bot: bool,
}

impl IceoryxBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            bot: true,
        }
    }

    pub fn bot(self, bot: bool) -> Self {
        Self { bot, ..self }
    }

    pub fn receiver<T>(self) -> Result<IceoryxReceiver<T>, ChannelError> {
        let config = ChannelConfig::load_config();
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;

        let sub_factory = if self.bot {
            let service_name = ServiceName::new(&format!("{}/ToBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(config.buffer_size)
                .max_publishers(1)
                .max_subscribers(config.max_bots)
                .user_header::<CustomHeader>()
                .open_or_create()
                .map_err(|error| ChannelError::BuildError(error.to_string()))?
        } else {
            let service_name = ServiceName::new(&format!("{}/FromBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(config.buffer_size)
                .max_publishers(config.max_bots)
                .max_subscribers(1)
                .user_header::<CustomHeader>()
                .open_or_create()
                .map_err(|error| ChannelError::BuildError(error.to_string()))?
        };

        let subscriber = sub_factory
            .subscriber_builder()
            .create()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;

        Ok(IceoryxReceiver {
            subscriber,
            _t_marker: Default::default(),
        })
    }

    pub fn sender<T>(self) -> Result<IceoryxSender<T>, ChannelError> {
        let config = ChannelConfig::load_config();
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;

        let pub_factory = if self.bot {
            let service_name = ServiceName::new(&format!("{}/FromBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(config.buffer_size)
                .max_publishers(config.max_bots)
                .max_subscribers(1)
                .user_header::<CustomHeader>()
                .open_or_create()
                .map_err(|error| ChannelError::BuildError(error.to_string()))?
        } else {
            let service_name = ServiceName::new(&format!("{}/ToBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(config.buffer_size)
                .max_publishers(1)
                .max_subscribers(config.max_bots)
                .user_header::<CustomHeader>()
                .open_or_create()
                .map_err(|error| ChannelError::BuildError(error.to_string()))?
        };

        let publisher = pub_factory
            .publisher_builder()
            .initial_max_slice_len(MAX_PAYLOAD_SIZE)
            .create()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;

        Ok(IceoryxSender {
            publisher,
            _t_marker: Default::default(),
        })
    }
}

pub struct IceoryxSender<T> {
    publisher: Publisher<ipc::Service, [u8], CustomHeader>,
    _t_marker: PhantomData<T>,
}

impl<T> IceoryxSender<T>
where
    T: Encode,
{
    pub fn send(&self, id: u64, data: &T) -> Result<(), ChannelError> {
        let sample = self.publisher.loan_slice_uninit(MAX_PAYLOAD_SIZE)?;
        let mut sample = unsafe { sample.assume_init() };

        let payload = sample.payload_mut();
        let length = bincode::encode_into_slice(data, payload, config::standard())?;

        sample.user_header_mut().id = id;
        sample.user_header_mut().len = length;

        sample.send()?;

        Ok(())
    }
}

pub struct IceoryxReceiver<T> {
    subscriber: Subscriber<ipc::Service, [u8], CustomHeader>,
    _t_marker: PhantomData<T>,
}

impl<T> IceoryxReceiver<T>
where
    T: Decode<()>,
{
    pub fn receive(&self) -> Result<Option<(u64, T)>, ChannelError> {
        match self.subscriber.receive()? {
            None => Ok(None),
            Some(sample) => {
                let id = sample.user_header().id;
                let len = sample.user_header().len;

                let bytes = &sample.payload()[0..len];
                let (decoded, _len): (T, usize) =
                    bincode::decode_from_slice(bytes, config::standard())?;
                Ok(Some((id, decoded)))
            }
        }
    }
}

pub struct IceoryxChannel<S, R> {
    publisher: IceoryxSender<S>,
    subscriber: IceoryxReceiver<R>,
    symbol_to_inst_no: HashMap<String, usize>,
}

impl<S, R> IceoryxChannel<S, R>
where
    S: Encode,
    R: Decode<()>,
{
    pub fn new(name: &str) -> Result<Self, ChannelError> {
        let publisher = IceoryxBuilder::new(name).sender()?;
        let subscriber = IceoryxBuilder::new(name).receiver()?;

        Ok(Self {
            publisher,
            subscriber,
            symbol_to_inst_no: Default::default(),
        })
    }

    pub fn register(&mut self, inst_no: usize, symbol: &str) -> bool {
        if self.symbol_to_inst_no.contains_key(symbol) {
            return false;
        }
        self.symbol_to_inst_no.insert(symbol.to_string(), inst_no);
        true
    }

    pub fn send(&self, id: u64, data: &S) -> Result<(), ChannelError> {
        self.publisher.send(id, data)
    }

    pub fn receive(&self) -> Result<Option<(u64, R)>, ChannelError> {
        self.subscriber.receive()
    }
}

pub struct IceoryxUnifiedChannel {
    channel: Vec<Rc<IceoryxChannel<LiveRequest, LiveEvent>>>,
    unique_channel: Vec<Rc<IceoryxChannel<LiveRequest, LiveEvent>>>,
    ch_i: usize,
    node: Node<ipc::Service>,
}

impl IceoryxUnifiedChannel {
    pub fn new(
        channel: Vec<Rc<IceoryxChannel<LiveRequest, LiveEvent>>>,
    ) -> Result<Self, ChannelError> {
        assert!(!channel.is_empty());

        let unique_channel = {
            let mut unique_vec = Vec::new();

            for item in &channel {
                if !unique_vec.iter().any(|x| Rc::ptr_eq(x, item)) {
                    unique_vec.push(item.clone());
                }
            }

            unique_vec
        };

        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;
        Ok(Self {
            channel,
            unique_channel,
            ch_i: 0,
            node,
        })
    }
}

impl Channel for IceoryxUnifiedChannel {
    fn build<MD>(instruments: &[Instrument<MD>]) -> Result<Self, BuildError>
    where
        Self: Sized,
    {
        let mut channel: HashMap<String, IceoryxChannel<LiveRequest, LiveEvent>> = HashMap::new();
        for (inst_no, instrument) in instruments.iter().enumerate() {
            match channel.entry(instrument.connector_name.clone()) {
                Entry::Occupied(mut entry) => {
                    let ch = entry.get_mut();
                    if !ch.register(inst_no, &instrument.symbol) {
                        return Err(BuildError::Duplicate(
                            instrument.connector_name.clone(),
                            instrument.symbol.clone(),
                        ));
                    }
                }
                Entry::Vacant(entry) => {
                    let mut ch = IceoryxChannel::new(&instrument.connector_name)
                        .map_err(|error| BuildError::Error(anyhow::Error::from(error)))?;
                    if !ch.register(inst_no, &instrument.symbol) {
                        return Err(BuildError::Duplicate(
                            instrument.connector_name.clone(),
                            instrument.symbol.clone(),
                        ));
                    }
                    entry.insert(ch);
                }
            }
        }
        let channel: HashMap<_, _> = channel.into_iter().map(|(k, v)| (k, Rc::new(v))).collect();
        let channel_list: Vec<_> = instruments
            .iter()
            .map(|i| channel.get(&i.connector_name).unwrap().clone())
            .collect();

        IceoryxUnifiedChannel::new(channel_list)
            .map_err(|error| BuildError::Error(anyhow::Error::from(error)))
    }

    fn recv_timeout(&mut self, id: u64, timeout: Duration) -> Result<(usize, LiveEvent), BotError> {
        let instant = Instant::now();
        loop {
            let elapsed = instant.elapsed();
            if elapsed > timeout {
                return Err(BotError::Timeout);
            }

            // todo: this needs to retrieve Iox2Event without waiting.
            match self.node.wait(Duration::from_nanos(1)) {
                Ok(()) => {
                    let ch = unsafe { self.unique_channel.get_unchecked(self.ch_i) };

                    self.ch_i += 1;
                    if self.ch_i == self.unique_channel.len() {
                        self.ch_i = 0;
                    }

                    if let Some((dst_id, ev)) = ch
                        .receive()
                        .map_err(|err| BotError::Custom(err.to_string()))?
                    {
                        if dst_id == 0 || dst_id == id {
                            match &ev {
                                LiveEvent::BatchStart
                                | LiveEvent::BatchEnd
                                | LiveEvent::Error(_) => {
                                    // todo: it may cause incorrect usage.
                                    return Ok((0, ev));
                                }
                                LiveEvent::Feed { symbol, .. }
                                | LiveEvent::Order { symbol, .. }
                                | LiveEvent::Position { symbol, .. } => {
                                    if let Some(inst_no) = ch.symbol_to_inst_no.get(symbol) {
                                        return Ok((*inst_no, ev));
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_error) => {
                    return Err(BotError::Interrupted);
                }
            }
        }
    }

    fn send(&mut self, id: u64, inst_no: usize, request: LiveRequest) -> Result<(), BotError> {
        self.channel
            .get(inst_no)
            .ok_or(BotError::InstrumentNotFound)?
            .send(id, &request)
            .map_err(|err| BotError::Custom(err.to_string()))?;
        Ok(())
    }
}
