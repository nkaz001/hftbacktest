use criterion::*;
use hftbacktest::backtest::data::{read_npy_file, write_npy, write_npz};
use hftbacktest::{
    backtest::data::{read_npz_file, Data},
    types::Event,
};
use std::fs::File;
use std::time::Duration;
use std::{fs, ops::Index};

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("format-throughput");
    let events: Vec<Event> = (0..1000_000)
        .map(|id| Event {
            ev: id,
            exch_ts: 1_000_000,
            local_ts: 1_000_001,
            px: 1.0,
            qty: 1.0,
            order_id: 1,
            ival: 100,
            fval: 100.0,
        })
        .collect();

    let mut npy_file = File::create("bench.npy").expect("couldn't create bench.npy");
    let mut npz_file = File::create("bench.npz").expect("couldn't create bench.npz");

    write_npy(&mut npy_file, &events).expect("failed to generate npy file");
    write_npz(&mut npz_file, &events).expect("failed to generate npz file");

    group.throughput(Throughput::Elements(events.len() as u64));
    group.warm_up_time(Duration::from_secs(10));
    group.bench_function("npz", |b| {
        b.iter(|| benchmark_npz_file())
    });
    group.bench_function("npy", |b| b.iter(|| benchmark_npy_file()));
    group.finish();

    let _ = fs::remove_file("bench.npy");
    let _ = fs::remove_file("bench.npz");
}

#[inline]
fn read_all(data: Data<Event>) {
    for index in 0..data.len() {
        black_box(data.index(index));
    }
}

fn benchmark_npz_file() {
    let data = read_npz_file::<Event>("bench.npz", "data").unwrap();
    read_all(data);
}

fn benchmark_npy_file() {
    let data = read_npy_file::<Event>("bench.npy").unwrap();
    read_all(data);
}

criterion_group!(benches, bench);
criterion_main!(benches);
