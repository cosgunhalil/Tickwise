//! The push form of the recorder and the replayer: hashes and dumps
//! supplied by the caller rather than pulled from a probe. This is what
//! every engine bridge stands on, so it is tested on its own and against
//! the probe form it must agree with.

use std::io::Cursor;
use tickwise::format::{Chunk, RecReader};
use tickwise::replayer::{ReplayConfig, ReplayError, Replayer};
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, StateDump};

/// A simulation whose hashes and dump follow the tick exactly.
struct Sim {
    tick: u64,
}

impl Sim {
    fn light(&self) -> u64 {
        self.tick.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }
    fn full(&self) -> u64 {
        self.tick.wrapping_mul(31).wrapping_add(7)
    }
    fn dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        dump.insert("tick", self.tick);
        dump.insert("score", self.tick * 3);
        dump
    }
}

impl DeterminismProbe for Sim {
    fn light_hash(&self) -> u64 {
        self.light()
    }
    fn full_hash(&self) -> u64 {
        self.full()
    }
    fn state_dump(&self) -> StateDump {
        self.dump()
    }
}

const TICKS: u64 = 200;

fn config() -> RecorderConfig {
    RecorderConfig {
        full_hash_interval: 50,
        dump_interval: 100,
        input_format_id: 7,
        ..RecorderConfig::default()
    }
}

fn record_with_probe() -> Vec<u8> {
    let mut rec = Recorder::new(Vec::new(), config()).unwrap();
    let mut sim = Sim { tick: 0 };
    for tick in 0..TICKS {
        sim.tick = tick;
        rec.record_tick(tick, &[tick as u8], &sim).unwrap();
    }
    rec.finish().unwrap()
}

fn record_with_pushes() -> Vec<u8> {
    let mut rec = Recorder::new(Vec::new(), config()).unwrap();
    let mut sim = Sim { tick: 0 };
    for tick in 0..TICKS {
        sim.tick = tick;
        // What a bridge does: compute only what the recorder will keep.
        let full = if rec.wants_full_hash(tick) {
            sim.full()
        } else {
            0
        };
        rec.record_tick_hashes(tick, &[tick as u8], sim.light(), full)
            .unwrap();
        if rec.wants_dump(tick) {
            rec.record_state_dump(tick, sim.dump()).unwrap();
        }
    }
    rec.finish().unwrap()
}

#[test]
fn the_push_form_of_the_recorder_writes_the_same_bytes_as_the_probe_form() {
    assert_eq!(record_with_probe(), record_with_pushes());
}

#[test]
fn a_pushed_full_hash_is_ignored_off_the_interval() {
    let mut rec = Recorder::new(Vec::new(), config()).unwrap();
    for tick in 0..TICKS {
        // Garbage on every tick that is not on the interval.
        let full = if rec.wants_full_hash(tick) {
            tick * 31 + 7
        } else {
            0xDEAD
        };
        rec.record_tick_hashes(tick, &[], tick, full).unwrap();
    }
    let bytes = rec.finish().unwrap();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    let fulls: Vec<(u64, u64)> = reader
        .chunks()
        .unwrap()
        .map(Result::unwrap)
        .filter_map(|c| match c {
            Chunk::FullHash { tick, hash } => Some((tick, hash)),
            _ => None,
        })
        .collect();
    assert_eq!(fulls, vec![(0, 7), (50, 1557), (100, 3107), (150, 4657)]);
}

fn replay<F>(
    bytes: &[u8],
    dump_at: &[u64],
    mut drive: F,
) -> Result<Vec<(u64, StateDump)>, ReplayError>
where
    F: FnMut(&mut Replayer, u64, &Sim) -> Result<(), ReplayError>,
{
    let mut reader = RecReader::open(Cursor::new(bytes)).unwrap();
    let mut rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            dump_at_ticks: dump_at.to_vec(),
            verify_hashes: true,
            expected_input_format_id: Some(7),
        },
    )?;
    let mut sim = Sim { tick: 0 };
    while let Some(step) = rep.next_step() {
        let tick = step.tick();
        assert_eq!(step.inputs(), &[tick as u8], "inputs come back per tick");
        sim.tick = tick;
        drive(&mut rep, tick, &sim)?;
    }
    rep.into_dumps()
}

#[test]
fn the_push_form_of_the_replayer_verifies_and_dumps_like_the_probe_form() {
    let bytes = record_with_probe();

    let via_probe = replay(&bytes, &[42, 150], |rep, _, sim| rep.after_tick(sim)).unwrap();
    let via_pushes = replay(&bytes, &[42, 150], |rep, tick, sim| {
        let full = if rep.wants_full_hash(tick) {
            sim.full()
        } else {
            0
        };
        let dump = rep.wants_dump(tick).then(|| sim.dump());
        rep.after_tick_hashes(sim.light(), full, dump)
    })
    .unwrap();

    assert_eq!(via_probe, via_pushes);
    assert_eq!(via_probe.len(), 2);
    assert_eq!(via_probe[0].0, 42);
    assert_eq!(via_probe[1].0, 150);
    assert_eq!(via_probe[1].1.get("score"), Some(&450u64.into()));
}

#[test]
fn wants_full_hash_names_exactly_the_recorded_full_hash_ticks() {
    let bytes = record_with_probe();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    let rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            verify_hashes: true,
            ..ReplayConfig::default()
        },
    )
    .unwrap();
    let wanted: Vec<u64> = (0..TICKS).filter(|t| rep.wants_full_hash(*t)).collect();
    assert_eq!(wanted, vec![0, 50, 100, 150]);

    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    let quiet = Replayer::from_reader(&mut reader, ReplayConfig::default()).unwrap();
    assert!(
        !quiet.wants_full_hash(50),
        "nothing is wanted when verification is off"
    );
}

#[test]
fn a_pushed_hash_that_disagrees_is_reported_at_its_tick() {
    let bytes = record_with_probe();
    let err = replay(&bytes, &[], |rep, tick, sim| {
        let light = if tick == 77 {
            sim.light() ^ 1
        } else {
            sim.light()
        };
        let full = if rep.wants_full_hash(tick) {
            sim.full()
        } else {
            0
        };
        rep.after_tick_hashes(light, full, None)
    })
    .unwrap_err();
    match err {
        ReplayError::HashMismatch { tick: 77, .. } => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_missing_dump_is_an_error_and_the_step_can_be_completed_afterwards() {
    let bytes = record_with_probe();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    let mut rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            dump_at_ticks: vec![3],
            verify_hashes: false,
            expected_input_format_id: None,
        },
    )
    .unwrap();
    for _ in 0..3 {
        rep.next_step().unwrap();
        rep.after_tick_hashes(0, 0, None).unwrap();
    }
    let step = rep.next_step().unwrap();
    assert_eq!(step.tick(), 3);
    assert!(rep.wants_dump(3));

    let err = rep.after_tick_hashes(0, 0, None).unwrap_err();
    assert!(matches!(err, ReplayError::MissingDump { tick: 3 }));
    // The step is still pending, so supplying the dump completes it.
    rep.after_tick_hashes(0, 0, Some(StateDump::empty()))
        .unwrap();
    rep.next_step().unwrap();
    rep.after_tick_hashes(0, 0, None).unwrap();

    // An unrequested dump is dropped rather than recorded.
    rep.next_step().unwrap();
    let mut extra = StateDump::empty();
    extra.insert("stray", 1u64);
    rep.after_tick_hashes(0, 0, Some(extra)).unwrap();

    let dumps = rep.into_dumps().unwrap();
    assert_eq!(dumps.len(), 1);
    assert_eq!(dumps[0].0, 3);
}
