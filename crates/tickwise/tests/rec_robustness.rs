//! Deterministic randomized robustness sweep for the `.rec` reader.
//!
//! The real fuzzer is the cargo-fuzz target under `fuzz/`, which CI runs
//! on Linux with libFuzzer. This sweep is its portable little sibling: a
//! seeded generator applies thousands of random mutations to a valid
//! recording, on every platform, in normal `cargo test` runs. Nothing
//! here may panic; errors are the expected outcome.

use std::io::Cursor;
use tickwise::format::RecReader;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, SnapshotPolicy, StateDump};

const CASES: u64 = 5_000;

struct StubProbe {
    tick: u64,
}

impl DeterminismProbe for StubProbe {
    fn light_hash(&self) -> u64 {
        self.tick.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }
    fn full_hash(&self) -> u64 {
        self.tick.wrapping_mul(31)
    }
    fn state_dump(&self) -> StateDump {
        StateDump::empty()
    }
}

/// Local copy of the refsim LCG, small enough to duplicate in a test.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }
}

fn base_recording() -> Vec<u8> {
    let config = RecorderConfig {
        full_hash_interval: 25,
        snapshot: SnapshotPolicy::Every(50),
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::new(Vec::new(), config).unwrap();
    let mut probe = StubProbe { tick: 0 };
    for tick in 0..200u64 {
        probe.tick = tick;
        rec.record_tick(tick, &[(tick % 7) as u8, (tick % 3) as u8], &probe)
            .unwrap();
        if rec.wants_snapshot(tick) {
            rec.record_snapshot(tick, &[1, 2, 3, 4]).unwrap();
        }
    }
    rec.record_marker(100, "midpoint").unwrap();
    rec.finish().unwrap()
}

fn mutate(base: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut bytes = base.to_vec();
    match rng.below(5) {
        // Flip up to eight random bytes.
        0 => {
            for _ in 0..=rng.below(8) {
                let pos = rng.below(bytes.len() as u64) as usize;
                bytes[pos] ^= rng.next() as u8;
            }
        }
        // Truncate at a random point.
        1 => {
            let len = rng.below(bytes.len() as u64) as usize;
            bytes.truncate(len);
        }
        // Splice a random slice of the file over another position.
        2 => {
            let src = rng.below(bytes.len() as u64) as usize;
            let dst = rng.below(bytes.len() as u64) as usize;
            let len = rng.below(64) as usize;
            for i in 0..len {
                if src + i < bytes.len() && dst + i < bytes.len() {
                    bytes[dst + i] = bytes[src + i];
                }
            }
        }
        // Insert random garbage at a random point.
        3 => {
            let pos = rng.below(bytes.len() as u64) as usize;
            let garbage: Vec<u8> = (0..rng.below(32)).map(|_| rng.next() as u8).collect();
            bytes.splice(pos..pos, garbage);
        }
        // Replace the whole file with pure noise of a random size.
        _ => {
            let len = rng.below(512) as usize;
            bytes = (0..len).map(|_| rng.next() as u8).collect();
        }
    }
    bytes
}

#[test]
fn thousands_of_random_mutations_never_panic() {
    let base = base_recording();
    let mut rng = Rng(0x0DD_BA11);

    for case in 0..CASES {
        let mutated = mutate(&base, &mut rng);
        // The case number in the message makes any failure reproducible,
        // since the generator is fully deterministic.
        let outcome = std::panic::catch_unwind(|| {
            if let Ok(mut reader) = RecReader::open(Cursor::new(&mutated)) {
                if let Ok(chunks) = reader.chunks() {
                    for chunk in chunks {
                        let _ = chunk;
                    }
                }
                let _ = reader.read_index();
                let _ = reader.verify_checksum();
            }
        });
        assert!(outcome.is_ok(), "reader panicked on mutation case {case}");
    }
}
