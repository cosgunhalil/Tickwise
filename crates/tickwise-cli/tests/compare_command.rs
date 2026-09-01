//! End-to-end tests for the compare command against real files.

use std::path::PathBuf;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, StateDump};

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

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tickwise-cmp-test-{}-{name}", std::process::id()))
}

fn write_recording(path: &PathBuf, diverge_at: Option<u64>) {
    let config = RecorderConfig {
        full_hash_interval: 100,
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::create(path, config).unwrap();
    let mut probe = ScriptedProbe { light: 0, full: 0 };
    for tick in 0..500u64 {
        let value = match diverge_at {
            Some(at) if tick >= at => tick + 1,
            _ => tick,
        };
        probe.light = value;
        probe.full = value;
        rec.record_tick(tick, &[], &probe).unwrap();
    }
    rec.finish().unwrap();
}

#[test]
fn identical_recordings_exit_zero_with_self_check_hint() {
    let a = temp_path("same-a.rec");
    let b = temp_path("same-b.rec");
    write_recording(&a, None);
    write_recording(&b, None);

    let output = tickwise_cli::compare::render(&a, &b).unwrap();
    let code = tickwise_cli::run(&[
        "compare".to_string(),
        a.display().to_string(),
        b.display().to_string(),
    ]);
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    assert!(!output.diverged);
    assert!(output.text.contains("identical over 500 compared ticks"));
    assert!(output.text.contains("self-check"));
    assert_eq!(code, 0);
}

#[test]
fn diverged_recordings_exit_one_with_pass_2_hint() {
    let a = temp_path("div-a.rec");
    let b = temp_path("div-b.rec");
    write_recording(&a, None);
    write_recording(&b, Some(321));

    let output = tickwise_cli::compare::render(&a, &b).unwrap();
    let code = tickwise_cli::run(&[
        "compare".to_string(),
        a.display().to_string(),
        b.display().to_string(),
    ]);
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    assert!(output.diverged);
    assert!(output.text.contains("first divergence at tick 321"));
    assert!(
        output
            .text
            .contains("confirmed by the full hash at tick 400")
    );
    assert!(output.text.contains("dump_at_ticks = [321]"));
    assert_eq!(code, 1);
}

#[test]
fn missing_files_exit_two() {
    let missing = temp_path("nope.rec");
    let code = tickwise_cli::run(&[
        "compare".to_string(),
        missing.display().to_string(),
        missing.display().to_string(),
    ]);
    assert_eq!(code, 2);
}
