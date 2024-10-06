use std::{
    marker::PhantomData,
    rc::Rc,
    string::FromUtf8Error,
    time::{Duration, Instant},
};

use bincode::{
    config,
    error::{DecodeError, EncodeError},
    Decode,
    Encode,
};
use iceoryx2::{
    port::{
        publisher::{Publisher, PublisherLoanError, PublisherSendError},
        subscriber::{Subscriber, SubscriberReceiveError},
    },
    prelude::{ipc, Node, NodeBuilder, NodeEvent, ServiceName},
};
use thiserror::Error;

use crate::{
    live::{BotError, Channel},
    prelude::{LiveEvent, Request},
};

pub const TO_ALL: u64 = 0;

const MAX_PAYLOAD_SIZE: usize = 512;

#[derive(Default, Debug)]
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
    SubscriberReceive(#[from] SubscriberReceiveError),
    #[error("{0:?}")]
    PublisherLoan(#[from] PublisherLoanError),
    #[error("{0:?}")]
    PublisherSend(#[from] PublisherSendError),
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
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;
        let sub_factory = if self.bot {
            let service_name = ServiceName::new(&format!("{}/ToBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(100000)
                .max_publishers(1)
                .max_subscribers(500)
                .user_header::<CustomHeader>()
                .open_or_create()
                .map_err(|error| ChannelError::BuildError(error.to_string()))?
        } else {
            let service_name = ServiceName::new(&format!("{}/FromBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(100000)
                .max_publishers(500)
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
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;
        let pub_factory = if self.bot {
            let service_name = ServiceName::new(&format!("{}/FromBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(100000)
                .max_publishers(500)
                .max_subscribers(1)
                .user_header::<CustomHeader>()
                .open_or_create()
                .map_err(|error| ChannelError::BuildError(error.to_string()))?
        } else {
            let service_name = ServiceName::new(&format!("{}/ToBot", self.name))
                .map_err(|error| ChannelError::BuildError(error.to_string()))?;
            node.service_builder(&service_name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(100000)
                .max_publishers(1)
                .max_subscribers(500)
                .user_header::<CustomHeader>()
                .open_or_create()
                .map_err(|error| ChannelError::BuildError(error.to_string()))?
        };

        let publisher = pub_factory
            .publisher_builder()
            .max_slice_len(MAX_PAYLOAD_SIZE)
            .create()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;

        Ok(IceoryxSender {
            //_pub_factory: pub_factory,
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
    T: Decode,
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
}

impl<S, R> IceoryxChannel<S, R>
where
    S: Encode,
    R: Decode,
{
    pub fn new(name: &str) -> Result<Self, anyhow::Error> {
        let publisher = IceoryxBuilder::new(name).sender()?;
        let subscriber = IceoryxBuilder::new(name).receiver()?;

        Ok(Self {
            publisher,
            subscriber,
        })
    }

    pub fn send(&self, id: u64, data: &S) -> Result<(), ChannelError> {
        self.publisher.send(id, data)
    }

    pub fn receive(&self) -> Result<Option<(u64, R)>, ChannelError> {
        self.subscriber.receive()
    }
}

pub struct IceoryxUnifiedChannel {
    channel: Vec<Rc<IceoryxChannel<Request, LiveEvent>>>,
    ch_i: usize,
    node: Node<ipc::Service>,
}

impl IceoryxUnifiedChannel {
    pub fn new(
        channel_list: Vec<Rc<IceoryxChannel<Request, LiveEvent>>>,
    ) -> Result<Self, ChannelError> {
        assert!(!channel_list.is_empty());
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|error| ChannelError::BuildError(error.to_string()))?;
        Ok(Self {
            channel: channel_list,
            ch_i: 0,
            node,
        })
    }
}

impl Channel for IceoryxUnifiedChannel {
    fn recv_timeout(&mut self, id: u64, timeout: Duration) -> Result<LiveEvent, BotError> {
        let instant = Instant::now();
        loop {
            let elapsed = instant.elapsed();
            if elapsed > timeout {
                return Err(BotError::Timeout);
            }

            // todo: this needs to retrieve Iox2Event without waiting.
            match self.node.wait(Duration::from_nanos(1)) {
                NodeEvent::Tick => {
                    let ch = unsafe { self.channel.get_unchecked(self.ch_i) };

                    self.ch_i += 1;
                    if self.ch_i == self.channel.len() {
                        self.ch_i = 0;
                    }

                    if let Some((dst_id, ev)) = ch
                        .receive()
                        .map_err(|err| BotError::Custom(err.to_string()))?
                    {
                        if dst_id == 0 || dst_id == id {
                            return Ok(ev);
                        }
                    }
                }
                NodeEvent::TerminationRequest | NodeEvent::InterruptSignal => {
                    return Err(BotError::Interrupted);
                }
            }
        }
    }

    fn send(&mut self, asset_no: usize, request: Request) -> Result<(), BotError> {
        let publisher = self
            .channel
            .get(asset_no)
            .ok_or(BotError::InstrumentNotFound)?;
        publisher
            .send(TO_ALL, &request)
            .map_err(|err| BotError::Custom(err.to_string()))?;
        Ok(())
    }
}
