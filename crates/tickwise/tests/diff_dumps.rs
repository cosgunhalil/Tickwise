//! Tests for the structural diff engine: every classification path, the
//! float policy, and file-level tick pairing.

use std::io::Cursor;
use tickwise::diff::{
    Detail, DiffClass, DiffError, FloatPolicy, Side, diff_dumps, structural_from,
};
use tickwise::format::{Chunk, Header, RecReader, RecWriter};
use tickwise::{StateDump, Value};

fn base() -> StateDump {
    let mut d = StateDump::empty();
    d.insert("tick", 4021u64);
    d.insert("score", 77i64);
    d.insert("alive", true);
    d.insert("name", "refsim");
    d.insert("players", Value::Len(2));
    d.insert("players[0].velocity.x", 3.5f32);
    d.insert("players[1].velocity.x", -1.25f32);
    d.insert("precise", 0.1f64);
    d
}

#[test]
fn identical_dumps_have_no_differences() {
    let result = diff_dumps(4021, &base(), &base(), &FloatPolicy::default());
    assert!(result.is_identical());
    assert_eq!(result.fields_compared, 8);
    assert_eq!(result.tick, 4021);
}

#[test]
fn each_class_is_recognized() {
    let a = base();
    let mut b = base();
    // Structural: only on one side, length change, type change.
    b.insert("extra", 1u64);
    b.insert("players", Value::Len(3));
    b.insert("alive", 1u64);
    // Exact: integer and string changes.
    b.insert("score", 78i64);
    b.insert("name", "refsim2");
    // Sub-epsilon: one ULP above 3.5.
    b.insert(
        "players[0].velocity.x",
        f32::from_bits(3.5f32.to_bits() + 1),
    );
    // Exact float: a large jump.
    b.insert("players[1].velocity.x", 2.0f32);
    // Sub-epsilon f64.
    b.insert("precise", 0.1f64 + 1e-15);

    let result = diff_dumps(0, &a, &b, &FloatPolicy::default());
    let by_path = |path: &str| {
        result
            .differences
            .iter()
            .find(|d| d.path == path)
            .unwrap_or_else(|| panic!("no difference at {path}"))
    };

    assert_eq!(by_path("extra").class, DiffClass::Structural);
    assert!(matches!(
        by_path("extra").detail,
        Detail::OnlyOn {
            side: Side::Second,
            ..
        }
    ));
    assert_eq!(by_path("players").class, DiffClass::Structural);
    assert!(matches!(
        by_path("players").detail,
        Detail::LengthMismatch { a: 2, b: 3 }
    ));
    assert_eq!(by_path("alive").class, DiffClass::Structural);
    assert!(matches!(
        by_path("alive").detail,
        Detail::TypeMismatch {
            a: "bool",
            b: "u64"
        }
    ));
    assert_eq!(by_path("score").class, DiffClass::Exact);
    assert_eq!(by_path("name").class, DiffClass::Exact);
    assert_eq!(
        by_path("players[0].velocity.x").class,
        DiffClass::SubEpsilonFloat
    );
    assert_eq!(by_path("players[1].velocity.x").class, DiffClass::Exact);
    assert_eq!(by_path("precise").class, DiffClass::SubEpsilonFloat);

    assert_eq!(result.count(DiffClass::Structural), 3);
    assert_eq!(result.count(DiffClass::Exact), 3);
    assert_eq!(result.count(DiffClass::SubEpsilonFloat), 2);
    assert_eq!(result.fields_compared, 8);

    // Differences come out in sorted path order.
    let paths: Vec<&str> = result.differences.iter().map(|d| d.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);
}

#[test]
fn missing_on_the_first_side_is_structural_too() {
    let mut a = base();
    a.insert("only_a", 5u64);
    let mut b = base();
    b.insert("only_b", 6u64);
    let result = diff_dumps(0, &a, &b, &FloatPolicy::default());
    assert_eq!(result.differences.len(), 2);
    assert!(matches!(
        result.differences[0].detail,
        Detail::OnlyOn {
            side: Side::First,
            ..
        }
    ));
    assert!(matches!(
        result.differences[1].detail,
        Detail::OnlyOn {
            side: Side::Second,
            ..
        }
    ));
}

#[test]
fn the_float_policy_decides_the_class() {
    let mut a = StateDump::empty();
    a.insert("x", 1.0f32);
    let mut b = StateDump::empty();
    b.insert("x", 1.0f32 + 1e-6);

    let lenient = diff_dumps(0, &a, &b, &FloatPolicy::default());
    assert_eq!(lenient.differences[0].class, DiffClass::SubEpsilonFloat);

    let strict = diff_dumps(0, &a, &b, &FloatPolicy::strict());
    assert_eq!(strict.differences[0].class, DiffClass::Exact);
}

#[test]
fn nan_and_infinity_are_exact_differences() {
    let mut a = StateDump::empty();
    a.insert("x", 1.0f32);
    a.insert("y", f64::INFINITY);
    let mut b = StateDump::empty();
    b.insert("x", f32::NAN);
    b.insert("y", f64::NEG_INFINITY);
    let result = diff_dumps(0, &a, &b, &FloatPolicy::default());
    assert_eq!(result.count(DiffClass::Exact), 2);
}

#[test]
fn identical_nan_bits_are_not_a_difference() {
    let mut a = StateDump::empty();
    a.insert("x", f32::NAN);
    let b = a.clone();
    assert!(diff_dumps(0, &a, &b, &FloatPolicy::default()).is_identical());
}

#[test]
fn difference_display_reads_like_a_sentence() {
    let mut a = StateDump::empty();
    a.insert("projectiles", Value::Len(14));
    let mut b = StateDump::empty();
    b.insert("projectiles", Value::Len(15));
    let result = diff_dumps(0, &a, &b, &FloatPolicy::default());
    assert_eq!(
        result.differences[0].to_string(),
        "projectiles: length 14 versus 15, structural"
    );
}

fn dump_file(ticks: &[(u64, StateDump)]) -> Vec<u8> {
    let mut writer = RecWriter::new(Vec::new(), &Header::default()).unwrap();
    for (tick, dump) in ticks {
        writer
            .write_chunk(&Chunk::StateDump {
                tick: *tick,
                dump: dump.clone(),
            })
            .unwrap();
    }
    writer.finish(0).unwrap()
}

#[test]
fn files_are_paired_by_tick() {
    let mut changed = base();
    changed.insert("score", 99i64);
    let a = dump_file(&[(100, base()), (200, base())]);
    let b = dump_file(&[(200, changed), (300, base())]);

    let mut ra = RecReader::open(Cursor::new(&a)).unwrap();
    let mut rb = RecReader::open(Cursor::new(&b)).unwrap();
    let report = structural_from(&mut ra, &mut rb, FloatPolicy::default()).unwrap();

    assert_eq!(report.ticks.len(), 1);
    assert_eq!(report.ticks[0].tick, 200);
    assert_eq!(report.ticks[0].differences.len(), 1);
    assert_eq!(report.ticks[0].differences[0].path, "score");
    assert_eq!(report.only_in_a, vec![100]);
    assert_eq!(report.only_in_b, vec![300]);
    assert!(!report.is_identical());
}

#[test]
fn missing_dumps_and_disjoint_ticks_are_errors() {
    let empty = dump_file(&[]);
    let a = dump_file(&[(100, base())]);
    let b = dump_file(&[(200, base())]);

    let mut re = RecReader::open(Cursor::new(&empty)).unwrap();
    let mut ra = RecReader::open(Cursor::new(&a)).unwrap();
    assert!(matches!(
        structural_from(&mut re, &mut ra, FloatPolicy::default()),
        Err(DiffError::NoDumps { side: Side::First })
    ));

    let mut ra = RecReader::open(Cursor::new(&a)).unwrap();
    let mut rb = RecReader::open(Cursor::new(&b)).unwrap();
    assert!(matches!(
        structural_from(&mut ra, &mut rb, FloatPolicy::default()),
        Err(DiffError::NoCommonTicks)
    ));
}
