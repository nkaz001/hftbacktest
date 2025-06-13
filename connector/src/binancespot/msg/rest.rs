use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub last_update_id: i64,
    pub asks: Vec<(String,String)>,
    pub bids: Vec<(String,String)>,
}
