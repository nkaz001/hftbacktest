use std::time::Duration;

use hftbacktest::{
    live::ipc::{IceoryxReceiver, IceoryxSender},
    prelude::{LiveEvent, Status},
    types::Request,
};
use iceoryx2::{iox2::Iox2, prelude::Iox2Event};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedSender},
    task::LocalSet,
};
use tracing::error;

use crate::{bybit::Bybit, connector::Connector};

#[cfg(feature = "binancefutures")]
pub mod binancefutures;
#[cfg(feature = "bybit")]
pub mod bybit;

mod connector;
mod util;

fn sender<C: Connector>(
    name: &str,
    tx: UnboundedSender<LiveEvent>,
    connector: &mut C,
) -> Result<(), anyhow::Error> {
    let subscriber = IceoryxReceiver::<Request>::build(name)?;
    loop {
        let cycle_time = Duration::from_nanos(1000);
        match Iox2::wait(cycle_time) {
            Iox2Event::Tick => {
                while let Some(ev) = subscriber.receive()? {
                    match ev {
                        Request::Order { asset, order } => match order.req {
                            Status::New => {
                                connector.submit(asset, order, tx.clone())?;
                            }
                            Status::Canceled => {
                                connector.cancel(asset, order, tx.clone())?;
                            }
                            status => {
                                error!(?status, "");
                            }
                        },
                    }
                }
            }
            Iox2Event::TerminationRequest => {
                break;
            }
            Iox2Event::InterruptSignal => {
                break;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let name = "test";

    let (tx, mut rx) = unbounded_channel();

    let mut connector = Bybit::builder().build().unwrap();
    connector.run(tx.clone()).unwrap();

    let local = LocalSet::new();
    let handle = local.spawn_local(async move {
        let publisher = IceoryxSender::<LiveEvent>::build(name).unwrap();
        while let Some(ev) = rx.recv().await {
            if let Err(error) = publisher.send(&ev) {
                error!(?error, "");
                break;
            }
        }
    });

    if let Err(error) = sender(name, tx, &mut connector) {
        error!(?error, "");
    }
    let _ = handle.await;
}
