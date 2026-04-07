//! Tardis.dev data converter — shared types and NPZ writer.
//!
//! The actual conversion logic lives in `tardis_fast.rs`.

use std::fs::File;
use std::path::Path;

use crate::types::Event;

#[cfg(feature = "backtest")]
use crate::backtest::data::write_npy;

/// Snapshot processing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotMode {
    /// Process all snapshots
    Process,
    /// Ignore Start-of-Day snapshots only
    IgnoreSod,
    /// Ignore all snapshots
    Ignore,
}

/// Configuration for Tardis conversion
#[derive(Debug, Clone)]
pub struct TardisConvertConfig {
    /// Buffer size for events (default: 100_000_000)
    pub buffer_size: usize,
    /// Buffer size for snapshot events (default: 1_000_000)
    pub ss_buffer_size: usize,
    /// Base latency to add (in nanoseconds, default: 0)
    pub base_latency: i64,
    /// Snapshot processing mode (default: Process)
    pub snapshot_mode: SnapshotMode,
}

impl Default for TardisConvertConfig {
    fn default() -> Self {
        Self {
            buffer_size: 100_000_000,
            ss_buffer_size: 1_000_000,
            base_latency: 0,
            snapshot_mode: SnapshotMode::Process,
        }
    }
}

/// Writes events to an NPZ file (compressed numpy archive).
///
/// The events are stored under the key "data" in the archive,
/// matching the Python convention used by `np.savez_compressed(filename, data=events)`.
#[cfg(feature = "backtest")]
pub fn write_npz_file<P: AsRef<Path>>(events: &[Event], output_path: P) -> Result<(), String> {
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    let path = output_path.as_ref();
    println!("Saving to {:?}", path);

    let file = File::create(path).map_err(|e| format!("Failed to create output file: {}", e))?;

    let mut zip = ZipWriter::new(file);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .large_file(true);

    zip.start_file("data.npy", options)
        .map_err(|e| format!("Failed to start ZIP entry: {}", e))?;

    write_npy(&mut zip, events).map_err(|e| format!("Failed to write NPY data: {}", e))?;

    zip.finish()
        .map_err(|e| format!("Failed to finish ZIP: {}", e))?;

    println!("Saved {} events to {:?}", events.len(), path);
    Ok(())
}
