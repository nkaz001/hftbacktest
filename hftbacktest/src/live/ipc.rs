use std::{
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

use bincode::{
    config,
    error::{DecodeError, EncodeError},
    Decode,
    Encode,
};
use iceoryx2::{
    iox2::Iox2,
    port::{
        publisher::{Publisher, PublisherLoanError, PublisherSendError},
        subscriber::{Subscriber, SubscriberReceiveError},
    },
    prelude::{zero_copy, Iox2Event, ServiceName},
    service::{port_factory::publish_subscribe::PortFactory, Service},
};
use thiserror::Error;

use crate::{
    live::{BotError, Channel},
    prelude::{LiveEvent, Request},
};

#[repr(C)]
#[derive(Debug)]
struct BinPayload {
    data: [u8; 1024],
    len: usize,
    id: usize,
}

impl Default for BinPayload {
    fn default() -> Self {
        Self {
            data: [0; 1024],
            len: 0,
            id: 0,
        }
    }
}

#[derive(Error, Debug)]
pub enum PubSubError {
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
}

pub struct IceoryxSender<T> {
    _pub_factory: PortFactory<zero_copy::Service, BinPayload>,
    publisher: Publisher<zero_copy::Service, BinPayload>,
    _t_marker: PhantomData<T>,
}

impl<T> IceoryxSender<T>
where
    T: Encode,
{
    pub fn build(name: &str) -> Result<Self, anyhow::Error> {
        let from_bot = ServiceName::new(&format!("{}/FromBot", name))?;
        let pub_factory = zero_copy::Service::new(&from_bot)
            .publish_subscribe()
            .max_publishers(1000)
            .max_subscribers(1)
            .open_or_create::<BinPayload>()?;

        let publisher = pub_factory.publisher().create()?;

        Ok(Self {
            _pub_factory: pub_factory,
            publisher,
            _t_marker: Default::default(),
        })
    }

    pub fn send(&self, data: &T) -> Result<(), PubSubError> {
        let sample = self.publisher.loan_uninit()?;
        let mut sample = unsafe { sample.assume_init() };
        let payload = sample.payload_mut();

        let length = bincode::encode_into_slice(data, &mut payload.data, config::standard())?;

        payload.len = length;
        sample.send()?;
        Ok(())
    }
}

pub struct IceoryxReceiver<T> {
    _sub_factory: PortFactory<zero_copy::Service, BinPayload>,
    subscriber: Subscriber<zero_copy::Service, BinPayload>,
    _t_marker: PhantomData<T>,
}

impl<T> IceoryxReceiver<T>
where
    T: Decode,
{
    pub fn build(name: &str) -> Result<Self, anyhow::Error> {
        let to_bot = ServiceName::new(&format!("{}/ToBot", name))?;
        let sub_factory = zero_copy::Service::new(&to_bot)
            .publish_subscribe()
            .max_publishers(1)
            .max_subscribers(1000)
            .open_or_create::<BinPayload>()?;

        let subscriber = sub_factory.subscriber().create()?;

        Ok(Self {
            _sub_factory: sub_factory,
            subscriber,
            _t_marker: Default::default(),
        })
    }

    pub fn receive(&self) -> Result<Option<T>, PubSubError> {
        match self.subscriber.receive()? {
            None => Ok(None),
            Some(sample) => {
                let bytes = &sample.data[0..sample.len];
                let (decoded, _len): (T, usize) =
                    bincode::decode_from_slice(bytes, config::standard())?;
                Ok(Some(decoded))
            }
        }
    }
}

pub struct IceoryxPubSub<S, R> {
    publisher: IceoryxSender<S>,
    subscriber: IceoryxReceiver<R>,
}

impl<S, R> IceoryxPubSub<S, R>
where
    S: Encode,
    R: Decode,
{
    pub fn new(name: &str) -> Result<Self, anyhow::Error> {
        let publisher = IceoryxSender::build(name)?;
        let subscriber = IceoryxReceiver::build(name)?;

        Ok(Self {
            publisher,
            subscriber,
        })
    }

    pub fn send(&self, data: &S) -> Result<(), PubSubError> {
        self.publisher.send(data)
    }

    pub fn receive(&self) -> Result<Option<R>, PubSubError> {
        self.subscriber.receive()
    }
}

pub struct PubSubList {
    pubsub: Vec<Rc<IceoryxPubSub<Request, LiveEvent>>>,
    pubsub_i: usize,
}

impl PubSubList {
    pub fn new(pubsub: Vec<Rc<IceoryxPubSub<Request, LiveEvent>>>) -> Self {
        assert!(!pubsub.is_empty());
        Self {
            pubsub,
            pubsub_i: 0,
        }
    }
}

impl Channel for PubSubList {
    fn recv_timeout(&mut self, timeout: Duration) -> Result<LiveEvent, BotError> {
        let instant = Instant::now();
        loop {
            let elapsed = instant.elapsed();
            if elapsed > timeout {
                return Err(BotError::Timeout);
            }

            // todo: this needs to retrieve Iox2Event without waiting.
            match Iox2::wait(Duration::from_nanos(1)) {
                Iox2Event::Tick => {
                    let pubsub = unsafe { self.pubsub.get_unchecked(self.pubsub_i) };

                    self.pubsub_i += 1;
                    if self.pubsub_i == self.pubsub.len() {
                        self.pubsub_i = 0;
                    }

                    if let Some(ev) = pubsub
                        .receive()
                        .map_err(|err| BotError::Custom(err.to_string()))?
                    {
                        return Ok(ev);
                    }
                }
                Iox2Event::TerminationRequest | Iox2Event::InterruptSignal => {
                    return Err(BotError::Interrupted);
                }
            }
        }
    }

    fn send(&mut self, asset_no: usize, request: Request) -> Result<(), BotError> {
        let publisher = self.pubsub.get(asset_no).ok_or(BotError::AssetNotFound)?;
        publisher
            .send(&request)
            .map_err(|err| BotError::Custom(err.to_string()))?;
        Ok(())
    }
}
