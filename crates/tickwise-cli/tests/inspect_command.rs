//! End-to-end tests for the inspect command against real files.

use std::path::PathBuf;
use tickwise::format::SnapshotPolicy;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, SessionMeta, StateDump};

struct StubProbe;

impl DeterminismProbe for StubProbe {
    fn light_hash(&self) -> u64 {
        11
    }
    fn full_hash(&self) -> u64 {
        22
    }
    fn state_dump(&self) -> StateDump {
        StateDump::empty()
    }
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tickwise-cli-test-{}-{name}", std::process::id()))
}

fn write_recording(path: &PathBuf) {
    let config = RecorderConfig {
        session_meta: SessionMeta {
            game_id: "refsim".to_string(),
            build_hash: "cafe01".to_string(),
            platform: "test".to_string(),
            tick_rate: 60,
            rng_seed: 7,
            created_at: 1_756_400_000,
        },
        full_hash_interval: 50,
        snapshot: SnapshotPolicy::Every(100),
        input_format_id: 3,
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::create(path, config).unwrap();
    for tick in 0..150u64 {
        rec.record_tick(tick, &[(tick % 4) as u8], &StubProbe)
            .unwrap();
        if rec.wants_snapshot(tick) {
            rec.record_snapshot(tick, b"state").unwrap();
        }
    }
    rec.record_marker(42, "round start").unwrap();
    rec.finish().unwrap();
}

#[test]
fn inspect_reports_the_session_faithfully() {
    let path = temp_path("ok.rec");
    write_recording(&path);

    let report = tickwise_cli::inspect::render(&path).unwrap();
    std::fs::remove_file(&path).unwrap();

    assert!(!report.corrupt);
    let text = &report.text;
    assert!(text.contains("version 1"));
    assert!(text.contains("game           refsim"));
    assert!(text.contains("build          cafe01"));
    assert!(text.contains("60 ticks per second"));
    assert!(text.contains("full hashes    every 50 ticks"));
    assert!(text.contains("snapshots      every 100 ticks"));
    assert!(text.contains("input format   id 3"));
    assert!(text.contains("ticks          150"));
    assert!(text.contains("holding 150 hashes"));
    assert!(text.contains("at ticks 0, 100"));
    assert!(text.contains("checksum ok"));
    assert!(text.contains("tickwise compare"));
}

#[test]
fn inspect_flags_a_corrupted_file() {
    let path = temp_path("corrupt.rec");
    write_recording(&path);

    let mut bytes = std::fs::read(&path).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xFF;
    std::fs::write(&path, &bytes).unwrap();

    let result = tickwise_cli::inspect::render(&path);
    std::fs::remove_file(&path).unwrap();

    // A mid-file flip lands in the chunk stream: either open fails, or
    // the report survives and must carry the corruption verdict.
    if let Ok(report) = result {
        assert!(
            report.corrupt,
            "corruption was not flagged:\n{}",
            report.text
        );
    }
}

#[test]
fn inspect_rejects_a_missing_file() {
    let path = temp_path("does-not-exist.rec");
    assert!(tickwise_cli::inspect::render(&path).is_err());
}

#[test]
fn inspect_rejects_a_non_rec_file() {
    let path = temp_path("not-a-rec.txt");
    std::fs::write(&path, b"hello, this is not a recording").unwrap();
    let result = tickwise_cli::inspect::render(&path);
    std::fs::remove_file(&path).unwrap();
    assert!(result.is_err());
}

#[test]
fn cli_run_dispatches_and_reports_usage() {
    assert_eq!(tickwise_cli::run(&[]), 2);
    assert_eq!(tickwise_cli::run(&["help".to_string()]), 0);
    assert_eq!(tickwise_cli::run(&["--version".to_string()]), 0);
    assert_eq!(tickwise_cli::run(&["nonsense".to_string()]), 2);
    assert_eq!(tickwise_cli::run(&["compare".to_string()]), 2);
    assert_eq!(tickwise_cli::run(&["inspect".to_string()]), 2);

    let path = temp_path("cli.rec");
    write_recording(&path);
    let code = tickwise_cli::run(&["inspect".to_string(), path.display().to_string()]);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(code, 0);
}
