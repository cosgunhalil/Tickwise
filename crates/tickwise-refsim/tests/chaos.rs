//! The M2 definition of done: every chaos class is caught at the
//! correct tick.
//!
//! Each test records a clean control run and a run with one chaos mode
//! striking at tick 250, then asks Tickwise for the first divergence.
//! Detection expectations differ by class on purpose: the cheap per-tick
//! light hash catches state-visible chaos immediately, while float drift
//! slips past it and is caught by the next full hash, demonstrating the
//! blind spot mechanism.

use std::io::Cursor;
use tickwise::compare::{HashKind, Outcome, first_divergence_from};
use tickwise::format::RecReader;
use tickwise::{Recorder, RecorderConfig, SessionMeta};
use tickwise_refsim::{ChaosConfig, ChaosMode, Lcg, PlayerInput, World, WorldConfig};

const TICKS: u64 = 600;
const FULL_INTERVAL: u32 = 100;
const CHAOS_START: u64 = 250;
const SEED: u64 = 0x0DD_BA11;

fn record_run(chaos: Option<ChaosConfig>) -> Vec<u8> {
    let config = RecorderConfig {
        session_meta: SessionMeta {
            game_id: "refsim-chaos".to_string(),
            rng_seed: SEED,
            ..SessionMeta::default()
        },
        full_hash_interval: FULL_INTERVAL,
        ..RecorderConfig::default()
    };
    let mut world = World::new(WorldConfig {
        seed: SEED,
        chaos,
        ..WorldConfig::default()
    });
    let mut input_rng = Lcg::new(9001);
    let mut rec = Recorder::new(Vec::new(), config).unwrap();

    for tick in 0..TICKS {
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
    }
    rec.finish().unwrap()
}

fn first_divergence_against_control(mode: ChaosMode) -> Outcome {
    let control = record_run(None);
    let chaotic = record_run(Some(ChaosConfig {
        mode,
        start_tick: CHAOS_START,
    }));
    let mut a = RecReader::open(Cursor::new(&control)).unwrap();
    let mut b = RecReader::open(Cursor::new(&chaotic)).unwrap();
    first_divergence_from(&mut a, &mut b).unwrap().outcome
}

#[test]
fn control_runs_are_identical() {
    let a = record_run(None);
    let b = record_run(None);
    let mut ra = RecReader::open(Cursor::new(&a)).unwrap();
    let mut rb = RecReader::open(Cursor::new(&b)).unwrap();
    let report = first_divergence_from(&mut ra, &mut rb).unwrap();
    assert!(
        matches!(report.outcome, Outcome::Identical { ticks_compared, .. } if ticks_compared == TICKS)
    );
}

#[test]
fn uninit_read_is_caught_by_the_light_hash_at_the_strike_tick() {
    match first_divergence_against_control(ChaosMode::UninitRead) {
        Outcome::Diverged(d) => {
            assert_eq!(d.tick, CHAOS_START);
            assert_eq!(d.detected_by, HashKind::Light);
        }
        other => panic!("expected divergence, got {other:?}"),
    }
}

#[test]
fn hashmap_iter_is_caught_by_the_light_hash_at_the_strike_tick() {
    match first_divergence_against_control(ChaosMode::HashmapIter) {
        Outcome::Diverged(d) => {
            assert_eq!(d.tick, CHAOS_START);
            assert_eq!(d.detected_by, HashKind::Light);
        }
        other => panic!("expected divergence, got {other:?}"),
    }
}

#[test]
fn time_dependent_is_caught_by_the_light_hash_at_the_strike_tick() {
    match first_divergence_against_control(ChaosMode::TimeDependent) {
        Outcome::Diverged(d) => {
            assert_eq!(d.tick, CHAOS_START);
            assert_eq!(d.detected_by, HashKind::Light);
        }
        other => panic!("expected divergence, got {other:?}"),
    }
}

#[test]
fn float_drift_slips_past_the_light_hash_and_the_full_hash_catches_it() {
    match first_divergence_against_control(ChaosMode::FloatDrift) {
        Outcome::Diverged(d) => {
            // The first full hash at or after the strike is tick 300.
            assert_eq!(d.tick, 300);
            assert_eq!(d.detected_by, HashKind::Full);
            assert_eq!(d.confirming_full_hash_tick, Some(300));
        }
        other => panic!("expected divergence, got {other:?}"),
    }
}

#[test]
fn chaos_mode_names_round_trip() {
    for mode in ChaosMode::ALL {
        let parsed: ChaosMode = mode.to_string().parse().unwrap();
        assert_eq!(parsed, mode);
    }
    assert!("gremlins".parse::<ChaosMode>().is_err());
}
