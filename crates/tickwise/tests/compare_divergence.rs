//! Tests for first-divergence search across every verdict path.

use std::io::Cursor;
use tickwise::compare::{
    CompareError, CompareWarning, Divergence, HashKind, Outcome, first_divergence_from,
};
use tickwise::format::RecReader;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, SessionMeta, StateDump};

/// A probe whose hashes are set directly by the test loop.
struct ScriptedProbe {
    light: u64,
    full: u64,
}

impl DeterminismProbe for ScriptedProbe {
    fn light_hash(&self) -> u64 {
        self.light
    }
    fn full_hash(&self) -> u64 {
        self.full
    }
    fn state_dump(&self) -> StateDump {
        StateDump::empty()
    }
}

const FULL_INTERVAL: u32 = 100;

/// Records a session where the light and full hashes at each tick come
/// from the two closures.
fn record<L, F>(ticks: std::ops::Range<u64>, seed: u64, light: L, full: F) -> Vec<u8>
where
    L: Fn(u64) -> u64,
    F: Fn(u64) -> u64,
{
    let config = RecorderConfig {
        session_meta: SessionMeta {
            rng_seed: seed,
            ..SessionMeta::default()
        },
        full_hash_interval: FULL_INTERVAL,
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::new(Vec::new(), config).unwrap();
    let mut probe = ScriptedProbe { light: 0, full: 0 };
    for tick in ticks {
        probe.light = light(tick);
        probe.full = full(tick);
        rec.record_tick(tick, &[], &probe).unwrap();
    }
    rec.finish().unwrap()
}

fn compare(a: &[u8], b: &[u8]) -> Result<tickwise::compare::CompareReport, CompareError> {
    let mut ra = RecReader::open(Cursor::new(a)).unwrap();
    let mut rb = RecReader::open(Cursor::new(b)).unwrap();
    first_divergence_from(&mut ra, &mut rb)
}

/// Records with a dump interval, diverging in the light hash from `strike`.
fn record_with_dumps(dump_interval: u32, strike: Option<u64>) -> Vec<u8> {
    let config = RecorderConfig {
        full_hash_interval: FULL_INTERVAL,
        dump_interval,
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::new(Vec::new(), config).unwrap();
    let mut probe = ScriptedProbe { light: 0, full: 0 };
    for tick in 0..1000u64 {
        let diverged = strike.is_some_and(|at| tick >= at);
        probe.light = tick ^ u64::from(diverged);
        probe.full = tick;
        rec.record_tick(tick, &[], &probe).unwrap();
    }
    rec.finish().unwrap()
}

#[test]
fn a_report_names_the_dumps_each_side_carries() {
    let a = record_with_dumps(250, None);
    let b = record_with_dumps(250, Some(421));
    let report = compare(&a, &b).unwrap();

    assert_eq!(report.dump_ticks_a, vec![0, 250, 500, 750]);
    assert_eq!(report.dump_ticks_b, vec![0, 250, 500, 750]);
    assert!(report.has_dumps());
    match report.outcome {
        Outcome::Diverged(Divergence { tick: 421, .. }) => {}
        other => panic!("{other:?}"),
    }
    // The first shared dump past the divergence is where diff should look,
    // and the last one before it is the before picture.
    assert_eq!(report.shared_dump_at_or_after(421), Some(500));
    assert_eq!(report.shared_dump_at_or_before(420), Some(250));
    assert_eq!(report.shared_dump_at_or_after(751), None);
}

#[test]
fn dumps_on_only_one_side_are_reported_but_never_shared() {
    let a = record_with_dumps(250, None);
    let b = record_with_dumps(0, Some(421));
    let report = compare(&a, &b).unwrap();

    assert_eq!(report.dump_ticks_a.len(), 4);
    assert!(report.dump_ticks_b.is_empty());
    assert!(report.has_dumps());
    assert_eq!(report.shared_dump_at_or_after(0), None);
    assert_eq!(report.shared_dump_at_or_before(999), None);
}

#[test]
fn recordings_without_dumps_report_none() {
    let a = record(0..100, 1, |t| t, |t| t);
    let report = compare(&a, &a).unwrap();
    assert!(!report.has_dumps());
    assert!(report.dump_ticks_a.is_empty());
}

#[test]
fn identical_recordings_compare_identical() {
    let a = record(0..500, 1, |t| t * 3, |t| t * 7);
    let b = record(0..500, 1, |t| t * 3, |t| t * 7);
    let report = compare(&a, &b).unwrap();
    assert!(report.warnings.is_empty());
    assert_eq!(
        report.outcome,
        Outcome::Identical {
            ticks_compared: 500,
            extra_ticks_a: 0,
            extra_ticks_b: 0,
        }
    );
}

#[test]
fn light_hash_divergence_is_found_at_the_exact_tick() {
    let a = record(0..500, 1, |t| t, |t| t);
    // Diverges at tick 321 in both streams.
    let b = record(
        0..500,
        1,
        |t| if t >= 321 { t + 1 } else { t },
        |t| {
            if t >= 321 { t + 1 } else { t }
        },
    );
    let report = compare(&a, &b).unwrap();
    assert_eq!(
        report.outcome,
        Outcome::Diverged(Divergence {
            tick: 321,
            detected_by: HashKind::Light,
            last_agreeing_tick: Some(320),
            confirming_full_hash_tick: Some(400),
        })
    );
    let text = report.to_string();
    assert!(text.contains("tick 321"));
    assert!(text.contains("confirmed by the full hash at tick 400"));
}

#[test]
fn full_hash_catches_a_light_hash_blind_spot() {
    // The light hash never sees the difference, the full hash does from
    // tick 250 onward, so the first full hash to fire is at 300.
    let a = record(0..500, 1, |_| 42, |t| t);
    let b = record(0..500, 1, |_| 42, |t| if t >= 250 { t + 1 } else { t });
    let report = compare(&a, &b).unwrap();
    assert_eq!(
        report.outcome,
        Outcome::Diverged(Divergence {
            tick: 300,
            detected_by: HashKind::Full,
            last_agreeing_tick: Some(200),
            confirming_full_hash_tick: Some(300),
        })
    );
    assert!(report.to_string().contains("blind spot"));
}

#[test]
fn full_hash_firing_before_light_reports_the_earlier_tick() {
    // Full hashes diverge from tick 100, the light stream only from 350.
    let a = record(0..500, 1, |t| t, |t| t);
    let b = record(
        0..500,
        1,
        |t| if t >= 350 { t + 1 } else { t },
        |t| if t >= 100 { t + 1 } else { t },
    );
    let report = compare(&a, &b).unwrap();
    match report.outcome {
        Outcome::Diverged(d) => {
            assert_eq!(d.tick, 100);
            assert_eq!(d.detected_by, HashKind::Full);
        }
        other => panic!("expected divergence, got {other:?}"),
    }
}

#[test]
fn different_lengths_with_identical_overlap() {
    let a = record(0..800, 1, |t| t, |t| t);
    let b = record(0..500, 1, |t| t, |t| t);
    let report = compare(&a, &b).unwrap();
    assert_eq!(
        report.outcome,
        Outcome::Identical {
            ticks_compared: 500,
            extra_ticks_a: 300,
            extra_ticks_b: 0,
        }
    );
}

#[test]
fn recordings_starting_at_different_ticks_compare_over_the_overlap() {
    let a = record(0..600, 1, |t| t, |t| t);
    let b = record(400..900, 1, |t| if t >= 550 { t + 9 } else { t }, |t| t);
    let report = compare(&a, &b).unwrap();
    match report.outcome {
        Outcome::Diverged(d) => {
            assert_eq!(d.tick, 550);
            assert_eq!(d.detected_by, HashKind::Light);
            assert_eq!(d.last_agreeing_tick, Some(549));
        }
        other => panic!("expected divergence, got {other:?}"),
    }
}

#[test]
fn seed_mismatch_is_a_warning_not_an_error() {
    let a = record(0..100, 1, |t| t, |t| t);
    let b = record(0..100, 2, |t| t, |t| t);
    let report = compare(&a, &b).unwrap();
    assert_eq!(report.warnings, vec![CompareWarning::SeedMismatch(1, 2)]);
    assert!(matches!(report.outcome, Outcome::Identical { .. }));
}

#[test]
fn hash_algo_mismatch_is_an_error() {
    let a = record(0..100, 1, |t| t, |t| t);
    let config = RecorderConfig {
        hash_algo_id: 9,
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::new(Vec::new(), config).unwrap();
    let probe = ScriptedProbe { light: 0, full: 0 };
    rec.record_tick(0, &[], &probe).unwrap();
    let b = rec.finish().unwrap();

    assert!(matches!(
        compare(&a, &b),
        Err(CompareError::HashAlgoMismatch { a: 0, b: 9 })
    ));
}

#[test]
fn disjoint_tick_ranges_are_an_error() {
    let a = record(0..100, 1, |t| t, |t| t);
    let b = record(5000..5100, 1, |t| t, |t| t);
    assert!(matches!(compare(&a, &b), Err(CompareError::NoCommonTicks)));
}
