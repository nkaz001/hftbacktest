use flate2::read::GzDecoder;
use hftbacktest::types::Event;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter};

use hftbacktest::backtest::data::write_npy_data;

use crate::bybit::bybit_process;

pub struct ConverterBase {
    base_latency: i64,
    min_latency: i64,
    time_mul: i64,
}

impl ConverterBase {
    pub fn new(base_latency: i64) -> Self {
        Self {
            base_latency,
            min_latency: i64::MAX,
            time_mul: 1_000_000, // convert to nanos.
        }
    }

    pub fn convert_ts(&self, ts: i64) -> i64 {
        ts * self.time_mul
    }

    pub fn latency(&mut self, latency: i64) -> i64 {
        if latency > 0 && latency < self.min_latency {
            self.min_latency = latency;
        }

        if latency < 0 {
            return self.min_latency + self.base_latency;
        }

        latency
    }
}

#[allow(non_camel_case_types)]
pub enum Converter {
    bybit(ConverterBase),
}

pub trait IConverter {
    fn process(
        &mut self,
        local_ts: i64,
        payload: &str,
    ) -> Result<Vec<Event>, Box<dyn std::error::Error>>;
}

impl IConverter for Converter {
    fn process(
        &mut self,
        local_ts: i64,
        payload: &str,
    ) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
        match self {
            Converter::bybit(base) => bybit_process(base, local_ts, payload),
        }
    }
}

impl Converter {
    pub fn new(exchange: &str, base_latency: i64) -> Self {
        match exchange {
            "bybit" => Converter::bybit(ConverterBase::new(base_latency)),
            _ => panic!("Unknown exchange"),
        }
    }

    pub fn process_file(
        &mut self,
        input: BufReader<GzDecoder<File>>,
        output: &mut BufWriter<File>,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut counter: usize = 0;
        for line in input.lines() {
            let line = line?;

            // Split the line into timestamp and JSON part
            if let Some((timestamp_str, json_str)) = line.split_once(' ') {
                // Parse the timestamp
                let timestamp: i64 = timestamp_str.parse()?;
                let events = self.process(timestamp, json_str)?;
                counter += events.len();
                write_npy_data(output, events.as_slice())?;
            } else {
                eprintln!("Error: line format incorrect: {}", line);
            }
        }

        Ok(counter)
    }
}
