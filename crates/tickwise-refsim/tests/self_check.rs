//! The self-check workflow, the lowest-friction entry point into
//! Tickwise: reproduce your own session from its recording and compare.
//!
//! This test replays the inputs stored in a `.rec` file through a fresh
//! world and proves the second recording is identical to the first. It
//! exercises the whole loop: input encoding, repeat suppression
//! expansion, simulation determinism, and the compare verdict.

use std::io::Cursor;
use tickwise::compare::{Outcome, first_divergence_from};
use tickwise::format::{Chunk, RecReader};
use tickwise::{Recorder, RecorderConfig, SessionMeta};
use tickwise_refsim::{Lcg, PlayerInput, World, WorldConfig};

const TICKS: u64 = 1_000;
const SEED: u64 = 0x0DD_BA11;

fn recorder() -> Recorder<Vec<u8>> {
    let config = RecorderConfig {
        session_meta: SessionMeta {
            game_id: "refsim-self-check".to_string(),
            rng_seed: SEED,
            ..SessionMeta::default()
        },
        full_hash_interval: 100,
        input_format_id: 1,
        ..RecorderConfig::default()
    };
    Recorder::new(Vec::new(), config).unwrap()
}

fn encode(inputs: &[PlayerInput]) -> Vec<u8> {
    inputs
        .iter()
        .flat_map(|i| [i.move_x as u8, i.move_y as u8])
        .collect()
}

fn decode(bytes: &[u8]) -> Vec<PlayerInput> {
    bytes
        .chunks_exact(2)
        .map(|pair| PlayerInput {
            move_x: pair[0] as i8,
            move_y: pair[1] as i8,
        })
        .collect()
}

#[test]
fn a_session_rebuilt_from_its_own_recording_is_identical() {
    // Session A: LCG-driven inputs, the original play session.
    let mut world = World::new(WorldConfig {
        seed: SEED,
        ..WorldConfig::default()
    });
    let mut input_rng = Lcg::new(777);
    let mut rec = recorder();
    for tick in 0..TICKS {
        let inputs: Vec<PlayerInput> = (0..2)
            .map(|_| PlayerInput {
                move_x: (input_rng.next_u64() % 3) as i8 - 1,
                move_y: (input_rng.next_u64() % 3) as i8 - 1,
            })
            .collect();
        world.step(&inputs);
        rec.record_tick(tick, &encode(&inputs), &world).unwrap();
    }
    let first = rec.finish().unwrap();

    // Pull the input frames back out of the recording.
    let mut reader = RecReader::open(Cursor::new(&first)).unwrap();
    let frames: Vec<(u64, Vec<u8>)> = reader
        .chunks()
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|chunk| match chunk {
            Chunk::InputFrame { tick, data } => Some((tick, data)),
            _ => None,
        })
        .collect();
    assert!(!frames.is_empty());

    // Session B: a fresh world driven purely by the recorded inputs,
    // expanded with the frame persistence rule.
    let mut world = World::new(WorldConfig {
        seed: SEED,
        ..WorldConfig::default()
    });
    let mut rec = recorder();
    for tick in 0..TICKS {
        let bytes = frames
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .map(|(_, data)| data.clone())
            .unwrap_or_default();
        world.step(&decode(&bytes));
        rec.record_tick(tick, &bytes, &world).unwrap();
    }
    let second = rec.finish().unwrap();

    // The verdict: the rebuilt session must be indistinguishable.
    let mut ra = RecReader::open(Cursor::new(&first)).unwrap();
    let mut rb = RecReader::open(Cursor::new(&second)).unwrap();
    let report = first_divergence_from(&mut ra, &mut rb).unwrap();
    assert!(report.warnings.is_empty());
    assert_eq!(
        report.outcome,
        Outcome::Identical {
            ticks_compared: TICKS,
            extra_ticks_a: 0,
            extra_ticks_b: 0,
        }
    );
}
