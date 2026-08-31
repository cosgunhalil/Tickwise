//! End-to-end recorder tests: record a session, read the `.rec` bytes
//! back, and verify every expectation about the chunk stream.

use std::io::Cursor;
use tickwise::format::{Chunk, RecReader, SnapshotPolicy};
use tickwise::{DeterminismProbe, RecordError, Recorder, RecorderConfig, StateDump};

/// A probe whose hashes are pure functions of a counter, so every
/// recorded value is predictable in assertions.
struct CountingProbe {
    frame: u64,
}

impl DeterminismProbe for CountingProbe {
    fn light_hash(&self) -> u64 {
        self.frame.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    fn full_hash(&self) -> u64 {
        self.frame.wrapping_mul(31).wrapping_add(7)
    }

    fn state_dump(&self) -> StateDump {
        StateDump::empty()
    }
}

const TICKS: u64 = 150;
const FULL_INTERVAL: u32 = 50;

fn record_session() -> Vec<u8> {
    let config = RecorderConfig {
        full_hash_interval: FULL_INTERVAL,
        snapshot: SnapshotPolicy::Every(100),
        input_format_id: 7,
        ..RecorderConfig::default()
    };
    let mut probe = CountingProbe { frame: 0 };
    let mut rec = Recorder::new(Vec::new(), config).unwrap();

    for tick in 0..TICKS {
        probe.frame += 1;
        let inputs = [tick as u8, (tick / 2) as u8];
        rec.record_tick(tick, &inputs, &probe).unwrap();
        if rec.wants_snapshot(tick) {
            rec.record_snapshot(tick, b"snapshot-bytes").unwrap();
        }
    }
    rec.record_marker(42, "round start").unwrap();
    rec.finish().unwrap()
}

#[test]
fn recorded_session_reads_back_with_the_expected_structure() {
    let bytes = record_session();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();

    assert_eq!(reader.tick_count(), TICKS);
    assert_eq!(reader.header().config.input_format_id, 7);
    assert_eq!(reader.header().config.full_hash_interval, FULL_INTERVAL);

    let chunks: Vec<Chunk> = reader
        .chunks()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let mut input_ticks = Vec::new();
    let mut light_hashes = Vec::new();
    let mut full_hashes = Vec::new();
    let mut snapshots = Vec::new();
    let mut markers = Vec::new();

    for chunk in &chunks {
        match chunk {
            Chunk::InputFrame { tick, data } => {
                assert_eq!(data, &vec![*tick as u8, (*tick / 2) as u8]);
                input_ticks.push(*tick);
            }
            Chunk::LightHashBatch { first_tick, hashes } => {
                assert_eq!(*first_tick, light_hashes.len() as u64);
                light_hashes.extend_from_slice(hashes);
            }
            Chunk::FullHash { tick, hash } => full_hashes.push((*tick, *hash)),
            Chunk::Snapshot { tick, data } => {
                assert_eq!(data, b"snapshot-bytes");
                snapshots.push(*tick);
            }
            Chunk::Marker { tick, label } => markers.push((*tick, label.clone())),
            Chunk::Unknown { .. } => panic!("recorder wrote an unknown chunk"),
        }
    }

    // Every tick has its input frame, in order.
    assert_eq!(input_ticks, (0..TICKS).collect::<Vec<_>>());

    // Every tick has its light hash, batched as 64 + 64 + 22.
    assert_eq!(light_hashes.len() as u64, TICKS);
    let batch_sizes: Vec<usize> = chunks
        .iter()
        .filter_map(|c| match c {
            Chunk::LightHashBatch { hashes, .. } => Some(hashes.len()),
            _ => None,
        })
        .collect();
    assert_eq!(batch_sizes, vec![64, 64, 22]);
    for (tick, hash) in light_hashes.iter().enumerate() {
        let frame = tick as u64 + 1;
        assert_eq!(*hash, frame.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    }

    // Full hashes land exactly on the interval.
    assert_eq!(
        full_hashes.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
        vec![0, 50, 100]
    );
    for (tick, hash) in &full_hashes {
        let frame = tick + 1;
        assert_eq!(*hash, frame.wrapping_mul(31).wrapping_add(7));
    }

    // Snapshots follow the Every(100) policy.
    assert_eq!(snapshots, vec![0, 100]);

    // The marker survived.
    assert_eq!(markers, vec![(42, "round start".to_string())]);

    reader.verify_checksum().unwrap();
}

#[test]
fn constant_inputs_collapse_to_a_single_frame() {
    let probe = CountingProbe { frame: 0 };
    let mut rec = Recorder::new(Vec::new(), RecorderConfig::default()).unwrap();
    for tick in 0..200 {
        rec.record_tick(tick, &[3, 7], &probe).unwrap();
    }
    let bytes = rec.finish().unwrap();

    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    let frames: Vec<Chunk> = reader
        .chunks()
        .unwrap()
        .filter_map(Result::ok)
        .filter(|c| matches!(c, Chunk::InputFrame { .. }))
        .collect();
    assert_eq!(
        frames,
        vec![Chunk::InputFrame {
            tick: 0,
            data: vec![3, 7],
        }]
    );
}

#[test]
fn changed_inputs_reconstruct_exactly() {
    // Inputs change at ticks 0, 10, and 25, including a change to empty.
    let schedule = |tick: u64| -> Vec<u8> {
        if tick < 10 {
            vec![1]
        } else if tick < 25 {
            Vec::new()
        } else {
            vec![9, 9]
        }
    };

    let probe = CountingProbe { frame: 0 };
    let mut rec = Recorder::new(Vec::new(), RecorderConfig::default()).unwrap();
    for tick in 0..40 {
        rec.record_tick(tick, &schedule(tick), &probe).unwrap();
    }
    let bytes = rec.finish().unwrap();

    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    let frames: Vec<(u64, Vec<u8>)> = reader
        .chunks()
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|c| match c {
            Chunk::InputFrame { tick, data } => Some((tick, data)),
            _ => None,
        })
        .collect();
    assert_eq!(
        frames,
        vec![(0, vec![1]), (10, Vec::new()), (25, vec![9, 9])]
    );

    // Expand with the format rule: a frame applies until the next frame.
    for tick in 0..40u64 {
        let expanded = frames
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .map(|(_, d)| d.clone())
            .unwrap();
        assert_eq!(expanded, schedule(tick), "wrong inputs at tick {tick}");
    }
}

#[test]
fn non_sequential_ticks_are_rejected() {
    let probe = CountingProbe { frame: 0 };
    let mut rec = Recorder::new(Vec::new(), RecorderConfig::default()).unwrap();
    rec.record_tick(0, &[], &probe).unwrap();
    rec.record_tick(1, &[], &probe).unwrap();
    let err = rec.record_tick(5, &[], &probe).unwrap_err();
    assert!(matches!(
        err,
        RecordError::NonSequentialTick {
            expected: 2,
            got: 5
        }
    ));
}

#[test]
fn recording_may_start_at_any_tick() {
    let probe = CountingProbe { frame: 0 };
    let mut rec = Recorder::new(Vec::new(), RecorderConfig::default()).unwrap();
    rec.record_tick(4000, &[], &probe).unwrap();
    rec.record_tick(4001, &[], &probe).unwrap();
    let bytes = rec.finish().unwrap();

    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.tick_count(), 2);
    let first = reader.chunks().unwrap().next().unwrap().unwrap();
    assert_eq!(first.first_tick(), 4000);
}

#[test]
fn empty_session_finishes_cleanly() {
    let rec = Recorder::new(Vec::new(), RecorderConfig::default()).unwrap();
    let bytes = rec.finish().unwrap();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.tick_count(), 0);
    assert_eq!(reader.chunks().unwrap().count(), 0);
}
