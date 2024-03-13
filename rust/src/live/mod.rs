use std::collections::{HashMap, HashSet};

use crate::{connector::Connector, error::BuildError, live::bot::Bot};

pub mod bot;

#[derive(Clone)]
pub struct AssetInfo {
    pub asset_no: usize,
    pub symbol: String,
    pub tick_size: f32,
    pub lot_size: f32,
}

pub struct LiveBuilder {
    conns: HashMap<String, Box<dyn Connector + Send + 'static>>,
    assets: Vec<(String, AssetInfo)>,
}

impl LiveBuilder {
    pub fn new() -> Self {
        Self {
            conns: HashMap::new(),
            assets: Vec::new(),
        }
    }

    pub fn register<C>(mut self, name: &str, conn: C) -> Self
    where
        C: Connector + Send + 'static,
    {
        self.conns.insert(name.to_string(), Box::new(conn));
        self
    }

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

        let con = Bot::new(conns, self.assets);
        Ok(con)
    }
}
