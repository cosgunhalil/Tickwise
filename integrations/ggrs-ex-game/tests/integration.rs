//! Tickwise against a real GGRS rollback session.

use std::io::Cursor;
use tickwise::compare::{Outcome, first_divergence_from};
use tickwise::diff::{DiffClass, FloatPolicy, diff_dumps};
use tickwise::format::RecReader;
use tickwise_ggrs_ex_game::{replay_recording, run_synctest_session};

const PLAYERS: usize = 2;
const FRAMES: usize = 300;

fn compare(a: &[u8], b: &[u8]) -> Outcome {
    let mut ra = RecReader::open(Cursor::new(a)).unwrap();
    let mut rb = RecReader::open(Cursor::new(b)).unwrap();
    first_divergence_from(&mut ra, &mut rb).unwrap().outcome
}

#[test]
fn recordings_are_invariant_to_the_rollback_pattern() {
    let shallow = run_synctest_session(PLAYERS, FRAMES, 2, None).unwrap();
    let deep = run_synctest_session(PLAYERS, FRAMES, 7, None).unwrap();

    // GGRS really did roll back, and every re-simulation matched the
    // recorded hash, otherwise the run would have failed.
    assert!(shallow.resimulated_frames > 0);
    assert!(deep.resimulated_frames > shallow.resimulated_frames);
    assert_eq!(shallow.frames_recorded, FRAMES as u64);

    // Different rollback depths, byte-identical recordings.
    assert_eq!(shallow.recording, deep.recording);
    assert_eq!(shallow.final_state, deep.final_state);
    assert!(matches!(
        compare(&shallow.recording, &deep.recording),
        Outcome::Identical { .. }
    ));
}

#[test]
fn a_plain_loop_replays_the_rollback_session_exactly() {
    let session = run_synctest_session(PLAYERS, FRAMES, 5, None).unwrap();
    let dump = replay_recording(&session.recording, PLAYERS, 200, None).unwrap();
    assert_eq!(dump.get("frame"), Some(&tickwise::Value::I64(200)));
    assert_eq!(
        dump.get("positions"),
        Some(&tickwise::Value::Len(PLAYERS as u64))
    );
}

#[test]
fn an_injected_defect_is_located_by_compare_and_named_by_diff() {
    let clean = run_synctest_session(PLAYERS, FRAMES, 5, None).unwrap();
    let buggy = run_synctest_session(PLAYERS, FRAMES, 5, Some(150)).unwrap();

    let tick = match compare(&clean.recording, &buggy.recording) {
        Outcome::Diverged(d) => d.tick,
        other => panic!("expected divergence, got {other:?}"),
    };
    assert_eq!(tick, 150);

    let dump_clean = replay_recording(&clean.recording, PLAYERS, tick, None).unwrap();
    let dump_buggy = replay_recording(&buggy.recording, PLAYERS, tick, Some(150)).unwrap();
    let diff = diff_dumps(tick, &dump_clean, &dump_buggy, &FloatPolicy::default());

    assert_eq!(diff.differences.len(), 1);
    assert_eq!(diff.differences[0].path, "positions[0][0]");
    assert_eq!(diff.differences[0].class, DiffClass::Exact);
}
