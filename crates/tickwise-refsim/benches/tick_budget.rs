//! The light hash budget, measured.
//!
//! The rule is that `light_hash` costs below 1 percent of a tick. This
//! bench measures a refsim tick and each probe callback at two world sizes
//! so the ratio, and how it scales, is a number rather than a hope.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig};
use tickwise_refsim::{PlayerInput, World, WorldConfig};

const INPUTS: [PlayerInput; 2] = [
    PlayerInput {
        move_x: 1,
        move_y: 0,
    },
    PlayerInput {
        move_x: 0,
        move_y: -1,
    },
];

fn bench_world(c: &mut Criterion, label: &str, ball_count: u32) {
    let config = WorldConfig {
        ball_count,
        ..WorldConfig::default()
    };
    let mut group = c.benchmark_group(label);

    let mut world = World::new(config.clone());
    group.bench_function("step", |b| b.iter(|| world.step(black_box(&INPUTS))));

    let world = World::new(config.clone());
    group.bench_function("light_hash", |b| b.iter(|| black_box(world.light_hash())));
    group.bench_function("full_hash", |b| b.iter(|| black_box(world.full_hash())));
    group.bench_function("state_dump", |b| b.iter(|| black_box(world.state_dump())));

    // The whole per-tick cost a user pays: simulate, then record into a
    // sink that discards bytes so memory stays flat across iterations.
    let mut world = World::new(config);
    let mut rec = Recorder::new(std::io::sink(), RecorderConfig::default()).unwrap();
    let mut tick = 0u64;
    group.bench_function("step + record_tick", |b| {
        b.iter(|| {
            world.step(black_box(&INPUTS));
            rec.record_tick(tick, &[1, 0, 0, 255], &world).unwrap();
            tick += 1;
        })
    });

    group.finish();
}

fn benches(c: &mut Criterion) {
    bench_world(c, "refsim 8 balls", 8);
    bench_world(c, "refsim 1000 balls", 1000);
}

criterion_group! {
    name = tick_budget;
    config = Criterion::default()
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(2));
    targets = benches
}
criterion_main!(tick_budget);
