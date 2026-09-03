//! The callback path: a hand-written `DeterminismProbe`, then the whole
//! two-pass workflow in one program with no files and no CLI.
//!
//! Run with `cargo run -p tickwise --example callback_probe`.
//!
//! This is the path for performance-sensitive code and for the future
//! FFI bridge: you decide exactly what each hash covers and what the dump
//! lists. The serde_probe example does the same job with less code.

use std::io::Cursor;
use tickwise::compare::{Outcome, first_divergence_from};
use tickwise::diff::{FloatPolicy, diff_dumps};
use tickwise::format::RecReader;
use tickwise::{
    DeterminismProbe, Recorder, RecorderConfig, ReplayConfig, Replayer, StateDump, Value,
};

/// A tiny deterministic simulation: two ships drifting in a box.
#[derive(Clone)]
struct Sim {
    tick: u64,
    positions: [(f32, f32); 2],
    velocities: [(f32, f32); 2],
    score: u64,
    /// The bug switch. A real game would not have one of these; this
    /// stands in for a desync introduced by some code change.
    buggy: bool,
}

/// Inputs are your own encoding. Here one byte per ship: bit 0 thrust,
/// bit 1 turn.
fn inputs_for(tick: u64) -> [u8; 2] {
    [((tick / 10) % 4) as u8, ((tick / 7) % 4) as u8]
}

impl Sim {
    fn new(buggy: bool) -> Self {
        Self {
            tick: 0,
            positions: [(10.0, 10.0), (90.0, 90.0)],
            velocities: [(0.0, 0.0); 2],
            score: 0,
            buggy,
        }
    }

    fn step(&mut self, inputs: &[u8]) {
        self.tick += 1;
        for (i, input) in inputs.iter().enumerate().take(2) {
            let (vx, vy) = &mut self.velocities[i];
            if input & 1 != 0 {
                *vx += 0.5;
            }
            if input & 2 != 0 {
                *vy -= 0.25;
            }
            *vx *= 0.9;
            *vy *= 0.9;
            let (x, y) = &mut self.positions[i];
            *x = (*x + *vx).clamp(0.0, 100.0);
            *y = (*y + *vy).clamp(0.0, 100.0);
            if *x == 0.0 || *x == 100.0 {
                self.score += 1;
            }
        }
        // The defect: from tick 300 on, the buggy build applies friction
        // once more to ship 0. Sub-epsilon at first, then it compounds.
        if self.buggy && self.tick >= 300 {
            self.velocities[0].0 *= 0.999_999;
        }
    }
}

impl DeterminismProbe for Sim {
    /// Cheap digest: the tick, the score, and ship positions as bits.
    /// Velocities are left out on purpose, so this example also shows a
    /// blind spot being reported and then closed by the full hash.
    fn light_hash(&self) -> u64 {
        let mut h = self.tick ^ self.score.rotate_left(32);
        for (x, y) in &self.positions {
            h = h.rotate_left(7) ^ u64::from(x.to_bits()) ^ (u64::from(y.to_bits()) << 32);
        }
        h
    }

    /// Everything that influences a future tick.
    fn full_hash(&self) -> u64 {
        let mut h = self.light_hash();
        for (vx, vy) in &self.velocities {
            h = h.rotate_left(11) ^ u64::from(vx.to_bits()) ^ (u64::from(vy.to_bits()) << 32);
        }
        h
    }

    /// The same set as the full hash, field by field, for the diff.
    fn state_dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        dump.insert("tick", self.tick);
        dump.insert("score", self.score);
        dump.insert("ships", Value::Len(2));
        for i in 0..2 {
            dump.insert(format!("ships[{i}].position.x"), self.positions[i].0);
            dump.insert(format!("ships[{i}].position.y"), self.positions[i].1);
            dump.insert(format!("ships[{i}].velocity.x"), self.velocities[i].0);
            dump.insert(format!("ships[{i}].velocity.y"), self.velocities[i].1);
        }
        dump
    }
}

const TICKS: u64 = 600;

/// Pass 1: play a session and record it.
fn record(buggy: bool) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut sim = Sim::new(buggy);
    let mut rec = Recorder::new(
        Vec::new(),
        RecorderConfig {
            full_hash_interval: 100,
            input_format_id: 1,
            ..RecorderConfig::default()
        },
    )?;
    for tick in 0..TICKS {
        let inputs = inputs_for(tick);
        sim.step(&inputs);
        rec.record_tick(tick, &inputs, &sim)?;
    }
    Ok(rec.finish()?)
}

/// Pass 2: replay a recording and dump the state at one tick.
fn replay(recording: &[u8], buggy: bool, at: u64) -> Result<StateDump, Box<dyn std::error::Error>> {
    let mut reader = RecReader::open(Cursor::new(recording))?;
    let mut rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            dump_at_ticks: vec![at],
            verify_hashes: true,
            expected_input_format_id: Some(1),
        },
    )?;
    let mut sim = Sim::new(buggy);
    while let Some(step) = rep.next_step() {
        sim.step(step.inputs());
        rep.after_tick(&sim)?;
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
