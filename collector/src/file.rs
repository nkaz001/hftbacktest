use std::{
    collections::{HashMap, hash_map::Entry},
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
};

use chrono::{DateTime, NaiveDate, Utc};
use flate2::{Compression, write::GzEncoder};
use tracing::{info, warn};

/// Compress a .jsonl file to .gz and remove the original
fn compress_jsonl_to_gz(jsonl_path: &Path) -> io::Result<()> {
    let gz_path = jsonl_path.with_extension("gz");

    info!(?jsonl_path, ?gz_path, "Compressing jsonl to gz");

    // Read the jsonl file
    let input_file = File::open(jsonl_path)?;
    let mut reader = BufReader::new(input_file);

    // Create the gz file
    let output_file = File::create(&gz_path)?;
    let mut encoder = GzEncoder::new(output_file, Compression::default());

    // Copy data
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        encoder.write_all(&buffer[..bytes_read])?;
    }

    // Finish compression
    encoder.finish()?;

    // Remove the original jsonl file
    std::fs::remove_file(jsonl_path)?;

    info!(?gz_path, "Compression complete");
    Ok(())
}

/// Rotating file that writes to .jsonl (uncompressed) for real-time reading.
/// On date rotation, compresses the old file to .gz.
pub struct RotatingFile {
    date: NaiveDate,
    path: String,
    file: Option<BufWriter<File>>,
}

impl RotatingFile {
    fn create(datetime: DateTime<Utc>, path: &str) -> Result<BufWriter<File>, io::Error> {
        let date = datetime.date_naive().format("%Y%m%d");
        let file = File::options()
            .create(true)
            .append(true) // Append mode so we don't lose data on restart
            .open(format!("{path}_{date}.jsonl"))?;
        Ok(BufWriter::new(file))
    }

    fn jsonl_path(&self, date: NaiveDate) -> String {
        let date_str = date.format("%Y%m%d");
        format!("{}_{}.jsonl", self.path, date_str)
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
            // Close the current file
            if let Some(mut file) = self.file.take() {
                file.flush()?;
            }

            // Compress the old file in background (or synchronously for simplicity)
            let old_jsonl_path = self.jsonl_path(self.date);
            let old_path = Path::new(&old_jsonl_path);
            if old_path.exists() {
                if let Err(e) = compress_jsonl_to_gz(old_path) {
                    warn!(?e, path = %old_jsonl_path, "Failed to compress old jsonl file");
                }
            }

            // Create new file for today
            self.file = Some(Self::create(datetime, &self.path)?);
            self.date = date;
            info!(%date, %self.path, "Date changed, rotated to new file");
        }

        let timestamp = datetime.timestamp_nanos_opt().unwrap();
        let file = self.file.as_mut().unwrap();
        writeln!(file, "{timestamp} {data}")?;

        Ok(())
    }

    /// Flush the buffer to disk
    pub fn flush(&mut self) -> io::Result<()> {
        if let Some(ref mut file) = self.file {
            file.flush()?;
        }
        Ok(())
    }
}

impl Drop for RotatingFile {
    fn drop(&mut self) {
        // Just flush, don't compress - keep as .jsonl for warmup
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
        }
    }
}

pub struct Writer {
    path: String,
    file: HashMap<String, RotatingFile>,
}

impl Writer {
    pub fn new(path: &str) -> Self {
        // On startup, compress any old .jsonl files from previous days
        Self::compress_old_jsonl_files(path);

        Self {
            path: path.to_string(),
            file: Default::default(),
        }
    }

    /// Compress any .jsonl files from previous days
    fn compress_old_jsonl_files(path: &str) {
        let today = Utc::now().date_naive().format("%Y%m%d").to_string();

        let dir = Path::new(path);
        if !dir.exists() {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let file_path = entry.path();
                if let Some(ext) = file_path.extension() {
                    if ext == "jsonl" {
                        // Check if this is from a previous day
                        let filename = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

                        // Extract date from filename (e.g., "btcusdt_20251227")
                        if let Some(date_part) = filename.rsplit('_').next() {
                            if date_part != today && date_part.len() == 8 {
                                // This is an old file, compress it
                                info!(?file_path, "Found old jsonl file, compressing");
                                if let Err(e) = compress_jsonl_to_gz(&file_path) {
                                    warn!(?e, ?file_path, "Failed to compress old jsonl file");
                                }
                            }
                        }
                    }
                }
            }
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

    /// Flush all files to disk without closing them.
    /// The files remain as .jsonl for warmup.
    pub fn flush(&mut self) {
        info!("Flushing {} open files...", self.file.len());
        for (symbol, rotating_file) in self.file.iter_mut() {
            if let Err(e) = rotating_file.flush() {
                warn!(%symbol, ?e, "Error flushing file");
            } else {
                info!(%symbol, "File flushed successfully");
            }
        }
    }
}
