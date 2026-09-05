//! Records a 6000 tick refsim session into a .rec file.
//!
//! Usage: record_demo <out.rec> [--chaos <mode> [start_tick]]
//!
//! Chaos modes: float-drift, hashmap-iter, stale-value, time-dependent.
//! Record one clean session and one chaotic one, then watch tickwise
//! compare find the strike tick.

use tickwise::format::SnapshotPolicy;
use tickwise::{Recorder, RecorderConfig, SessionMeta};
use tickwise_refsim::{ChaosConfig, Lcg, PlayerInput, World, WorldConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out_path) = args.first() else {
        eprintln!("usage: record_demo <out.rec> [--chaos <mode> [start_tick]]");
        std::process::exit(2);
    };

    let chaos = match args.get(1).map(String::as_str) {
        Some("--chaos") => {
            let Some(mode_name) = args.get(2) else {
                eprintln!(
                    "--chaos needs a mode: float-drift, hashmap-iter, stale-value, time-dependent"
                );
                std::process::exit(2);
            };
            let mode = match mode_name.parse() {
                Ok(mode) => mode,
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(2);
                }
            };
            let start_tick = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3000u64);
            Some(ChaosConfig { mode, start_tick })
        }
        Some(other) => {
            eprintln!("unknown argument {other}");
            std::process::exit(2);
        }
        None => None,
    };

    let seed = 0x0DD_BA11u64;
    let config = RecorderConfig {
        session_meta: SessionMeta {
            game_id: "tickwise-refsim".to_string(),
            build_hash: "m2-dev".to_string(),
            platform: std::env::consts::OS.to_string(),
            tick_rate: 60,
            rng_seed: seed,
            created_at: 1_756_400_000,
        },
        full_hash_interval: 300,
        snapshot: SnapshotPolicy::Every(1800),
        input_format_id: 1,
        ..RecorderConfig::default()
    };
    let mut world = World::new(WorldConfig {
        seed,
        chaos,
        ..WorldConfig::default()
    });
    let mut input_rng = Lcg::new(9001);
    let mut rec = Recorder::create(out_path, config).unwrap();
    for tick in 0..6000u64 {
        let inputs: Vec<PlayerInput> = (0..2)
            .map(|_| PlayerInput {
                move_x: (input_rng.next_u64() % 3) as i8 - 1,
                move_y: (input_rng.next_u64() % 3) as i8 - 1,
            })
            .collect();
        let bytes: Vec<u8> = inputs
            .iter()
            .flat_map(|i| [i.move_x as u8, i.move_y as u8])
            .collect();
        world.step(&inputs);
        rec.record_tick(tick, &bytes, &world).unwrap();
        if rec.wants_snapshot(tick) {
            rec.record_snapshot(tick, b"demo snapshot bytes").unwrap();
        }
    }
    rec.record_marker(3000, "halfway point").unwrap();
    rec.finish().unwrap();
    match chaos {
        Some(c) => println!(
            "recorded 6000 ticks with {} chaos from tick {}",
            c.mode, c.start_tick
        ),
        None => println!("recorded 6000 clean ticks"),
    }
}
