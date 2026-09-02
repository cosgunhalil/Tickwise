//! Behavioral tests for the replayer: input expansion, hash verification,
//! dump capture, snapshot lookup, protocol enforcement, and the input
//! format check from decision #11.

use std::io::Cursor;
use tickwise::compare::HashKind;
use tickwise::format::{Chunk, RecReader, SnapshotPolicy};
use tickwise::{
    DeterminismProbe, Recorder, RecorderConfig, ReplayConfig, ReplayError, Replayer, StateDump,
};

/// Hashes are pure functions of the tick, so a replaying probe can
/// reproduce them exactly, or deliberately not.
struct TickProbe {
    tick: u64,
    lie_at: Option<u64>,
}

impl DeterminismProbe for TickProbe {
    fn light_hash(&self) -> u64 {
        let base = self.tick.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        if self.lie_at == Some(self.tick) {
            !base
        } else {
            base
        }
    }
    fn full_hash(&self) -> u64 {
        self.tick.wrapping_mul(31).wrapping_add(7)
    }
    fn state_dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        dump.insert("tick", self.tick);
        dump
    }
}

/// Inputs change at ticks 0, 10, and 25, including a change to empty.
fn inputs_for(tick: u64) -> Vec<u8> {
    if tick < 10 {
        vec![1]
    } else if tick < 25 {
        Vec::new()
    } else {
        vec![9, 9]
    }
}

fn record(ticks: std::ops::Range<u64>, input_format_id: u64) -> Vec<u8> {
    let config = RecorderConfig {
        full_hash_interval: 20,
        snapshot: SnapshotPolicy::Every(30),
        input_format_id,
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::new(Vec::new(), config).unwrap();
    let mut probe = TickProbe {
        tick: 0,
        lie_at: None,
    };
    for tick in ticks {
        probe.tick = tick;
        rec.record_tick(tick, &inputs_for(tick), &probe).unwrap();
        if rec.wants_snapshot(tick) {
            rec.record_snapshot(tick, format!("state@{tick}").as_bytes())
                .unwrap();
        }
    }
    rec.finish().unwrap()
}

fn replayer(bytes: &[u8], config: ReplayConfig) -> Result<Replayer, ReplayError> {
    let mut reader = RecReader::open(Cursor::new(bytes)).unwrap();
    Replayer::from_reader(&mut reader, config)
}

#[test]
fn full_replay_verifies_expands_inputs_and_dumps_at_requested_ticks() {
    let bytes = record(0..100, 7);
    let mut rep = replayer(
        &bytes,
        ReplayConfig {
            dump_at_ticks: vec![42, 7],
            verify_hashes: true,
            expected_input_format_id: Some(7),
        },
    )
    .unwrap();
    assert_eq!(rep.tick_range(), (0, 99));

    let mut probe = TickProbe {
        tick: 0,
        lie_at: None,
    };
    let mut seen = 0;
    while let Some(step) = rep.next_step() {
        assert_eq!(step.inputs(), inputs_for(step.tick()).as_slice());
        probe.tick = step.tick();
        rep.after_tick(&probe).unwrap();
        seen += 1;
    }
    assert_eq!(seen, 100);
    assert_eq!(rep.upcoming_tick(), None);

    let dump_bytes = rep.finish_into(Vec::new()).unwrap();
    let mut reader = RecReader::open(Cursor::new(&dump_bytes)).unwrap();
    assert_eq!(reader.header().config.input_format_id, 7);
    assert_eq!(reader.tick_count(), 100);
    let dumps: Vec<(u64, StateDump)> = reader
        .chunks()
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|c| match c {
            Chunk::StateDump { tick, dump } => Some((tick, dump)),
            _ => None,
        })
        .collect();
    assert_eq!(dumps.len(), 2);
    assert_eq!(dumps[0].0, 7);
    assert_eq!(dumps[1].0, 42);
    assert_eq!(dumps[1].1.get("tick"), Some(&tickwise::Value::U64(42)));
}

#[test]
fn a_lying_probe_is_caught_at_the_exact_tick() {
    let bytes = record(0..100, 0);
    let mut rep = replayer(
        &bytes,
        ReplayConfig {
            verify_hashes: true,
            ..ReplayConfig::default()
        },
    )
    .unwrap();
    let mut probe = TickProbe {
        tick: 0,
        lie_at: Some(42),
    };
    let mut failure = None;
    while let Some(step) = rep.next_step() {
        probe.tick = step.tick();
        if let Err(err) = rep.after_tick(&probe) {
            failure = Some(err);
            break;
        }
    }
    match failure {
        Some(ReplayError::HashMismatch { tick, kind, .. }) => {
            assert_eq!(tick, 42);
            assert_eq!(kind, HashKind::Light);
        }
        other => panic!("expected a hash mismatch, got {other:?}"),
    }
}

#[test]
fn verification_off_lets_a_lying_probe_through() {
    let bytes = record(0..100, 0);
    let mut rep = replayer(&bytes, ReplayConfig::default()).unwrap();
    let mut probe = TickProbe {
        tick: 0,
        lie_at: Some(42),
    };
    while let Some(step) = rep.next_step() {
        probe.tick = step.tick();
        rep.after_tick(&probe).unwrap();
    }
}

#[test]
fn input_format_mismatch_fails_at_open() {
    let bytes = record(0..10, 7);
    let result = replayer(
        &bytes,
        ReplayConfig {
            expected_input_format_id: Some(8),
            ..ReplayConfig::default()
        },
    );
    assert!(matches!(
        result,
        Err(ReplayError::InputFormatMismatch {
            recorded: 7,
            expected: 8
        })
    ));
}

#[test]
fn dump_ticks_outside_the_recording_fail_at_open() {
    let bytes = record(0..10, 0);
    let result = replayer(
        &bytes,
        ReplayConfig {
            dump_at_ticks: vec![4021],
            ..ReplayConfig::default()
        },
    );
    assert!(matches!(
        result,
        Err(ReplayError::TickOutOfRange {
            tick: 4021,
            first: 0,
            last: 9
        })
    ));
}

#[test]
fn protocol_violations_are_loud() {
    let bytes = record(0..10, 0);
    let probe = TickProbe {
        tick: 0,
        lie_at: None,
    };

    let mut rep = replayer(&bytes, ReplayConfig::default()).unwrap();
    assert!(matches!(
        rep.after_tick(&probe),
        Err(ReplayError::NoPendingStep)
    ));

    let mut rep = replayer(&bytes, ReplayConfig::default()).unwrap();
    let _ = rep.next_step();
    let _ = rep.next_step();
    rep.after_tick(&probe).unwrap();
    assert!(matches!(
        rep.into_dumps(),
        Err(ReplayError::StepSkipped { tick: 0 })
    ));
}

#[test]
fn snapshots_are_located_and_seeking_resumes_after_them() {
    let bytes = record(0..100, 0);
    let mut rep = replayer(&bytes, ReplayConfig::default()).unwrap();
    assert_eq!(rep.snapshot_ticks(), vec![0, 30, 60, 90]);

    let (tick, data) = rep.nearest_snapshot_before(75).unwrap();
    assert_eq!(tick, 60);
    assert_eq!(data, b"state@60");
    assert_eq!(rep.nearest_snapshot_before(90).map(|(t, _)| t), Some(90));

    rep.seek_to(tick + 1).unwrap();
    let step = rep.next_step().unwrap();
    assert_eq!(step.tick(), 61);
    assert_eq!(step.inputs(), &[9, 9]);

    assert!(matches!(
        rep.seek_to(500),
        Err(ReplayError::TickOutOfRange { tick: 500, .. })
    ));
}

#[test]
fn seeking_backwards_re_expands_inputs_correctly() {
    let bytes = record(0..100, 0);
    let mut rep = replayer(&bytes, ReplayConfig::default()).unwrap();
    let probe = TickProbe {
        tick: 0,
        lie_at: None,
    };
    for _ in 0..50 {
        rep.next_step().unwrap();
        rep.after_tick(&probe).unwrap();
    }
    rep.seek_to(5).unwrap();
    let step = rep.next_step().unwrap();
    assert_eq!(step.tick(), 5);
    assert_eq!(step.inputs(), &[1]);
}
