//! Compiles `ctest/harness.c` against the built shared library with the
//! system C compiler, runs it, and reads the recordings it produced back
//! with the core reader.
//!
//! The recordings stay in `target/<profile>/ctest/` so CI can run
//! `tickwise compare` and `tickwise inspect` on them afterwards.

use std::path::{Path, PathBuf};
use std::process::Command;
use tickwise::compare::{HashKind, Outcome, first_divergence};
use tickwise::format::{Chunk, RecReader};

/// `target/<profile>/`, derived from the test executable's location:
/// `target/<profile>/deps/c_harness-<hash>`.
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn shared_library_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "tickwise_ffi.dll"
    } else if cfg!(target_os = "macos") {
        "libtickwise_ffi.dylib"
    } else {
        "libtickwise_ffi.so"
    }
}

/// Builds the harness executable and returns its path.
fn compile_harness(profile: &Path, out_dir: &Path) -> PathBuf {
    let source = crate_dir().join("ctest").join("harness.c");
    let include = crate_dir().join("include");
    let target = std::env::var("TARGET").unwrap_or_else(|_| host_target());
    let compiler = cc::Build::new()
        .target(&target)
        .host(&target)
        .opt_level(0)
        .cargo_metadata(false)
        .get_compiler();

    let exe_name = if cfg!(target_os = "windows") {
        "harness.exe"
    } else {
        "harness"
    };
    let exe = out_dir.join(exe_name);

    let mut cmd = compiler.to_command();
    if compiler.is_like_msvc() {
        let import_lib = profile.join("tickwise_ffi.dll.lib");
        cmd.arg("/nologo")
            .arg("/W4")
            .arg("/WX")
            .arg(format!("/I{}", include.display()))
            .arg(format!("/Fo{}\\", out_dir.display()))
            .arg(format!("/Fe{}", exe.display()))
            .arg(&source)
            .arg("/link")
            .arg(&import_lib);
    } else {
        cmd.arg("-std=c99")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Werror")
            .arg("-pedantic")
            .arg("-I")
            .arg(&include)
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .arg("-L")
            .arg(profile)
            .arg("-ltickwise_ffi")
            .arg(format!("-Wl,-rpath,{}", profile.display()));
    }

    let output = cmd.output().expect("failed to launch the C compiler");
    assert!(
        output.status.success(),
        "C compiler failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    exe
}

fn host_target() -> String {
    // rustc exposes the host triple to build scripts, not to tests, so
    // reconstruct the pieces the cc crate needs to pick a compiler.
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "windows" => format!("{arch}-pc-windows-msvc"),
        "macos" => format!("{arch}-apple-darwin"),
        "linux" => format!("{arch}-unknown-linux-gnu"),
        other => panic!("unsupported host os {other}"),
    }
}

/// `cargo test` links tests against the rlib only, so the cdylib the
/// harness loads is built here, into the same profile directory.
fn build_shared_library(profile: &Path) {
    let mut cargo = Command::new(env!("CARGO"));
    cargo
        .arg("build")
        .arg("--lib")
        .arg("--manifest-path")
        .arg(crate_dir().join("Cargo.toml"));
    if profile.file_name().is_some_and(|name| name == "release") {
        cargo.arg("--release");
    }
    let output = cargo.output().expect("failed to launch cargo");
    assert!(
        output.status.success(),
        "cargo build --lib failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn c_harness_records_sessions_the_core_can_read_and_compare() {
    let profile = profile_dir();
    build_shared_library(&profile);
    let library = profile.join(shared_library_name());
    assert!(
        library.exists(),
        "shared library not found at {} after cargo build --lib",
        library.display()
    );

    let out_dir = profile.join("ctest");
    std::fs::create_dir_all(&out_dir).unwrap();
    let exe = compile_harness(&profile, &out_dir);

    let clean = out_dir.join("clean.rec");
    let chaotic = out_dir.join("chaotic.rec");
    for stale in [&clean, &chaotic, &out_dir.join("misuse.rec")] {
        let _ = std::fs::remove_file(stale);
    }

    let mut run = Command::new(&exe);
    run.arg(&clean).arg(&chaotic).arg(&out_dir);
    // Belt and braces for the dynamic loader: the rpath covers Linux and
    // macOS, PATH covers Windows, and the environment variables cover a
    // loader that ignores the rpath.
    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_var = profile.clone().into_os_string();
    path_var.push(if cfg!(windows) { ";" } else { ":" });
    path_var.push(existing_path);
    run.env("PATH", path_var);
    run.env("LD_LIBRARY_PATH", &profile);
    run.env("DYLD_LIBRARY_PATH", &profile);

    let output = run.output().expect("failed to launch the harness");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "harness failed with {:?}:\n{stdout}\n{stderr}",
        output.status.code()
    );
    assert!(stdout.contains("harness ok"), "{stdout}");

    let mut reader = RecReader::open(std::fs::File::open(&clean).unwrap()).unwrap();
    assert_eq!(reader.tick_count(), 600);
    assert_eq!(reader.header().meta.game_id, "c-harness");
    assert_eq!(reader.header().meta.platform, "ctest");
    assert_eq!(reader.header().meta.tick_rate, 60);
    assert_eq!(reader.header().config.full_hash_interval, 50);
    assert_eq!(reader.header().config.hash_algo_id, 1);
    assert_eq!(reader.header().config.input_format_id, 42);
    let mut full_hashes = 0;
    let mut snapshots = 0;
    let mut markers = Vec::new();
    for chunk in reader.chunks().unwrap() {
        match chunk.unwrap() {
            Chunk::FullHash { .. } => full_hashes += 1,
            Chunk::Snapshot { data, .. } => {
                assert_eq!(data.len(), 44);
                snapshots += 1;
            }
            Chunk::Marker { tick, label } => markers.push((tick, label)),
            _ => {}
        }
    }
    assert_eq!(full_hashes, 12);
    assert_eq!(snapshots, 6);
    assert_eq!(markers, vec![(300, "round start".to_owned())]);

    let report = first_divergence(&clean, &chaotic).unwrap();
    match report.outcome {
        Outcome::Diverged(d) => {
            assert_eq!(d.tick, 421);
            assert_eq!(d.detected_by, HashKind::Light);
            assert_eq!(d.last_agreeing_tick, Some(420));
            assert_eq!(d.confirming_full_hash_tick, Some(450));
        }
        other => panic!("expected a divergence, got {other:?}"),
    }
    assert!(report.warnings.is_empty());

    let identical = first_divergence(&clean, &clean).unwrap();
    assert!(matches!(identical.outcome, Outcome::Identical { .. }));
}
