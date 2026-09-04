//! Replays a refsim recording made by record_demo and writes a .dump file.
//!
//! Usage: replay_demo <in.rec> <out.dump> --dump-at <tick> [--chaos <mode> [start_tick]]
//!
//! The world is rebuilt from the seed stored in the recording, the
//! recorded inputs drive it, and every live hash is verified against the
//! recording. Pass the same --chaos flags the recording was made with,
//! otherwise verification fails at the strike tick, which is itself a
//! useful demonstration.

use tickwise::{ReplayConfig, ReplayError, Replayer};
use tickwise_refsim::{ChaosConfig, PlayerInput, World, WorldConfig};

fn usage_and_exit() -> ! {
    eprintln!(
        "usage: replay_demo <in.rec> <out.dump> --dump-at <tick> [--chaos <mode> [start_tick]]"
    );
    std::process::exit(2);
}

fn decode(bytes: &[u8]) -> Vec<PlayerInput> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| PlayerInput {
            move_x: pair[0] as i8,
            move_y: pair[1] as i8,
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(in_path), Some(out_path)) = (args.first(), args.get(1)) else {
        usage_and_exit();
    };

    let mut dump_at: Option<u64> = None;
    let mut chaos: Option<ChaosConfig> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--dump-at" => {
                dump_at = args.get(i + 1).and_then(|s| s.parse().ok());
                if dump_at.is_none() {
                    eprintln!("--dump-at needs a tick number");
                    std::process::exit(2);
                }
                i += 2;
            }
            "--chaos" => {
                let Some(mode_name) = args.get(i + 1) else {
                    eprintln!("--chaos needs a mode");
                    std::process::exit(2);
                };
                let mode = match mode_name.parse() {
                    Ok(mode) => mode,
                    Err(err) => {
                        eprintln!("{err}");
                        std::process::exit(2);
                    }
                };
                let start_tick = args.get(i + 2).and_then(|s| s.parse().ok());
                chaos = Some(ChaosConfig {
                    mode,
                    start_tick: start_tick.unwrap_or(3000),
                });
                i += if start_tick.is_some() { 3 } else { 2 };
            }
            other => {
                eprintln!("unknown argument {other}");
                usage_and_exit();
            }
        }
    }
    let Some(dump_at) = dump_at else {
        usage_and_exit();
    };

    let mut rep = match Replayer::open(
        in_path,
        ReplayConfig {
            dump_at_ticks: vec![dump_at],
            verify_hashes: true,
            expected_input_format_id: Some(1),
        },
    ) {
        Ok(rep) => rep,
        Err(err) => {
            eprintln!("replay_demo: {err}");
            std::process::exit(2);
        }
    };

    let seed = rep.header().meta.rng_seed;
    let mut world = World::new(WorldConfig {
        seed,
        chaos,
        ..WorldConfig::default()
    });

    let mut verification_failure: Option<ReplayError> = None;
    while let Some(step) = rep.next_step() {
        world.step(&decode(step.inputs()));
        if let Err(err) = rep.after_tick(&world) {
            verification_failure = Some(err);
            break;
        }
    }

    let (first, last) = rep.tick_range();
    match rep.finish(out_path) {
        Ok(()) => println!("wrote {out_path} with the state at tick {dump_at}"),
        Err(err) => {
            eprintln!("replay_demo: {err}");
            std::process::exit(2);
        }
    }

    match verification_failure {
        None => println!("replayed ticks {first} to {last}, every hash matched the recording"),
        Some(err) => {
            eprintln!("replay_demo: {err}");
            eprintln!("hint: pass the same --chaos flags the recording was made with");
            std::process::exit(1);
        }
    }
}
