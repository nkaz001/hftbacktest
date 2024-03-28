use std::collections::{HashMap, HashSet};

use crate::{
    connector::Connector,
    live::bot::{Bot, BotError, ErrorHandler},
    ty::Error,
    BuildError,
};

pub mod bot;

#[derive(Clone)]
pub struct AssetInfo {
    pub asset_no: usize,
    pub symbol: String,
    pub tick_size: f32,
    pub lot_size: f32,
}

/// Live [`Bot`] builder.
pub struct LiveBuilder {
    conns: HashMap<String, Box<dyn Connector + Send + 'static>>,
    assets: Vec<(String, AssetInfo)>,
    error_handler: Option<ErrorHandler>,
}

impl LiveBuilder {
    /// Constructs [`LiveBuilder`].
    pub fn new() -> Self {
        Self {
            conns: HashMap::new(),
            assets: Vec::new(),
            error_handler: None,
        }
    }

    /// Registers a [`Connector`] with a specified name.
    /// The specified name for this connector is used when using [`LiveBuilder::add`] to add an
    /// asset for trading through this connector.
    pub fn register<C>(mut self, name: &str, conn: C) -> Self
    where
        C: Connector + Send + 'static,
    {
        self.conns.insert(name.to_string(), Box::new(conn));
        self
    }

    /// Adds an asset.
    ///
    /// * `name` - Name of the [`Connector`], which is registered by [`LiveBuilder::register`],
    ///            through which this asset will be traded.
    /// * `symbol` - Symbol of the asset. You need to check with the [`Connector`] which symbology
    ///              is used.
    /// * `tick_size` - The minimum price fluctuation.
    /// * `lot_size` -  The minimum trade size.
    pub fn add(mut self, name: &str, symbol: &str, tick_size: f32, lot_size: f32) -> Self {
        let asset_no = self.assets.len();
        self.assets.push((
            name.to_string(),
            AssetInfo {
                asset_no,
                symbol: symbol.to_string(),
                tick_size,
                lot_size,
            },
        ));
        self
    }

    /// Registers the error handler to deal with an error from connectors.
    pub fn error_handler<Handler>(mut self, handler: Handler) -> Self
    where
        Handler: FnMut(Error) -> Result<(), BotError> + 'static,
    {
        self.error_handler = Some(Box::new(handler));
        self
    }

    /// Builds a live [`Bot`] based on the registered connectors and assets.
    pub fn build(self) -> Result<Bot, BuildError> {
        let mut dup = HashSet::new();
        let mut conns = self.conns;
        for (an, (name, asset_info)) in self.assets.iter().enumerate() {
            if !dup.insert(format!("{}/{}", name, asset_info.symbol)) {
                Err(BuildError::Duplicate(
                    name.clone(),
                    asset_info.symbol.clone(),
                ))?;
            }
            let conn = conns
                .get_mut(name)
                .ok_or(BuildError::ConnectorNotFound(name.to_string()))?;
            conn.add(
                an,
                asset_info.symbol.clone(),
                asset_info.tick_size,
                asset_info.lot_size,
            )?;
        }

        let con = Bot::new(conns, self.assets, self.error_handler);
        Ok(con)
    }
}
