use std::time::Duration;

use crate::{
    live::{BotError, Instrument},
    prelude::BuildError,
    types::{LiveEvent, LiveRequest},
};

mod config;
pub mod iceoryx;

pub const TO_ALL: u64 = 0;

/// Provides the IPC communication methods.
pub trait Channel {
    /// Builds a [`Channel`] based on a list of [`Instrument`].
    fn build<MD>(instruments: &[Instrument<MD>]) -> Result<Self, BuildError>
    where
        Self: Sized;

    /// Attempts to receive a [`LiveEvent`] from all registered connectors until the specified
    /// `timeout` duration is reached.
    /// If the ID of the received message does not match the provided ID, the message will be
    /// ignored and this will attempt to receive a [`LiveEvent`] again until the timeout is reached.
    ///
    /// `(instrument_no, LiveEvent)` will be returned if the message is received.
    fn recv_timeout(&mut self, id: u64, timeout: Duration) -> Result<(usize, LiveEvent), BotError>;

    /// Sends a [`LiveRequest`] to the connector corresponding to the `inst_no`.
    fn send(&mut self, id: u64, inst_no: usize, request: LiveRequest) -> Result<(), BotError>;
}
