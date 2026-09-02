//! End-to-end tests for the diff command against real dump files.

use std::path::PathBuf;
use tickwise::diff::FloatPolicy;
use tickwise::format::{Chunk, Header, RecWriter};
use tickwise::{StateDump, Value};
use tickwise_cli::diff::{DiffOptions, parse_args, render};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tickwise-diff-test-{}-{name}", std::process::id()))
}

fn base_dump() -> StateDump {
    let mut d = StateDump::empty();
    d.insert("score", 77u64);
    d.insert("projectiles", Value::Len(14));
    d.insert("players", Value::Len(2));
    d.insert("players[2].velocity.x", 3.5f32);
    d
}

fn write_dump_file(path: &PathBuf, dumps: &[(u64, StateDump)]) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = RecWriter::new(file, &Header::default()).unwrap();
    for (tick, dump) in dumps {
        writer
            .write_chunk(&Chunk::StateDump {
                tick: *tick,
                dump: dump.clone(),
            })
            .unwrap();
    }
    writer.finish(0).unwrap();
}

fn plain() -> DiffOptions {
    DiffOptions::default()
}

#[test]
fn identical_dumps_exit_zero() {
    let a = temp_path("same-a.dump");
    let b = temp_path("same-b.dump");
    write_dump_file(&a, &[(4021, base_dump())]);
    write_dump_file(&b, &[(4021, base_dump())]);

    let output = render(&a, &b, &plain()).unwrap();
    let code = tickwise_cli::run(&[
        "diff".to_string(),
        a.display().to_string(),
        b.display().to_string(),
        "--no-color".to_string(),
    ]);
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    assert!(!output.differs);
    assert!(output.text.contains("identical over 4 fields"));
    assert!(output.text.contains("identical at 1 compared tick"));
    assert_eq!(code, 0);
}

#[test]
fn the_design_doc_example_renders_as_promised() {
    let a = temp_path("doc-a.dump");
    let b = temp_path("doc-b.dump");
    let mut changed = base_dump();
    changed.insert("projectiles", Value::Len(15));
    changed.insert(
        "players[2].velocity.x",
        f32::from_bits(3.5f32.to_bits() + 1),
    );
    changed.insert("score", 78u64);
    write_dump_file(&a, &[(4021, base_dump())]);
    write_dump_file(&b, &[(4021, changed)]);

    let output = render(&a, &b, &plain()).unwrap();
    let code = tickwise_cli::run(&[
        "diff".to_string(),
        a.display().to_string(),
        b.display().to_string(),
        "--no-color".to_string(),
    ]);
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    assert!(output.differs);
    assert!(output.text.contains("tick 4021"));
    assert!(
        output
            .text
            .contains("1 structural, 1 exact, 1 sub-epsilon float drift")
    );
    assert!(
        output
            .text
            .contains("structural     projectiles: length 14 versus 15")
    );
    assert!(output.text.contains("score: 77 versus 78"));
    assert!(
        output
            .text
            .contains("players[2].velocity.x: 3.5 versus 3.5000002")
    );
    assert!(output.text.contains("3 differences across 1 compared tick"));
    assert!(output.text.contains("structural differences usually mean"));
    assert_eq!(code, 1);
}

#[test]
fn strict_policy_turns_drift_into_exact() {
    let a = temp_path("strict-a.dump");
    let b = temp_path("strict-b.dump");
    let mut drifted = base_dump();
    drifted.insert(
        "players[2].velocity.x",
        f32::from_bits(3.5f32.to_bits() + 1),
    );
    write_dump_file(&a, &[(1, base_dump())]);
    write_dump_file(&b, &[(1, drifted)]);

    let lenient = render(&a, &b, &plain()).unwrap();
    let strict = render(
        &a,
        &b,
        &DiffOptions {
            policy: FloatPolicy::strict(),
            ..plain()
        },
    )
    .unwrap();
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    assert!(
        lenient
            .text
            .contains("0 structural, 0 exact, 1 sub-epsilon float drift")
    );
    assert!(lenient.text.contains("only sub-epsilon float drift"));
    assert!(
        strict
            .text
            .contains("0 structural, 1 exact, 0 sub-epsilon float drift")
    );
}

#[test]
fn colors_appear_only_when_asked() {
    let a = temp_path("color-a.dump");
    let b = temp_path("color-b.dump");
    let mut changed = base_dump();
    changed.insert("score", 1u64);
    write_dump_file(&a, &[(1, base_dump())]);
    write_dump_file(&b, &[(1, changed)]);

    let plain_out = render(&a, &b, &plain()).unwrap();
    let colored = render(
        &a,
        &b,
        &DiffOptions {
            color: true,
            ..plain()
        },
    )
    .unwrap();
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    assert!(!plain_out.text.contains("\x1b["));
    assert!(colored.text.contains("\x1b[33m"));
}

#[test]
fn flags_parse_and_reject_garbage() {
    let (a, b, options) = parse_args(&[
        "x.dump".to_string(),
        "--epsilon-f32".to_string(),
        "0.5".to_string(),
        "y.dump".to_string(),
        "--all".to_string(),
        "--no-color".to_string(),
    ])
    .unwrap();
    assert_eq!(a, "x.dump");
    assert_eq!(b, "y.dump");
    assert_eq!(options.policy.epsilon_f32, 0.5);
    assert!(options.show_all);
    assert!(!options.color);

    assert!(parse_args(&["only-one.dump".to_string()]).is_err());
    assert!(parse_args(&["a".to_string(), "b".to_string(), "--bogus".to_string()]).is_err());
    assert!(
        parse_args(&[
            "a".to_string(),
            "b".to_string(),
            "--epsilon-f64".to_string(),
            "abc".to_string()
        ])
        .is_err()
    );
}

#[test]
fn unmatched_ticks_and_missing_files_are_reported() {
    let a = temp_path("ticks-a.dump");
    let b = temp_path("ticks-b.dump");
    write_dump_file(&a, &[(100, base_dump()), (200, base_dump())]);
    write_dump_file(&b, &[(200, base_dump()), (300, base_dump())]);
    let output = render(&a, &b, &plain()).unwrap();
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();
    assert!(output.text.contains("only in first  dumps at ticks 100"));
    assert!(output.text.contains("only in second dumps at ticks 300"));

    let missing = temp_path("nope.dump");
    let code = tickwise_cli::run(&[
        "diff".to_string(),
        missing.display().to_string(),
        missing.display().to_string(),
    ]);
    assert_eq!(code, 2);
}
