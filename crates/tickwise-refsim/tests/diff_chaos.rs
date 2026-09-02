//! Each chaos class leaves a characteristic fingerprint in the structural
//! diff. This is the field-level half of the two-pass workflow, proven on
//! the reference simulation.

use tickwise::DeterminismProbe;
use tickwise::diff::{DiffClass, FloatPolicy, diff_dumps};
use tickwise_refsim::{ChaosConfig, ChaosMode, World, WorldConfig};

const SEED: u64 = 0x0DD_BA11;
const STRIKE: u64 = 250;

fn world(chaos: Option<ChaosConfig>) -> World {
    World::new(WorldConfig {
        seed: SEED,
        chaos,
        ..WorldConfig::default()
    })
}

fn dump_diff_after_strike(mode: ChaosMode) -> tickwise::diff::TickDiff {
    let mut clean = world(None);
    let mut chaotic = world(Some(ChaosConfig {
        mode,
        start_tick: STRIKE,
    }));
    for _ in 0..=STRIKE {
        clean.step(&[]);
        chaotic.step(&[]);
    }
    diff_dumps(
        STRIKE,
        &clean.state_dump(),
        &chaotic.state_dump(),
        &FloatPolicy::default(),
    )
}

#[test]
fn float_drift_shows_up_as_a_single_sub_epsilon_difference() {
    let diff = dump_diff_after_strike(ChaosMode::FloatDrift);
    assert_eq!(diff.differences.len(), 1);
    let d = &diff.differences[0];
    assert_eq!(d.path, "balls[0].velocity.x");
    assert_eq!(d.class, DiffClass::SubEpsilonFloat);
}

#[test]
fn uninit_read_shows_up_as_an_exact_score_difference() {
    let diff = dump_diff_after_strike(ChaosMode::UninitRead);
    assert_eq!(diff.differences.len(), 1);
    let d = &diff.differences[0];
    assert_eq!(d.path, "score");
    assert_eq!(d.class, DiffClass::Exact);
}

#[test]
fn hashmap_iter_shows_up_as_an_exact_score_difference() {
    let diff = dump_diff_after_strike(ChaosMode::HashmapIter);
    assert_eq!(diff.differences.len(), 1);
    assert_eq!(diff.differences[0].path, "score");
    assert_eq!(diff.differences[0].class, DiffClass::Exact);
}

#[test]
fn time_dependent_shows_up_as_an_exact_rng_difference() {
    let diff = dump_diff_after_strike(ChaosMode::TimeDependent);
    assert_eq!(diff.differences.len(), 1);
    assert_eq!(diff.differences[0].path, "rng.state");
    assert_eq!(diff.differences[0].class, DiffClass::Exact);
}

#[test]
fn clean_twins_diff_identical() {
    let mut a = world(None);
    let mut b = world(None);
    for _ in 0..500 {
        a.step(&[]);
        b.step(&[]);
    }
    let diff = diff_dumps(
        500,
        &a.state_dump(),
        &b.state_dump(),
        &FloatPolicy::default(),
    );
    assert!(diff.is_identical());
    assert!(diff.fields_compared > 30);
}
