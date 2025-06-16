use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use hftbacktest::prelude::*;
use tokio::{
    select,
    sync::{
        broadcast::{Receiver, error::RecvError},
        mpsc::UnboundedSender,
    },
    time,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::{debug, error, warn};

use crate::{
    binancespot::{
        BinanceSpotError,
        msg::stream::{UserEventStream, UserStream},
        rest::BinanceSpotClient,
    },
    connector::PublishEvent,
};


pub struct UserDataStream {
    client: BinanceSpotClient,
    listen_key: String,
    user_event_stream: UserEventStream,
    sender: UnboundedSender<UserStream>,
    receiver: Receiver<UserStream>,
}

// impl UserDataStream {
//     pub fn new(
//         client: BinanceSpotClient,
//         ev_tx: UnboundedSender<PublishEvent>,
//         order_manager: SharedOrderManager,
//         symbols: SharedSymbolSet,
//         symbol_rx: Receiver<String>,
//     ) -> Self {
//         Self {
//             symbols,
//             client,
//             ev_tx,
//             order_manager,
//             symbol_rx,
//         }
//     }
// }