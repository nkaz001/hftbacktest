use std::{env, fs};

use serde::Deserialize;

pub const MAX_PAYLOAD_SIZE: usize = 512;
const MAX_BOTS_PER_CONNECTOR: usize = 200;
const CHANNEL_BUFFER_SIZE: usize = 10000;

#[derive(Deserialize)]
pub struct ChannelConfig {
    pub buffer_size: usize,
    pub max_bots: usize,
}

impl ChannelConfig {
    pub fn load_config() -> Self {
        if let Ok(config_file) = env::var("HFTBACKTEST_CHANNEL_CONFIG") {
            if let Ok(contents) = fs::read_to_string(config_file) {
                if let Ok(config) = toml::from_str::<ChannelConfig>(&contents) {
                    return config;
                }
            }
        }
        ChannelConfig::default()
    }
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            buffer_size: CHANNEL_BUFFER_SIZE,
            max_bots: MAX_BOTS_PER_CONNECTOR,
        }
    }
}
