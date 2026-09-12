//! The end to end test: build the extension, run the Godot project twice
//! headless, and confirm Tickwise names the tick where the two runs
//! stopped agreeing.
//!
//! Godot is found through the `GODOT_BIN` environment variable, or as
//! `godot` on the path. Without it the test reports that it was skipped
//! and passes, so a contributor with no engine installed can still run
//! `cargo test`. Continuous integration sets `TICKWISE_REQUIRE_GODOT=1`,
//! which turns a missing engine into a failure instead.

use std::path::{Path, PathBuf};
use std::process::Command;
use tickwise::compare::{HashKind, Outcome, first_divergence};

const TICKS: u64 = 600;
const STRIKE: u64 = 421;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn library_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "tickwise_godot.dll"
    } else if cfg!(target_os = "macos") {
        "libtickwise_godot.dylib"
    } else {
        "libtickwise_godot.so"
    }
}

/// The Godot binary, or None when the engine is not available.
fn godot_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GODOT_BIN") {
        let path = PathBuf::from(path);
        assert!(
            path.exists(),
            "GODOT_BIN points at {}, which does not exist",
            path.display()
        );
        return Some(path);
    }
    let probe = Command::new("godot").arg("--version").output();
    match probe {
        Ok(output) if output.status.success() => Some(PathBuf::from("godot")),
        _ => None,
    }
}

/// `target/<profile>/`, derived from the test executable's location.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Builds the extension and copies it where the project expects it.
///
/// `cargo test` builds the test binary but not the cdylib the engine
/// loads, so the build happens here and the test is self-contained on a
/// fresh checkout.
fn build_and_stage() -> PathBuf {
    let output = Command::new(env!("CARGO"))
        .arg("build")
        .arg("--lib")
        .arg("--manifest-path")
        .arg(crate_dir().join("Cargo.toml"))
        .output()
        .expect("failed to launch cargo");
    assert!(
        output.status.success(),
        "cargo build --lib failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let built = profile_dir().join(library_name());
    assert!(
        built.exists(),
        "the extension was not built at {}",
        built.display()
    );
    let project = crate_dir().join("godot");
    let bin = project.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(&built, bin.join(library_name())).unwrap();
    project
}

/// Runs the project headless and returns its output.
fn run_godot(godot: &Path, project: &Path, args: &[&str]) -> (bool, String) {
    let mut command = Command::new(godot);
    command
        .arg("--headless")
        .arg("--path")
        .arg(project)
        .args(args);
    let output = command.output().expect("failed to launch godot");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

/// Records one session and returns the path it was written to.
fn record(godot: &Path, project: &Path, name: &str, chaos: bool) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("tickwise-godot-{}-{name}.rec", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let path_arg = path.to_str().unwrap().to_string();
    let mut args = vec![
        "--script",
        "res://main.gd",
        "--",
        "--out",
        path_arg.as_str(),
    ];
    if chaos {
        args.push("--chaos");
    }
    let (ok, text) = run_godot(godot, project, &args);
    assert!(
        ok && !text.contains("FAIL:"),
        "the {name} session failed:\n{text}"
    );
    assert!(
        path.exists(),
        "the {name} session wrote no recording:\n{text}"
    );
    path
}

#[test]
fn two_runs_of_a_godot_project_compare_to_the_injected_divergence() {
    let Some(godot) = godot_binary() else {
        if std::env::var_os("TICKWISE_REQUIRE_GODOT").is_some() {
            panic!("TICKWISE_REQUIRE_GODOT is set but no Godot binary was found");
        }
        println!(
            "skipped: no Godot binary. Set GODOT_BIN to a Godot 4.6 executable to run this test."
        );
        return;
    };

    let project = build_and_stage();

    // The engine only loads extensions listed in .godot/extension_list.cfg,
    // which an import pass generates. A fresh checkout has no .godot at
    // all, so the import runs first and every later run is fast.
    let (_, import_log) = run_godot(&godot, &project, &["--import"]);
    assert!(
        project.join(".godot/extension_list.cfg").exists(),
        "the import pass did not register the extension:\n{import_log}"
    );

    let clean = record(&godot, &project, "clean", false);
    let chaotic = record(&godot, &project, "chaotic", true);
    let clean_again = record(&godot, &project, "clean-again", false);

    // Determinism first: the same run twice must agree everywhere. A
    // divergence here would mean the probe reads something that is not
    // part of the simulation.
    let repeat = first_divergence(&clean, &clean_again).unwrap();
    assert!(
        matches!(
            repeat.outcome,
            Outcome::Identical {
                ticks_compared: TICKS,
                ..
            }
        ),
        "two identical runs disagreed: {:?}",
        repeat.outcome
    );

    let report = first_divergence(&clean, &chaotic).unwrap();
    match report.outcome {
        Outcome::Diverged(divergence) => {
            assert_eq!(divergence.tick, STRIKE);
            assert_eq!(divergence.detected_by, HashKind::Light);
            assert_eq!(divergence.last_agreeing_tick, Some(STRIKE - 1));
            assert_eq!(divergence.confirming_full_hash_tick, Some(450));
        }
        other => panic!("expected a divergence, got {other:?}"),
    }
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    for path in [clean, chaotic, clean_again] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn the_recording_carries_the_session_metadata() {
    let Some(godot) = godot_binary() else {
        if std::env::var_os("TICKWISE_REQUIRE_GODOT").is_some() {
            panic!("TICKWISE_REQUIRE_GODOT is set but no Godot binary was found");
        }
        println!("skipped: no Godot binary.");
        return;
    };
    use tickwise::format::{Chunk, RecReader};

    let project = build_and_stage();
    run_godot(&godot, &project, &["--import"]);
    let path = record(&godot, &project, "metadata", false);

    let mut reader = RecReader::open(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(reader.tick_count(), TICKS);
    assert_eq!(reader.header().meta.game_id, "tickwise-godot-tests");
    assert_eq!(reader.header().meta.build_hash, "test-build");
    assert_eq!(reader.header().meta.rng_seed, 12345);
    assert_eq!(
        reader.header().meta.tick_rate,
        60,
        "taken from the project's physics rate when none is set"
    );
    assert_eq!(reader.header().config.full_hash_interval, 50);
    assert_eq!(reader.header().config.input_format_id, 7);
    // FNV-1a over the variant walk, declared as caller defined.
    assert_eq!(reader.header().config.hash_algo_id, 0);

    let mut light = 0;
    let mut full = 0;
    let mut inputs = 0;
    for chunk in reader.chunks().unwrap() {
        match chunk.unwrap() {
            Chunk::LightHashBatch { hashes, .. } => light += hashes.len(),
            Chunk::FullHash { .. } => full += 1,
            Chunk::InputFrame { .. } => inputs += 1,
            _ => {}
        }
    }
    assert_eq!(light, TICKS as usize);
    assert_eq!(full, 12, "every 50 ticks over 600");
    assert!(inputs > 1, "scripted inputs change during the session");

    let _ = std::fs::remove_file(path);
}
