use flate2::read::GzDecoder;
use hftbacktest::backtest::data::write_npy_header;
use hftbacktest::types::Event;
use std::fs::{remove_file, File};
use std::io::{copy, BufReader, BufWriter, Seek, SeekFrom, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use clap::Parser;

mod bybit;
mod converter;

use converter::Converter;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    exchange: String,

    #[arg(long)]
    input: String,

    #[arg(long, default_value = "test.npz")]
    output: String,

    #[arg(long, default_value_t = 5_000_000)]
    base_latency: i64,

    #[arg(long, default_value = "/tmp/")]
    temp_dir: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut temp_file = args.temp_dir;
    temp_file.push_str("temp.npy");
    let file = File::create(&temp_file)?;
    let mut writer = BufWriter::new(file);

    // This ordering may not be optimal for speed!
    let input = File::open(args.input.clone())?;
    let decoder = GzDecoder::new(input);
    let reader = BufReader::new(decoder);

    let mut converter = Converter::new(&*args.exchange, args.base_latency);

    write_npy_header::<BufWriter<File>, Event>(&mut writer, 0)?;

    // Actually do the work..
    println!("Converting {} to {}", args.input, &temp_file);
    let counter = converter.process_file(reader, &mut writer)?;
    println!("Created {} events", counter);

    writer.seek(SeekFrom::Start(0))?;
    write_npy_header::<BufWriter<File>, Event>(&mut writer, counter)?;
    writer.flush()?;

    let output = File::create(&args.output)?;
    let zip_writer = BufWriter::new(output);
    let mut zip = ZipWriter::new(zip_writer);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::DEFLATE)
        .compression_level(Some(9));

    zip.start_file("data.npy", options)?;

    println!("Compressing {} to {}", &temp_file, &args.output);
    let mut temp_read = BufReader::new(File::open(&temp_file)?);
    copy(&mut temp_read, &mut zip)?;
    zip.finish()?;

    println!("Removing {}", &temp_file);
    remove_file(&temp_file)?;

    Ok(())
}
