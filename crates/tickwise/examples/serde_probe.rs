//! The serde path: a `#[derive(Serialize)]` state, an automatic probe, and
//! typed inputs. Same two-pass workflow as the callback_probe example,
//! with the probe and the dump written for you.
//!
//! Run with `cargo run -p tickwise --features serde --example serde_probe`.

use serde::{Deserialize, Serialize};
use std::io::Cursor;
use tickwise::compare::{Outcome, first_divergence_from};
use tickwise::diff::{FloatPolicy, diff_dumps};
use tickwise::format::RecReader;
use tickwise::serde_probe::{HashAlgo, SerdeProbe, format_id};
use tickwise::{Recorder, RecorderConfig, ReplayConfig, Replayer, StateDump};

/// Your input type, any Serialize + Deserialize value.
#[derive(Serialize, Deserialize, Clone, Copy)]
struct Input {
    thrust: bool,
    turn: i8,
}

/// Your state type. Deriving Serialize is the whole integration.
#[derive(Serialize, Clone)]
struct Ship {
    position: (f32, f32),
    velocity: (f32, f32),
}

#[derive(Serialize, Clone)]
struct Sim {
    tick: u64,
    score: u64,
    ships: Vec<Ship>,
    /// Rendering state has no business in a hash or a dump.
    #[serde(skip)]
    last_frame_ms: f32,
    #[serde(skip)]
    buggy: bool,
}

/// The light view: a small struct of the desync-critical fields, so the
/// per-tick hash stays within budget while the full hash covers all.
#[derive(Serialize)]
struct LightView {
    tick: u64,
    score: u64,
    ship_count: usize,
}

fn input_for(tick: u64) -> Input {
    Input {
        thrust: (tick / 10) % 2 == 0,
        turn: ((tick / 7) % 3) as i8 - 1,
    }
}

impl Sim {
    fn new(buggy: bool) -> Self {
        Self {
            tick: 0,
            score: 0,
            ships: vec![
                Ship {
                    position: (10.0, 10.0),
                    velocity: (0.0, 0.0),
                },
                Ship {
                    position: (90.0, 90.0),
                    velocity: (0.0, 0.0),
                },
            ],
            last_frame_ms: 0.0,
            buggy,
        }
    }

    fn step(&mut self, input: Input) {
        self.tick += 1;
        self.last_frame_ms = 16.7; // pretend render timing, never hashed
        for ship in &mut self.ships {
            if input.thrust {
                ship.velocity.0 += 0.5;
            }
            ship.velocity.1 += f32::from(input.turn) * 0.25;
            ship.velocity.0 *= 0.9;
            ship.velocity.1 *= 0.9;
            ship.position.0 = (ship.position.0 + ship.velocity.0).clamp(0.0, 100.0);
            ship.position.1 = (ship.position.1 + ship.velocity.1).clamp(0.0, 100.0);
            if ship.position.0 == 100.0 {
                self.score += 1;
            }
        }
        // The defect: a spawn that only the buggy build performs, so the
        // ship count differs. That is a structural difference.
        if self.buggy && self.tick == 250 {
            self.ships.push(Ship {
                position: (50.0, 50.0),
                velocity: (0.0, 0.0),
            });
        }
    }

    fn light_view(&self) -> LightView {
        LightView {
            tick: self.tick,
            score: self.score,
            ship_count: self.ships.len(),
        }
    }
}

const TICKS: u64 = 600;
const INPUT_LABEL: &str = "serde example Input v1";

fn config() -> RecorderConfig {
    RecorderConfig {
        full_hash_interval: 100,
        hash_algo_id: HashAlgo::Xxh3.id(),
        input_format_id: format_id(INPUT_LABEL),
        ..RecorderConfig::default()
    }
}

fn record(buggy: bool) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut sim = Sim::new(buggy);
    let mut rec = Recorder::new(Vec::new(), config())?;
    for tick in 0..TICKS {
        let input = input_for(tick);
        sim.step(input);
        let view = sim.light_view();
        rec.record_tick_typed(tick, &input, &SerdeProbe::with_light(&sim, &view))?;
    }
    Ok(rec.finish()?)
}

fn replay(recording: &[u8], buggy: bool, at: u64) -> Result<StateDump, Box<dyn std::error::Error>> {
    let mut reader = RecReader::open(Cursor::new(recording))?;
    let mut rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            dump_at_ticks: vec![at],
            verify_hashes: true,
            expected_input_format_id: Some(format_id(INPUT_LABEL)),
        },
    )?;
    let mut sim = Sim::new(buggy);
    while let Some(step) = rep.next_step() {
        let input: Input = step.inputs_typed()?;
        sim.step(input);
        let view = sim.light_view();
        rep.after_tick(&SerdeProbe::with_light(&sim, &view))?;
    }
    Ok(rep.into_dumps()?.remove(0).1)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clean = record(false)?;
    let buggy = record(true)?;
    println!(
        "recorded two sessions of {TICKS} ticks, {} bytes each",
        clean.len()
    );

    let mut a = RecReader::open(Cursor::new(&clean))?;
    let mut b = RecReader::open(Cursor::new(&buggy))?;
    let report = first_divergence_from(&mut a, &mut b)?;
    println!("compare: {report}");

    let Outcome::Diverged(divergence) = report.outcome else {
        println!("no divergence, nothing to diff");
        return Ok(());
    };

    let dump_clean = replay(&clean, false, divergence.tick)?;
    let dump_buggy = replay(&buggy, true, divergence.tick)?;
    let diff = diff_dumps(
        divergence.tick,
        &dump_clean,
        &dump_buggy,
        &FloatPolicy::default(),
    );
    let count = diff.differences.len();
    let noun = if count == 1 {
        "difference"
    } else {
        "differences"
    };
    println!(
        "diff at tick {}: {count} {noun} over {} fields",
        diff.tick, diff.fields_compared
    );
    for difference in &diff.differences {
        println!("  {difference}");
    }
    Ok(())
}
