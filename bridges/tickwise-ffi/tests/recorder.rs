//! The Pass 1 surface exercised from Rust: a full session read back with
//! the core reader, two sessions compared for a divergence, and every
//! misuse path returning a status instead of crashing.

use std::ffi::CStr;
use std::path::PathBuf;
use std::ptr;
use tickwise::compare::{HashKind, Outcome, first_divergence};
use tickwise::format::{Chunk, RecReader};
use tickwise_ffi::*;

/// A unique temp path per test so parallel tests never share a file.
fn temp_rec(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("tickwise-ffi-{}-{name}.rec", std::process::id()));
    path
}

fn last_error() -> String {
    // SAFETY: the function returns a valid NUL-terminated string that
    // lives until the next failing call on this thread, and this test
    // copies it immediately.
    unsafe { CStr::from_ptr(tickwise_last_error_message()) }
        .to_str()
        .unwrap()
        .to_owned()
}

fn config_with_meta() -> TickwiseRecorderConfig {
    let mut config = TickwiseRecorderConfig {
        game_id: ptr::null(),
        game_id_len: 0,
        build_hash: ptr::null(),
        build_hash_len: 0,
        platform: ptr::null(),
        platform_len: 0,
        tick_rate: 0,
        rng_seed: 0,
        created_at: 0,
        full_hash_interval: 0,
        snapshot_every: 0,
        hash_algo_id: 0,
        input_format_id: 0,
    };
    // SAFETY: a valid, writable config struct.
    assert_eq!(
        unsafe { tickwise_recorder_config_default(&mut config) },
        TickwiseStatus::Ok
    );
    config.game_id = b"ffi-test".as_ptr();
    config.game_id_len = 8;
    config.platform = b"test-host".as_ptr();
    config.platform_len = 9;
    config.tick_rate = 60;
    config.rng_seed = 99;
    config.full_hash_interval = 50;
    config.snapshot_every = 100;
    config.hash_algo_id = TICKWISE_HASH_ALGO_XXH3;
    config.input_format_id = 7;
    config
}

fn create(path: &std::path::Path, config: &TickwiseRecorderConfig) -> *mut TickwiseRecorder {
    let path_str = path.to_str().unwrap();
    let mut handle: *mut TickwiseRecorder = ptr::null_mut();
    // SAFETY: every pointer refers to live memory owned by this test for
    // the duration of the call.
    let status =
        unsafe { tickwise_recorder_create(path_str.as_ptr(), path_str.len(), config, &mut handle) };
    assert_eq!(status, TickwiseStatus::Ok, "{}", last_error());
    assert!(!handle.is_null());
    handle
}

/// Records `ticks` ticks whose light hash is a function of the tick and
/// of `divergence_at`: from that tick on, the hash carries an extra bit.
fn record_session(path: &std::path::Path, ticks: u64, divergence_at: Option<u64>) {
    let config = config_with_meta();
    let rec = create(path, &config);
    for tick in 0..ticks {
        let diverged = divergence_at.is_some_and(|at| tick >= at);
        let light = tick.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(diverged);
        // SAFETY: rec is a live handle owned by this test.
        let full = if unsafe { tickwise_recorder_wants_full_hash(rec, tick) } {
            light.wrapping_mul(31)
        } else {
            0
        };
        let inputs = [tick as u8, (tick / 2) as u8];
        // SAFETY: rec is live, inputs is a valid 2-byte slice.
        let status = unsafe {
            tickwise_recorder_record_tick(rec, tick, inputs.as_ptr(), inputs.len(), light, full)
        };
        assert_eq!(status, TickwiseStatus::Ok, "{}", last_error());
        // SAFETY: rec is live.
        if unsafe { tickwise_recorder_wants_snapshot(rec, tick) } {
            let data = b"snapshot-bytes";
            // SAFETY: rec is live, data is a valid slice.
            let status =
                unsafe { tickwise_recorder_record_snapshot(rec, tick, data.as_ptr(), data.len()) };
            assert_eq!(status, TickwiseStatus::Ok, "{}", last_error());
        }
    }
    let label = "round start";
    // SAFETY: rec is live, label is a valid UTF-8 slice.
    let status = unsafe { tickwise_recorder_record_marker(rec, 42, label.as_ptr(), label.len()) };
    assert_eq!(status, TickwiseStatus::Ok, "{}", last_error());
    // SAFETY: rec is live and finished exactly once here.
    assert_eq!(unsafe { tickwise_recorder_finish(rec) }, TickwiseStatus::Ok);
    // SAFETY: rec is live and destroyed exactly once here.
    unsafe { tickwise_recorder_destroy(rec) };
}

#[test]
fn recorded_session_reads_back_with_the_core_reader() {
    let path = temp_rec("roundtrip");
    record_session(&path, 150, None);

    let mut reader = RecReader::open(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(reader.tick_count(), 150);
    let header = reader.header();
    assert_eq!(header.meta.game_id, "ffi-test");
    assert_eq!(header.meta.platform, "test-host");
    assert_eq!(header.meta.build_hash, "");
    assert_eq!(header.meta.tick_rate, 60);
    assert_eq!(header.meta.rng_seed, 99);
    assert_eq!(header.config.full_hash_interval, 50);
    assert_eq!(header.config.hash_algo_id, TICKWISE_HASH_ALGO_XXH3);
    assert_eq!(header.config.input_format_id, 7);

    let mut full_ticks = Vec::new();
    let mut snapshot_ticks = Vec::new();
    let mut markers = Vec::new();
    let mut light_hashes = 0;
    for chunk in reader.chunks().unwrap() {
        match chunk.unwrap() {
            Chunk::FullHash { tick, hash } => {
                let expected = tick.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_mul(31);
                assert_eq!(hash, expected, "full hash at tick {tick}");
                full_ticks.push(tick);
            }
            Chunk::LightHashBatch { hashes, .. } => light_hashes += hashes.len(),
            Chunk::Snapshot { tick, data } => {
                assert_eq!(data, b"snapshot-bytes");
                snapshot_ticks.push(tick);
            }
            Chunk::Marker { tick, label } => markers.push((tick, label)),
            _ => {}
        }
    }
    assert_eq!(full_ticks, vec![0, 50, 100]);
    assert_eq!(snapshot_ticks, vec![0, 100]);
    assert_eq!(markers, vec![(42, "round start".to_owned())]);
    assert_eq!(light_hashes, 150);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn two_sessions_compare_to_the_injected_divergence() {
    let clean = temp_rec("clean");
    let chaotic = temp_rec("chaotic");
    record_session(&clean, 600, None);
    record_session(&chaotic, 600, Some(421));

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

    let same = first_divergence(&clean, &clean).unwrap();
    assert!(matches!(
        same.outcome,
        Outcome::Identical {
            ticks_compared: 600,
            ..
        }
    ));
    std::fs::remove_file(clean).unwrap();
    std::fs::remove_file(chaotic).unwrap();
}

#[test]
fn null_handles_are_reported_not_dereferenced() {
    let rec: *mut TickwiseRecorder = ptr::null_mut();
    // SAFETY: null is explicitly allowed by every handle contract.
    unsafe {
        assert_eq!(
            tickwise_recorder_record_tick(rec, 0, ptr::null(), 0, 0, 0),
            TickwiseStatus::NullPointer
        );
        assert!(last_error().contains("recorder is null"));
        assert!(!tickwise_recorder_wants_full_hash(rec, 0));
        assert!(!tickwise_recorder_wants_snapshot(rec, 0));
        assert_eq!(
            tickwise_recorder_record_snapshot(rec, 0, ptr::null(), 0),
            TickwiseStatus::NullPointer
        );
        assert_eq!(
            tickwise_recorder_record_marker(rec, 0, ptr::null(), 0),
            TickwiseStatus::NullPointer
        );
        assert_eq!(tickwise_recorder_finish(rec), TickwiseStatus::NullPointer);
        tickwise_recorder_destroy(rec);
        assert_eq!(
            tickwise_recorder_config_default(ptr::null_mut()),
            TickwiseStatus::NullPointer
        );
    }
}

#[test]
fn create_rejects_bad_arguments() {
    let config = config_with_meta();
    let mut handle: *mut TickwiseRecorder = ptr::null_mut();
    let path = temp_rec("never-created");
    let path_str = path.to_str().unwrap();
    // SAFETY: every non-null pointer refers to live memory owned by this
    // test; the null ones are the cases under test.
    unsafe {
        assert_eq!(
            tickwise_recorder_create(path_str.as_ptr(), path_str.len(), &config, ptr::null_mut()),
            TickwiseStatus::NullPointer
        );
        assert_eq!(
            tickwise_recorder_create(path_str.as_ptr(), path_str.len(), ptr::null(), &mut handle),
            TickwiseStatus::NullPointer
        );
        assert_eq!(
            tickwise_recorder_create(ptr::null(), 5, &config, &mut handle),
            TickwiseStatus::NullPointer
        );
        assert_eq!(
            tickwise_recorder_create(ptr::null(), 0, &config, &mut handle),
            TickwiseStatus::InvalidArgument
        );
        let bad_utf8 = [0xFFu8, 0xFE, 0x2E, 0x72, 0x65, 0x63];
        assert_eq!(
            tickwise_recorder_create(bad_utf8.as_ptr(), bad_utf8.len(), &config, &mut handle),
            TickwiseStatus::InvalidUtf8
        );
        let mut bad_meta = config;
        bad_meta.game_id = bad_utf8.as_ptr();
        bad_meta.game_id_len = 2;
        assert_eq!(
            tickwise_recorder_create(path_str.as_ptr(), path_str.len(), &bad_meta, &mut handle),
            TickwiseStatus::InvalidUtf8
        );
        assert!(last_error().contains("config.game_id"));
        let missing_dir = "definitely/not/a/dir/x.rec";
        assert_eq!(
            tickwise_recorder_create(
                missing_dir.as_ptr(),
                missing_dir.len(),
                &config,
                &mut handle
            ),
            TickwiseStatus::Io
        );
    }
    assert!(handle.is_null(), "out must be untouched on failure");
    assert!(!path.exists());
}

#[test]
fn misuse_after_create_returns_statuses() {
    let path = temp_rec("misuse");
    let config = config_with_meta();
    let rec = create(&path, &config);
    // SAFETY: rec is a live handle owned by this test; the null and
    // oversized arguments are the cases under test.
    unsafe {
        assert_eq!(
            tickwise_recorder_record_tick(rec, 0, ptr::null(), 4, 1, 0),
            TickwiseStatus::NullPointer
        );
        assert!(last_error().contains("inputs is null"));
        assert_eq!(
            tickwise_recorder_record_tick(rec, 0, ptr::null(), 0, 1, 0),
            TickwiseStatus::Ok
        );
        assert_eq!(
            tickwise_recorder_record_tick(rec, 5, ptr::null(), 0, 1, 0),
            TickwiseStatus::NonSequentialTick
        );
        assert!(last_error().contains("expected 1, got 5"));

        let long_label = vec![b'x'; 70_000];
        assert_eq!(
            tickwise_recorder_record_marker(rec, 0, long_label.as_ptr(), long_label.len()),
            TickwiseStatus::InvalidArgument
        );
        let bad_utf8 = [0xFFu8, 0xFE];
        assert_eq!(
            tickwise_recorder_record_marker(rec, 0, bad_utf8.as_ptr(), bad_utf8.len()),
            TickwiseStatus::InvalidUtf8
        );

        assert_eq!(tickwise_recorder_finish(rec), TickwiseStatus::Ok);
        assert_eq!(
            tickwise_recorder_finish(rec),
            TickwiseStatus::AlreadyFinished
        );
        assert_eq!(
            tickwise_recorder_record_tick(rec, 1, ptr::null(), 0, 1, 0),
            TickwiseStatus::AlreadyFinished
        );
        assert!(!tickwise_recorder_wants_full_hash(rec, 0));
        assert!(!tickwise_recorder_wants_snapshot(rec, 0));
        tickwise_recorder_destroy(rec);
    }

    let reader = RecReader::open(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(reader.tick_count(), 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn destroy_without_finish_does_not_crash() {
    let path = temp_rec("unfinished");
    let config = config_with_meta();
    let rec = create(&path, &config);
    // SAFETY: rec is live; destroying it without finish is allowed and
    // documented to leave an unreadable file.
    unsafe {
        assert_eq!(
            tickwise_recorder_record_tick(rec, 0, ptr::null(), 0, 1, 0),
            TickwiseStatus::Ok
        );
        tickwise_recorder_destroy(rec);
    }
    assert!(RecReader::open(std::fs::File::open(&path).unwrap()).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn status_names_and_error_message_are_static_strings() {
    for status in [
        TickwiseStatus::Ok,
        TickwiseStatus::NullPointer,
        TickwiseStatus::InvalidArgument,
        TickwiseStatus::InvalidUtf8,
        TickwiseStatus::Io,
        TickwiseStatus::NonSequentialTick,
        TickwiseStatus::AlreadyFinished,
        TickwiseStatus::Panic,
    ] {
        // SAFETY: the function returns a static NUL-terminated string.
        let name = unsafe { CStr::from_ptr(tickwise_status_name(status)) };
        assert!(!name.to_str().unwrap().is_empty());
    }
    assert_eq!(TickwiseStatus::Ok as i32, 0);
}

#[test]
fn xxh3_matches_the_core_hash() {
    let data = b"the same bytes on both sides";
    // SAFETY: data is a valid slice; the null cases are documented.
    unsafe {
        assert_eq!(
            tickwise_xxh3_64(data.as_ptr(), data.len()),
            xxhash_rust::xxh3::xxh3_64(data)
        );
        assert_eq!(
            tickwise_xxh3_64(ptr::null(), 0),
            xxhash_rust::xxh3::xxh3_64(&[])
        );
        assert_eq!(tickwise_xxh3_64(ptr::null(), 8), 0);
    }
    assert_eq!(TICKWISE_HASH_ALGO_USER_DEFINED, 0);
    assert_eq!(TICKWISE_HASH_ALGO_XXH3, 1);
    assert_eq!(TICKWISE_HASH_ALGO_BLAKE3, 2);
}
