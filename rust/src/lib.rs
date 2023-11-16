pub mod assettype;
pub mod depth;
pub mod order;
pub mod state;
pub mod models;
pub mod proc;
pub mod reader;
pub mod backtest;

mod evs;

use std::io::Error as IoError;

#[derive(Debug)]
pub enum Error {
    OrderAlreadyExist,
    OrderRequestInProcess,
    OrderNotFound,
    InvalidOrderRequest,
    InvalidOrderStatus,
    EndOfData,
    DataError(IoError)
}

impl From<IoError> for Error {
    fn from(value: IoError) -> Self {
        Error::DataError(value)
    }
}