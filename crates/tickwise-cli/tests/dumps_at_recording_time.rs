//! End to end: two recordings that carry state dumps, compared and then
//! diffed straight from the `.rec` files with no replay in between.

use std::path::PathBuf;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, StateDump};
use tickwise_cli::diff::{DiffOptions, render as render_diff};

/// State whose every field follows the tick, with one field that drifts
/// from the strike tick on.
struct Sim {
    tick: u64,
    strike: Option<u64>,
}

impl Sim {
    fn score(&self) -> u64 {
        let drift = self.strike.is_some_and(|at| self.tick >= at);
        self.tick * 3 + u64::from(drift)
    }
}

impl DeterminismProbe for Sim {
    fn light_hash(&self) -> u64 {
        self.score()
    }
    fn full_hash(&self) -> u64 {
        self.score().wrapping_mul(31)
    }
    fn state_dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        dump.insert("tick", self.tick);
        dump.insert("score", self.score());
        dump.insert("lives", 3u64);
        dump
    }
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("tickwise-dumps-test-{}-{name}", std::process::id()))
}

fn record(path: &PathBuf, strike: Option<u64>) {
    let config = RecorderConfig {
        full_hash_interval: 100,
        dump_interval: 100,
        ..RecorderConfig::default()
    };
    let mut rec = Recorder::create(path, config).unwrap();
    let mut sim = Sim { tick: 0, strike };
    for tick in 0..500u64 {
        sim.tick = tick;
        rec.record_tick(tick, &[], &sim).unwrap();
    }
    rec.finish().unwrap();
}

fn args(command: &str, a: &PathBuf, b: &PathBuf, extra: &[&str]) -> Vec<String> {
    let mut out = vec![
        command.to_string(),
        a.display().to_string(),
        b.display().to_string(),
    ];
    out.extend(extra.iter().map(|s| s.to_string()));
    out
}

#[test]
fn compare_points_at_the_first_shared_dump_after_the_divergence() {
    let a = temp_path("cmp-clean.rec");
    let b = temp_path("cmp-chaotic.rec");
    record(&a, None);
    record(&b, Some(321));

    let output = tickwise_cli::compare::render(&a, &b).unwrap();
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    assert!(output.diverged);
    assert!(output.text.contains("first divergence at tick 321"));
    assert!(
        output.text.contains("both recordings carry state dumps"),
        "{}",
        output.text
    );
    assert!(output.text.contains("--at 400"), "{}", output.text);
    assert!(
        output
            .text
            .contains("tick 300 holds the last dump where they still agreed"),
        "{}",
        output.text
    );
    assert!(
        !output.text.contains("dump_at_ticks"),
        "no replay hint when dumps exist"
    );
}

#[test]
fn compare_falls_back_to_the_replay_hint_when_dumps_stop_before_the_strike() {
    let a = temp_path("late-clean.rec");
    let b = temp_path("late-chaotic.rec");
    record(&a, None);
    record(&b, Some(450));

    let output = tickwise_cli::compare::render(&a, &b).unwrap();
    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();

    // Dumps land at 0, 100, ..., 400 and the strike is at 450, so no
    // shared dump follows the divergence.
    assert!(output.text.contains("first divergence at tick 450"));
    assert!(
        output.text.contains("none at a shared tick"),
        "{}",
        output.text
    );
    assert!(
        output.text.contains("dump_at_ticks = [450]"),
        "{}",
        output.text
    );
}

#[test]
fn diff_reads_dumps_straight_from_recordings_at_the_named_tick() {
    let a = temp_path("diff-clean.rec");
    let b = temp_path("diff-chaotic.rec");
    record(&a, None);
    record(&b, Some(321));

    let options = DiffOptions {
        at: Some(400),
        ..DiffOptions::default()
    };
    let output = render_diff(&a, &b, &options).unwrap();
    let code = tickwise_cli::run(&args("diff", &a, &b, &["--at", "400", "--no-color"]));

    assert!(output.differs);
    assert!(output.text.contains("tick 400"), "{}", output.text);
    assert!(output.text.contains("score"), "{}", output.text);
    assert!(output.text.contains("1200 versus 1201"), "{}", output.text);
    assert!(
        !output.text.contains("tick 300"),
        "only the named tick is shown"
    );
    assert_eq!(code, 1);

    // Without --at every shared dump is diffed: agreement before the
    // strike, differences after it.
    let all = render_diff(&a, &b, &DiffOptions::default()).unwrap();
    assert!(all.differs);
    assert!(all.text.contains("tick 300"), "{}", all.text);
    assert!(all.text.contains("tick 400"), "{}", all.text);

    // A tick with no shared dump is a clear error, not an empty report.
    let missing = render_diff(
        &a,
        &b,
        &DiffOptions {
            at: Some(350),
            ..DiffOptions::default()
        },
    );
    let message = missing.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("no dump at tick 350"), "{message}");
    assert!(message.contains("0, 100, 200, 300, 400"), "{message}");
    let code = tickwise_cli::run(&args("diff", &a, &b, &["--at", "350"]));
    assert_eq!(code, 2);

    std::fs::remove_file(&a).unwrap();
    std::fs::remove_file(&b).unwrap();
}

#[test]
fn inspect_shows_the_dump_interval_and_the_dump_ticks() {
    let a = temp_path("inspect.rec");
    record(&a, None);
    let output = tickwise_cli::inspect::render(&a).unwrap();
    std::fs::remove_file(&a).unwrap();

    assert!(
        output.text.contains("state dumps    every 100 ticks"),
        "{}",
        output.text
    );
    assert!(output.text.contains("state dumps"), "{}", output.text);
    assert!(
        output.text.contains("at ticks 0, 100, 200, 300, 400"),
        "{}",
        output.text
    );
}
