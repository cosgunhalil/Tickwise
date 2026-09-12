//! Pass 2 over the C surface, exercised from Rust: record with dumps on an
//! interval through the push calls, replay the recording with the same
//! simulation, verify every hash, collect dumps at named ticks, write the
//! `.dump`, and read everything back with the core. Then every misuse
//! path.

use std::ffi::CStr;
use std::path::PathBuf;
use std::ptr;
use tickwise::diff::{FloatPolicy, structural};
use tickwise::format::{Chunk, RecReader};
use tickwise::{StateDump, Value};
use tickwise_ffi::*;

fn temp(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("tickwise-ffi-replay-{}-{name}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

fn last_error() -> String {
    // SAFETY: the function returns a valid NUL-terminated string that
    // lives until the next failing call on this thread.
    unsafe { CStr::from_ptr(tickwise_last_error_message()) }
        .to_str()
        .unwrap()
        .to_owned()
}

/// The simulation both passes run: hashes and a dump that follow the tick.
struct Sim {
    tick: u64,
    strike: Option<u64>,
}

impl Sim {
    fn score(&self) -> u64 {
        self.tick * 3 + u64::from(self.strike.is_some_and(|at| self.tick >= at))
    }
    fn light(&self) -> u64 {
        self.score().wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }
    fn full(&self) -> u64 {
        self.score().wrapping_mul(31)
    }
    /// Fills a dump handle the way an engine would, field by field.
    fn write_dump(&self, dump: *mut TickwiseDump) {
        // SAFETY: dump is a live handle owned by the test; every path and
        // value slice is a valid literal.
        unsafe {
            assert_eq!(tickwise_dump_clear(dump), TickwiseStatus::Ok);
            let set = |path: &str, status: TickwiseStatus| {
                assert_eq!(status, TickwiseStatus::Ok, "{path}: {}", last_error());
            };
            set(
                "tick",
                tickwise_dump_set_u64(dump, b"tick".as_ptr(), 4, self.tick),
            );
            set(
                "score",
                tickwise_dump_set_u64(dump, b"score".as_ptr(), 5, self.score()),
            );
            set(
                "alive",
                tickwise_dump_set_bool(dump, b"alive".as_ptr(), 5, true),
            );
            set(
                "units",
                tickwise_dump_set_len(dump, b"units".as_ptr(), 5, 2),
            );
            set(
                "units[0].x",
                tickwise_dump_set_f32(dump, b"units[0].x".as_ptr(), 10, 1.5),
            );
            set(
                "units[1].x",
                tickwise_dump_set_f64(dump, b"units[1].x".as_ptr(), 10, -2.25),
            );
            set(
                "units[0].hp",
                tickwise_dump_set_i64(dump, b"units[0].hp".as_ptr(), 11, -7),
            );
            set(
                "name",
                tickwise_dump_set_str(dump, b"name".as_ptr(), 4, "K\u{e4}se".as_ptr(), 5),
            );
            set(
                "blob",
                tickwise_dump_set_bytes(dump, b"blob".as_ptr(), 4, [9u8, 8, 7].as_ptr(), 3),
            );
            set("gone", tickwise_dump_set_null(dump, b"gone".as_ptr(), 4));
        }
    }
}

fn expected_dump(tick: u64, score: u64) -> StateDump {
    let mut dump = StateDump::empty();
    dump.insert("tick", tick);
    dump.insert("score", score);
    dump.insert("alive", true);
    dump.insert("units", Value::Len(2));
    dump.insert("units[0].x", 1.5f32);
    dump.insert("units[1].x", -2.25f64);
    dump.insert("units[0].hp", -7i64);
    dump.insert("name", "K\u{e4}se");
    dump.insert("blob", vec![9u8, 8, 7]);
    dump.insert("gone", Value::Null);
    dump
}

const TICKS: u64 = 300;

fn record(path: &std::path::Path, strike: Option<u64>) {
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
        dump_interval: 0,
    };
    // SAFETY: a valid, writable config struct.
    assert_eq!(
        unsafe { tickwise_recorder_config_default(&mut config) },
        TickwiseStatus::Ok
    );
    assert_eq!(config.dump_interval, 0, "the default records no dumps");
    config.full_hash_interval = 50;
    config.snapshot_every = 100;
    config.dump_interval = 100;
    config.input_format_id = 9;

    let path_str = path.to_str().unwrap();
    let mut rec: *mut TickwiseRecorder = ptr::null_mut();
    // SAFETY: every pointer refers to live memory owned by this test.
    let status =
        unsafe { tickwise_recorder_create(path_str.as_ptr(), path_str.len(), &config, &mut rec) };
    assert_eq!(status, TickwiseStatus::Ok, "{}", last_error());

    let dump = tickwise_dump_new();
    let mut sim = Sim { tick: 0, strike };
    for tick in 0..TICKS {
        sim.tick = tick;
        let inputs = [tick as u8];
        // SAFETY: rec and dump are live handles owned by this test.
        unsafe {
            let full = if tickwise_recorder_wants_full_hash(rec, tick) {
                sim.full()
            } else {
                0
            };
            assert_eq!(
                tickwise_recorder_record_tick(rec, tick, inputs.as_ptr(), 1, sim.light(), full),
                TickwiseStatus::Ok
            );
            assert_eq!(tickwise_recorder_wants_dump(rec, tick), tick % 100 == 0);
            if tickwise_recorder_wants_dump(rec, tick) {
                sim.write_dump(dump);
                assert_eq!(
                    tickwise_recorder_record_dump(rec, tick, dump),
                    TickwiseStatus::Ok
                );
            }
            if tickwise_recorder_wants_snapshot(rec, tick) {
                let bytes = tick.to_le_bytes();
                assert_eq!(
                    tickwise_recorder_record_snapshot(rec, tick, bytes.as_ptr(), 8),
                    TickwiseStatus::Ok
                );
            }
        }
    }
    // SAFETY: live handles, each released exactly once.
    unsafe {
        assert_eq!(tickwise_recorder_finish(rec), TickwiseStatus::Ok);
        tickwise_recorder_destroy(rec);
        tickwise_dump_destroy(dump);
    }
}

/// Replays `path`, dumping at `dump_at`, and returns the collected dumps
/// by reading back the `.dump` file the replayer wrote.
fn replay(
    path: &std::path::Path,
    dump_at: &[u64],
    strike: Option<u64>,
    out: &std::path::Path,
) -> (TickwiseStatus, String) {
    let path_str = path.to_str().unwrap();
    let out_str = out.to_str().unwrap();
    let mut rep: *mut TickwiseReplayer = ptr::null_mut();
    // SAFETY: every pointer refers to live memory owned by this test.
    let status = unsafe {
        tickwise_replayer_open(
            path_str.as_ptr(),
            path_str.len(),
            if dump_at.is_empty() {
                ptr::null()
            } else {
                dump_at.as_ptr()
            },
            dump_at.len(),
            true,
            true,
            9,
            &mut rep,
        )
    };
    assert_eq!(status, TickwiseStatus::Ok, "{}", last_error());

    let dump = tickwise_dump_new();
    let mut sim = Sim { tick: 0, strike };
    let mut tick = 0u64;
    let mut inputs: *const u8 = ptr::null();
    let mut inputs_len = 0usize;
    let mut last = TickwiseStatus::Ok;
    let mut message = String::new();
    // SAFETY: rep and dump are live handles; the out pointers are locals.
    unsafe {
        let mut first = 0;
        let mut last_tick = 0;
        assert_eq!(
            tickwise_replayer_tick_range(rep, &mut first, &mut last_tick),
            TickwiseStatus::Ok
        );
        assert_eq!((first, last_tick), (0, TICKS - 1));

        while tickwise_replayer_next_step(rep, &mut tick, &mut inputs, &mut inputs_len) {
            assert_eq!(inputs_len, 1);
            assert_eq!(
                *inputs, tick as u8,
                "the recorded input comes back per tick"
            );
            sim.tick = tick;
            let full = if tickwise_replayer_wants_full_hash(rep, tick) {
                sim.full()
            } else {
                0
            };
            let dump_ptr = if tickwise_replayer_wants_dump(rep, tick) {
                sim.write_dump(dump);
                dump as *const TickwiseDump
            } else {
                ptr::null()
            };
            last = tickwise_replayer_after_tick(rep, sim.light(), full, dump_ptr);
            if last != TickwiseStatus::Ok {
                message = last_error();
                break;
            }
        }
        // A mismatch ends the caller's loop, not the session: the dumps
        // taken so far still reach the file.
        let finished = tickwise_replayer_finish(rep, out_str.as_ptr(), out_str.len());
        assert_eq!(finished, TickwiseStatus::Ok, "{}", last_error());
        assert_eq!(
            tickwise_replayer_finish(rep, out_str.as_ptr(), out_str.len()),
            TickwiseStatus::AlreadyFinished
        );
        tickwise_replayer_destroy(rep);
        tickwise_dump_destroy(dump);
    }
    (last, message)
}

fn dumps_in(path: &std::path::Path) -> Vec<(u64, StateDump)> {
    let mut reader = RecReader::open(std::fs::File::open(path).unwrap()).unwrap();
    reader
        .chunks()
        .unwrap()
        .map(Result::unwrap)
        .filter_map(|c| match c {
            Chunk::StateDump { tick, dump } => Some((tick, dump)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_recording_replays_through_the_c_surface_and_its_dumps_read_back() {
    let rec = temp("clean.rec");
    let out = temp("clean.dump");
    record(&rec, None);

    // Pass 1 dumps landed on the interval with every value type intact.
    let recorded = dumps_in(&rec);
    let ticks: Vec<u64> = recorded.iter().map(|(t, _)| *t).collect();
    assert_eq!(ticks, vec![0, 100, 200]);
    assert_eq!(recorded[1].1, expected_dump(100, 300));
    let reader = RecReader::open(std::fs::File::open(&rec).unwrap()).unwrap();
    assert_eq!(reader.header().config.dump_interval, 100);

    // Pass 2 verifies every hash and collects dumps at the named ticks.
    let (status, message) = replay(&rec, &[42, 250], None, &out);
    assert_eq!(status, TickwiseStatus::Ok, "{message}");
    let replayed = dumps_in(&out);
    let ticks: Vec<u64> = replayed.iter().map(|(t, _)| *t).collect();
    assert_eq!(ticks, vec![42, 250]);
    assert_eq!(replayed[1].1, expected_dump(250, 750));

    // The .dump the replayer wrote diffs cleanly against itself.
    let report = structural(&out, &out, FloatPolicy::default()).unwrap();
    assert!(report.is_identical());

    std::fs::remove_file(rec).unwrap();
    std::fs::remove_file(out).unwrap();
}

#[test]
fn a_replay_that_does_not_reproduce_is_caught_at_its_tick_and_keeps_its_dumps() {
    let rec = temp("chaotic.rec");
    let out = temp("chaotic.dump");
    // Recorded clean, replayed with a defect from tick 150: the replay
    // itself is what diverges, the way a nondeterministic bug behaves.
    record(&rec, None);
    let (status, message) = replay(&rec, &[100, 150], Some(150), &out);
    assert_eq!(status, TickwiseStatus::HashMismatch);
    assert!(message.contains("tick 150"), "{message}");

    // The dump at the divergent tick was taken before verification, so
    // it survives, and the earlier one with it.
    let replayed = dumps_in(&out);
    let ticks: Vec<u64> = replayed.iter().map(|(t, _)| *t).collect();
    assert_eq!(ticks, vec![100, 150]);
    assert_eq!(replayed[1].1.get("score"), Some(&Value::U64(451)));

    std::fs::remove_file(rec).unwrap();
    std::fs::remove_file(out).unwrap();
}

#[test]
fn opening_refuses_the_wrong_input_format_and_out_of_range_dump_ticks() {
    let rec = temp("open.rec");
    record(&rec, None);
    let path = rec.to_str().unwrap();
    let mut rep: *mut TickwiseReplayer = ptr::null_mut();
    // SAFETY: every pointer refers to live memory owned by this test; the
    // null and oversized arguments are the cases under test.
    unsafe {
        assert_eq!(
            tickwise_replayer_open(
                path.as_ptr(),
                path.len(),
                ptr::null(),
                0,
                true,
                true,
                8,
                &mut rep
            ),
            TickwiseStatus::InputFormatMismatch
        );
        assert!(rep.is_null(), "out untouched on failure");
        let far = [TICKS + 5];
        assert_eq!(
            tickwise_replayer_open(
                path.as_ptr(),
                path.len(),
                far.as_ptr(),
                1,
                true,
                false,
                0,
                &mut rep
            ),
            TickwiseStatus::TickOutOfRange
        );
        assert_eq!(
            tickwise_replayer_open(
                path.as_ptr(),
                path.len(),
                ptr::null(),
                3,
                true,
                false,
                0,
                &mut rep
            ),
            TickwiseStatus::NullPointer
        );
        let missing = "definitely/not/here.rec";
        assert_eq!(
            tickwise_replayer_open(
                missing.as_ptr(),
                missing.len(),
                ptr::null(),
                0,
                true,
                false,
                0,
                &mut rep
            ),
            TickwiseStatus::Io
        );
        assert_eq!(
            tickwise_replayer_open(
                path.as_ptr(),
                path.len(),
                ptr::null(),
                0,
                true,
                false,
                0,
                ptr::null_mut()
            ),
            TickwiseStatus::NullPointer
        );
        // Unchecked input format opens fine whatever the id.
        assert_eq!(
            tickwise_replayer_open(
                path.as_ptr(),
                path.len(),
                ptr::null(),
                0,
                false,
                false,
                12345,
                &mut rep
            ),
            TickwiseStatus::Ok
        );
        tickwise_replayer_destroy(rep);
    }
    std::fs::remove_file(rec).unwrap();
}

#[test]
fn protocol_slips_and_missing_dumps_are_statuses_not_crashes() {
    let rec = temp("protocol.rec");
    let out = temp("protocol.dump");
    record(&rec, None);
    let path = rec.to_str().unwrap();
    let out_str = out.to_str().unwrap();
    let mut rep: *mut TickwiseReplayer = ptr::null_mut();
    let dumps_at = [2u64];
    // SAFETY: every pointer refers to live memory owned by this test.
    unsafe {
        assert_eq!(
            tickwise_replayer_open(
                path.as_ptr(),
                path.len(),
                dumps_at.as_ptr(),
                1,
                false,
                false,
                0,
                &mut rep
            ),
            TickwiseStatus::Ok
        );
        // after_tick with no step pending.
        assert_eq!(
            tickwise_replayer_after_tick(rep, 0, 0, ptr::null()),
            TickwiseStatus::ProtocolMisuse
        );
        let mut tick = 0;
        let mut inputs: *const u8 = ptr::null();
        let mut len = 0;
        assert!(tickwise_replayer_next_step(
            rep,
            &mut tick,
            &mut inputs,
            &mut len
        ));
        assert_eq!(tick, 0);
        assert_eq!(
            tickwise_replayer_after_tick(rep, 0, 0, ptr::null()),
            TickwiseStatus::Ok
        );
        assert!(tickwise_replayer_next_step(
            rep,
            &mut tick,
            &mut inputs,
            &mut len
        ));
        assert_eq!(
            tickwise_replayer_after_tick(rep, 0, 0, ptr::null()),
            TickwiseStatus::Ok
        );

        // Tick 2 owes a dump: null is refused and the step stays pending.
        assert!(tickwise_replayer_next_step(
            rep,
            &mut tick,
            &mut inputs,
            &mut len
        ));
        assert_eq!(tick, 2);
        assert!(tickwise_replayer_wants_dump(rep, 2));
        assert!(
            !tickwise_replayer_wants_full_hash(rep, 0),
            "verification is off"
        );
        assert_eq!(
            tickwise_replayer_after_tick(rep, 0, 0, ptr::null()),
            TickwiseStatus::MissingDump
        );
        let dump = tickwise_dump_new();
        assert_eq!(tickwise_dump_len(dump), 0);
        assert_eq!(
            tickwise_dump_set_u64(dump, b"x".as_ptr(), 1, 1),
            TickwiseStatus::Ok
        );
        assert_eq!(
            tickwise_dump_set_u64(dump, ptr::null(), 0, 1),
            TickwiseStatus::InvalidArgument
        );
        assert_eq!(
            tickwise_dump_set_u64(dump, ptr::null(), 3, 1),
            TickwiseStatus::NullPointer
        );
        assert_eq!(
            tickwise_dump_set_u64(ptr::null_mut(), b"x".as_ptr(), 1, 1),
            TickwiseStatus::NullPointer
        );
        assert_eq!(tickwise_dump_len(dump), 1);
        assert_eq!(
            tickwise_replayer_after_tick(rep, 0, 0, dump),
            TickwiseStatus::Ok
        );

        // Skipping a step's after_tick is caught at finish.
        assert!(tickwise_replayer_next_step(
            rep,
            &mut tick,
            &mut inputs,
            &mut len
        ));
        assert!(tickwise_replayer_next_step(
            rep,
            &mut tick,
            &mut inputs,
            &mut len
        ));
        assert_eq!(
            tickwise_replayer_after_tick(rep, 0, 0, ptr::null()),
            TickwiseStatus::Ok
        );
        assert_eq!(
            tickwise_replayer_finish(rep, out_str.as_ptr(), out_str.len()),
            TickwiseStatus::ProtocolMisuse
        );
        // A failed finish leaves the session alive; destroy still works.
        assert!(!tickwise_replayer_next_step(
            ptr::null_mut(),
            &mut tick,
            &mut inputs,
            &mut len
        ));
        assert_eq!(
            tickwise_replayer_seek_to(rep, TICKS + 10),
            TickwiseStatus::TickOutOfRange
        );
        tickwise_replayer_destroy(rep);
        tickwise_dump_destroy(dump);
        tickwise_dump_destroy(ptr::null_mut());
        tickwise_replayer_destroy(ptr::null_mut());
    }
    std::fs::remove_file(rec).unwrap();
    let _ = std::fs::remove_file(out);
}

#[test]
fn snapshots_seek_the_replay_forward() {
    let rec = temp("seek.rec");
    let out = temp("seek.dump");
    record(&rec, None);
    let path = rec.to_str().unwrap();
    let out_str = out.to_str().unwrap();
    let mut rep: *mut TickwiseReplayer = ptr::null_mut();
    // SAFETY: every pointer refers to live memory owned by this test.
    unsafe {
        assert_eq!(
            tickwise_replayer_open(
                path.as_ptr(),
                path.len(),
                ptr::null(),
                0,
                true,
                false,
                0,
                &mut rep
            ),
            TickwiseStatus::Ok
        );
        let mut snap_tick = 0;
        let mut data: *const u8 = ptr::null();
        let mut len = 0;
        assert!(tickwise_replayer_nearest_snapshot_before(
            rep,
            250,
            &mut snap_tick,
            &mut data,
            &mut len
        ));
        assert_eq!(snap_tick, 200, "snapshots landed every 100 ticks");
        assert_eq!(len, 8);
        assert_eq!(
            u64::from_le_bytes(std::slice::from_raw_parts(data, 8).try_into().unwrap()),
            200
        );

        // Restore from the snapshot, resume at the next tick, and verify
        // the rest of the recording.
        assert_eq!(
            tickwise_replayer_seek_to(rep, snap_tick + 1),
            TickwiseStatus::Ok
        );
        let mut sim = Sim {
            tick: 0,
            strike: None,
        };
        let mut tick = 0;
        let mut inputs: *const u8 = ptr::null();
        let mut inputs_len = 0;
        let mut steps = 0;
        while tickwise_replayer_next_step(rep, &mut tick, &mut inputs, &mut inputs_len) {
            if steps == 0 {
                assert_eq!(tick, 201);
            }
            sim.tick = tick;
            let full = if tickwise_replayer_wants_full_hash(rep, tick) {
                sim.full()
            } else {
                0
            };
            assert_eq!(
                tickwise_replayer_after_tick(rep, sim.light(), full, ptr::null()),
                TickwiseStatus::Ok
            );
            steps += 1;
        }
        assert_eq!(steps, TICKS - 201);
        assert_eq!(
            tickwise_replayer_finish(rep, out_str.as_ptr(), out_str.len()),
            TickwiseStatus::Ok
        );
        tickwise_replayer_destroy(rep);
    }
    std::fs::remove_file(rec).unwrap();
    std::fs::remove_file(out).unwrap();
}
