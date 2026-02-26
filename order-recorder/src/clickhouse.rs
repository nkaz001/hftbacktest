//! ClickHouse helpers for account equity and factor snapshots.

use crate::events::{AccountSnapshot, FactorSnapshot};
use ::clickhouse::{Client, Row};
use serde::Serialize;
use time::OffsetDateTime;

fn nanos_to_datetime(nanos: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(nanos as i128)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

#[derive(Row, Serialize, Debug)]
struct AccountEquityRow {
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    timestamp: OffsetDateTime,
    connector: String,
    strategy_id: String,
    account_equity: f64,
    actual_equity: f64,
    uni_mmr: f64,
    account_initial_margin: f64,
    account_maint_margin: f64,
}

impl From<&AccountSnapshot> for AccountEquityRow {
    fn from(snapshot: &AccountSnapshot) -> Self {
        Self {
            timestamp: nanos_to_datetime(snapshot.timestamp),
            connector: snapshot.connector.clone(),
            strategy_id: snapshot.strategy_id.clone(),
            account_equity: snapshot.account_equity,
            actual_equity: snapshot.actual_equity,
            uni_mmr: snapshot.uni_mmr,
            account_initial_margin: snapshot.account_initial_margin,
            account_maint_margin: snapshot.account_maint_margin,
        }
    }
}

pub struct AccountEquityWriter {
    client: Client,
}

impl AccountEquityWriter {
    pub fn new(url: &str, database: &str) -> Self {
        let client = Client::default().with_url(url).with_database(database);
        Self { client }
    }

    pub async fn write_account(
        &self,
        snapshot: &AccountSnapshot,
    ) -> Result<(), ::clickhouse::error::Error> {
        let mut inserter = self.client.insert("account_equity")?;
        inserter.write(&AccountEquityRow::from(snapshot)).await?;
        inserter.end().await
    }
}

// ---- Factor Snapshot Writer ----

#[derive(Row, Serialize, Debug)]
struct FactorSnapshotRow {
    #[serde(with = "clickhouse::serde::time::datetime64::nanos")]
    timestamp: OffsetDateTime,
    connector: String,
    strategy_id: String,
    symbol: String,
    factor_name: String,
    factor_value: f64,
}

pub struct FactorSnapshotWriter {
    client: Client,
}

impl FactorSnapshotWriter {
    pub fn new(url: &str, database: &str) -> Self {
        let client = Client::default().with_url(url).with_database(database);
        Self { client }
    }

    pub async fn write_factors(
        &self,
        snapshot: &FactorSnapshot,
    ) -> Result<(), ::clickhouse::error::Error> {
        let mut inserter = self.client.insert("factor_snapshots")?;
        let ts = nanos_to_datetime(snapshot.timestamp);
        for (name, value) in &snapshot.factors {
            inserter
                .write(&FactorSnapshotRow {
                    timestamp: ts,
                    connector: snapshot.connector.clone(),
                    strategy_id: snapshot.strategy_id.clone(),
                    symbol: snapshot.symbol.clone(),
                    factor_name: name.clone(),
                    factor_value: *value,
                })
                .await?;
        }
        inserter.end().await
    }
}
