//! Records a 6000 tick refsim session into a .rec file.
//!
//! Usage: record_demo <out.rec> [--chaos <mode> [start_tick]] [--dump-every <ticks>]
//!
//! Chaos modes: float-drift, hashmap-iter, stale-value, time-dependent.
//! Record one clean session and one chaotic one, then watch tickwise
//! compare find the strike tick. With --dump-every, both recordings carry
//! state dumps and tickwise diff reads them directly, no replay needed.

use tickwise::format::SnapshotPolicy;
use tickwise::{Recorder, RecorderConfig, SessionMeta};
use tickwise_refsim::{ChaosConfig, Lcg, PlayerInput, World, WorldConfig};

const USAGE: &str =
    "usage: record_demo <out.rec> [--chaos <mode> [start_tick]] [--dump-every <ticks>]";

/// The wall clock as unix seconds, for the header's `created_at`. It is
/// metadata only, never compared, so it is the one place a demo may
/// read the clock.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out_path) = args.first() else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };

    let mut chaos = None;
    let mut dump_interval = 0u32;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--chaos" => {
                let Some(mode_name) = args.get(index + 1) else {
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
                index += 2;
                // An optional start tick follows the mode.
                let start_tick = match args.get(index).and_then(|s| s.parse().ok()) {
                    Some(tick) => {
                        index += 1;
                        tick
                    }
                    None => 3000u64,
                };
                chaos = Some(ChaosConfig { mode, start_tick });
            }
            "--dump-every" => {
                let Some(value) = args.get(index + 1).and_then(|s| s.parse::<u32>().ok()) else {
                    eprintln!("--dump-every needs a tick count");
                    std::process::exit(2);
                };
                dump_interval = value;
                index += 2;
            }
            other => {
                eprintln!("unknown argument {other}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    let seed = 0x0DD_BA11u64;
    let config = RecorderConfig {
        session_meta: SessionMeta {
            game_id: "tickwise-refsim".to_string(),
            build_hash: "m2-dev".to_string(),
            platform: std::env::consts::OS.to_string(),
            tick_rate: 60,
            rng_seed: seed,
            created_at: unix_now(),
        },
        full_hash_interval: 300,
        snapshot: SnapshotPolicy::Every(1800),
        input_format_id: 1,
        dump_interval,
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

    let dumps = if dump_interval > 0 {
        format!(", with a state dump every {dump_interval} ticks")
    } else {
        String::new()
    };
    match chaos {
        Some(c) => println!(
            "recorded 6000 ticks with {} chaos from tick {}{dumps}",
            c.mode, c.start_tick
        ),
        None => println!("recorded 6000 clean ticks{dumps}"),
    }
}
