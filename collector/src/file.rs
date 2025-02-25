use std::{
    collections::{HashMap, hash_map::Entry},
    fs::File,
    io,
    io::Write,
};

use chrono::{DateTime, NaiveDate, Utc};
use flate2::{Compression, write::GzEncoder};
use tracing::info;

pub struct RotatingFile {
    date: NaiveDate,
    path: String,
    file: Option<GzEncoder<File>>,
}

impl RotatingFile {
    fn create(datetime: DateTime<Utc>, path: &str) -> Result<GzEncoder<File>, io::Error> {
        let date = datetime.date_naive().format("%Y%m%d");
        let file = File::options()
            .create(true)
            .write(true)
            .open(format!("{path}_{date}.gz"))?;
        Ok(GzEncoder::new(file, Compression::default()))
    }

    pub fn new(datetime: DateTime<Utc>, path: String) -> Result<Self, io::Error> {
        Ok(Self {
            date: datetime.date_naive(),
            file: Some(Self::create(datetime, &path)?),
            path,
        })
    }

    pub fn write(&mut self, datetime: DateTime<Utc>, data: String) -> Result<(), io::Error> {
        let date = datetime.date_naive();
        if date != self.date {
            let file = self.file.take().unwrap();
            let _ = file.finish();
            self.file = Some(Self::create(datetime, &self.path)?);
            self.date = date;
            info!(%date, %self.path, "date is changed");
        }
        let timestamp = datetime.timestamp_nanos_opt().unwrap();
        self.file
            .as_mut()
            .unwrap()
            .write_all(format!("{timestamp} {data}\n").as_bytes())
    }
}

impl Drop for RotatingFile {
    fn drop(&mut self) {
        let _ = self.file.take().unwrap().finish();
    }
}

pub struct Writer {
    path: String,
    file: HashMap<String, RotatingFile>,
}

impl Writer {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            file: Default::default(),
        }
    }

    pub fn write(
        &mut self,
        recv_time: DateTime<Utc>,
        symbol: String,
        data: String,
    ) -> Result<(), anyhow::Error> {
        match self.file.entry(symbol.to_lowercase()) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().write(recv_time, data)?;
            }
            Entry::Vacant(entry) => {
                let symbol = entry.key().clone();
                let path = self.path.as_str();
                entry
                    .insert(RotatingFile::new(recv_time, format!("{path}/{symbol}"))?)
                    .write(recv_time, data)?;
            }
        }
        Ok(())
    }
}
