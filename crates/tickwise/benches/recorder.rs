//! Recorder overhead and offline comparison speed.
//!
//! `record_tick` with a trivial probe isolates what Tickwise itself costs
//! per tick, independent of any simulation. `compare` over two long
//! recordings shows what "first divergence in milliseconds" means.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::io::Cursor;
use tickwise::compare::first_divergence_from;
use tickwise::format::RecReader;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, StateDump};

struct TrivialProbe(u64);

impl DeterminismProbe for TrivialProbe {
    fn light_hash(&self) -> u64 {
        self.0
    }
    fn full_hash(&self) -> u64 {
        self.0.wrapping_mul(31)
    }
    fn state_dump(&self) -> StateDump {
        StateDump::empty()
    }
}

fn record(ticks: u64, diverge_at: Option<u64>) -> Vec<u8> {
    let mut rec = Recorder::new(Vec::new(), RecorderConfig::default()).unwrap();
    for tick in 0..ticks {
        let value = match diverge_at {
            Some(at) if tick >= at => tick + 1,
            _ => tick,
        };
        rec.record_tick(tick, &[(tick % 4) as u8, 0], &TrivialProbe(value))
            .unwrap();
    }
    rec.finish().unwrap()
}

fn benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("recorder");

    let mut rec = Recorder::new(std::io::sink(), RecorderConfig::default()).unwrap();
    let mut tick = 0u64;
    group.bench_function("record_tick, constant inputs", |b| {
        b.iter(|| {
            rec.record_tick(tick, black_box(&[1, 0, 0, 0]), &TrivialProbe(tick))
                .unwrap();
            tick += 1;
        })
    });

    let mut rec = Recorder::new(std::io::sink(), RecorderConfig::default()).unwrap();
    let mut tick = 0u64;
    group.bench_function("record_tick, inputs change every tick", |b| {
        b.iter(|| {
            let inputs = [(tick % 251) as u8, (tick % 7) as u8, 0, 0];
            rec.record_tick(tick, black_box(&inputs), &TrivialProbe(tick))
                .unwrap();
            tick += 1;
        })
    });
    group.finish();

    let ticks = 100_000;
    let a = record(ticks, None);
    let b = record(ticks, Some(ticks - 1));
    let mut group = c.benchmark_group("compare 100k ticks");
    group.bench_function("first_divergence near the end", |bench| {
        bench.iter(|| {
            let mut ra = RecReader::open(Cursor::new(&a)).unwrap();
            let mut rb = RecReader::open(Cursor::new(&b)).unwrap();
            black_box(first_divergence_from(&mut ra, &mut rb).unwrap())
        })
    });
    group.finish();
}

criterion_group! {
    name = recorder;
    config = Criterion::default()
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(2));
    targets = benches
}
criterion_main!(recorder);
