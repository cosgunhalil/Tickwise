//! Tests for the serde convenience layer: path emission, automatic
//! hashing, typed inputs, and the whole two-pass workflow driven by a
//! plain `#[derive(Serialize)]` state.

#![cfg(feature = "serde")]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use tickwise::diff::{DiffClass, FloatPolicy, diff_dumps};
use tickwise::format::RecReader;
use tickwise::serde_probe::{HashAlgo, SerdeProbe, format_id, to_dump};
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, ReplayConfig, Replayer, Value};

#[derive(Serialize, Clone)]
struct Vec2 {
    x: f32,
    y: f32,
}

#[derive(Serialize, Clone)]
enum Mode {
    Idle,
    Dashing { speed: f32 },
    Carrying(u32),
}

#[derive(Serialize, Clone)]
struct Player {
    name: String,
    position: Vec2,
    mode: Mode,
    target: Option<u32>,
}

#[derive(Serialize, Clone)]
struct World {
    tick: u64,
    players: Vec<Player>,
    scores: BTreeMap<String, i32>,
    flags: (bool, u8),
}

fn world() -> World {
    let mut scores = BTreeMap::new();
    scores.insert("alice".to_string(), 3);
    scores.insert("bob".to_string(), -1);
    World {
        tick: 42,
        players: vec![
            Player {
                name: "alice".to_string(),
                position: Vec2 { x: 1.5, y: -2.0 },
                mode: Mode::Idle,
                target: None,
            },
            Player {
                name: "bob".to_string(),
                position: Vec2 { x: 0.0, y: 9.25 },
                mode: Mode::Dashing { speed: 7.5 },
                target: Some(1),
            },
        ],
        scores,
        flags: (true, 200),
    }
}

#[test]
fn to_dump_emits_the_documented_paths() {
    let dump = to_dump(&world()).unwrap();
    assert_eq!(dump.get("tick"), Some(&Value::U64(42)));
    assert_eq!(dump.get("players"), Some(&Value::Len(2)));
    assert_eq!(
        dump.get("players[0].name"),
        Some(&Value::Str("alice".into()))
    );
    assert_eq!(dump.get("players[0].position.x"), Some(&Value::F32(1.5)));
    assert_eq!(
        dump.get("players[0].mode"),
        Some(&Value::Str("Idle".into()))
    );
    assert_eq!(dump.get("players[0].target"), Some(&Value::Null));
    assert_eq!(
        dump.get("players[1].mode.Dashing.speed"),
        Some(&Value::F32(7.5))
    );
    assert_eq!(dump.get("players[1].target"), Some(&Value::U64(1)));
    assert_eq!(dump.get("scores"), Some(&Value::Len(2)));
    assert_eq!(dump.get("scores[alice]"), Some(&Value::I64(3)));
    assert_eq!(dump.get("scores[bob]"), Some(&Value::I64(-1)));
    assert_eq!(dump.get("flags"), Some(&Value::Len(2)));
    assert_eq!(dump.get("flags[0]"), Some(&Value::Bool(true)));
    assert_eq!(dump.get("flags[1]"), Some(&Value::U64(200)));
}

#[test]
fn variant_changes_are_structural_and_hashmaps_dump_canonically() {
    let a = world();
    let mut b = world();
    b.players[0].mode = Mode::Carrying(9);
    let diff = diff_dumps(
        0,
        &to_dump(&a).unwrap(),
        &to_dump(&b).unwrap(),
        &FloatPolicy::default(),
    );
    // Idle lived at players[0].mode as a string, Carrying(9) lives at
    // players[0].mode.Carrying: two paths each present on one side only.
    assert_eq!(diff.count(DiffClass::Structural), 2);
    assert!(
        diff.differences
            .iter()
            .any(|d| d.path == "players[0].mode.Carrying")
    );
    assert!(diff.differences.iter().any(|d| d.path == "players[0].mode"));

    let mut m1 = HashMap::new();
    let mut m2 = HashMap::new();
    for i in 0..50u32 {
        m1.insert(i, i * 2);
    }
    for i in (0..50u32).rev() {
        m2.insert(i, i * 2);
    }
    assert_eq!(to_dump(&m1).unwrap(), to_dump(&m2).unwrap());
}

#[test]
fn float_map_keys_are_rejected_with_a_readable_error() {
    // A key that serializes as a float, since f32 itself cannot be a key.
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct FloatKey(u32);
    impl Serialize for FloatKey {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_f32(self.0 as f32 / 2.0)
        }
    }
    let mut m = BTreeMap::new();
    m.insert(FloatKey(3), 1u8);
    let err = to_dump(&m).unwrap_err();
    assert!(err.to_string().contains("map keys must be"));
}

#[test]
fn serde_probe_hashes_are_deterministic_and_sensitive() {
    let a = world();
    let mut b = world();
    let probe_a = SerdeProbe::new(&a);
    let probe_b = SerdeProbe::new(&b);
    assert_eq!(probe_a.full_hash(), probe_b.full_hash());
    assert_eq!(probe_a.light_hash(), probe_a.full_hash());
    assert_eq!(probe_a.hash_algo_id(), 1);

    b.players[1].position.y += 0.001;
    let probe_b = SerdeProbe::new(&b);
    assert_ne!(probe_a.full_hash(), probe_b.full_hash());

    // A light view that ignores positions keeps agreeing.
    #[derive(Serialize)]
    struct Light {
        tick: u64,
        players: usize,
    }
    let light_a = Light {
        tick: a.tick,
        players: a.players.len(),
    };
    let light_b = Light {
        tick: b.tick,
        players: b.players.len(),
    };
    let pa = SerdeProbe::with_light(&a, &light_a);
    let pb = SerdeProbe::with_light(&b, &light_b);
    assert_eq!(pa.light_hash(), pb.light_hash());
    assert_ne!(pa.full_hash(), pb.full_hash());

    // The dump from the probe is the same dump to_dump produces.
    assert_eq!(probe_a.state_dump(), to_dump(&a).unwrap());
}

#[cfg(feature = "blake3")]
#[test]
fn blake3_is_a_distinct_algorithm() {
    let a = world();
    let xx = SerdeProbe::new(&a);
    let bl = SerdeProbe::new(&a).with_algo(HashAlgo::Blake3);
    assert_eq!(bl.hash_algo_id(), 2);
    assert_ne!(xx.full_hash(), bl.full_hash());
}

#[test]
fn format_id_is_stable_and_label_sensitive() {
    assert_eq!(format_id("MyInput v1"), format_id("MyInput v1"));
    assert_ne!(format_id("MyInput v1"), format_id("MyInput v2"));
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
struct Input {
    dx: i8,
    dy: i8,
    jump: bool,
}

#[derive(Serialize, Clone)]
struct Sim {
    tick: u64,
    x: i64,
    y: i64,
    airborne: bool,
}

impl Sim {
    fn step(&mut self, input: Input) {
        self.tick += 1;
        self.x += i64::from(input.dx);
        self.y += i64::from(input.dy);
        self.airborne = input.jump;
    }
}

fn input_for(tick: u64) -> Input {
    Input {
        dx: ((tick / 7) % 3) as i8 - 1,
        dy: ((tick / 11) % 3) as i8 - 1,
        jump: tick.is_multiple_of(13),
    }
}

#[test]
fn the_whole_two_pass_workflow_runs_on_a_derived_state() {
    let format = format_id("Input v1");
    let config = RecorderConfig {
        full_hash_interval: 25,
        hash_algo_id: HashAlgo::Xxh3.id(),
        input_format_id: format,
        ..RecorderConfig::default()
    };

    // Pass 1: two sessions, the second one buggy from tick 60 on.
    let record = |bug: bool| {
        let mut sim = Sim {
            tick: 0,
            x: 0,
            y: 0,
            airborne: false,
        };
        let mut rec = Recorder::new(Vec::new(), config.clone()).unwrap();
        for tick in 0..100 {
            sim.step(input_for(tick));
            if bug && tick >= 60 {
                sim.y += 1;
            }
            rec.record_tick_typed(tick, &input_for(tick), &SerdeProbe::new(&sim))
                .unwrap();
        }
        rec.finish().unwrap()
    };
    let clean = record(false);
    let buggy = record(true);

    let mut ra = RecReader::open(Cursor::new(&clean)).unwrap();
    let mut rb = RecReader::open(Cursor::new(&buggy)).unwrap();
    let report = tickwise::compare::first_divergence_from(&mut ra, &mut rb).unwrap();
    let divergent_tick = match report.outcome {
        tickwise::compare::Outcome::Diverged(d) => d.tick,
        other => panic!("expected divergence, got {other:?}"),
    };
    assert_eq!(divergent_tick, 60);

    // Pass 2: replay both with typed inputs, dump at the divergent tick.
    let replay = |bytes: &[u8], bug: bool| {
        let mut reader = RecReader::open(Cursor::new(bytes)).unwrap();
        let mut rep = Replayer::from_reader(
            &mut reader,
            ReplayConfig {
                dump_at_ticks: vec![divergent_tick],
                verify_hashes: true,
                expected_input_format_id: Some(format),
            },
        )
        .unwrap();
        let mut sim = Sim {
            tick: 0,
            x: 0,
            y: 0,
            airborne: false,
        };
        while let Some(step) = rep.next_step() {
            let input: Input = step.inputs_typed().unwrap();
            assert_eq!(input, input_for(step.tick()));
            sim.step(input);
            if bug && step.tick() >= 60 {
                sim.y += 1;
            }
            rep.after_tick(&SerdeProbe::new(&sim)).unwrap();
        }
        rep.into_dumps().unwrap().remove(0).1
    };
    let dump_clean = replay(&clean, false);
    let dump_buggy = replay(&buggy, true);

    let diff = diff_dumps(
        divergent_tick,
        &dump_clean,
        &dump_buggy,
        &FloatPolicy::default(),
    );
    assert_eq!(diff.differences.len(), 1);
    assert_eq!(diff.differences[0].path, "y");
    assert_eq!(diff.differences[0].class, DiffClass::Exact);

    // The replayer refuses recordings made with a different input type.
    let mut reader = RecReader::open(Cursor::new(&clean)).unwrap();
    assert!(
        Replayer::from_reader(
            &mut reader,
            ReplayConfig {
                expected_input_format_id: Some(format_id("Input v2")),
                ..ReplayConfig::default()
            },
        )
        .is_err()
    );
}
