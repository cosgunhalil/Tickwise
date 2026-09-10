//! End to end: run a Bevy app twice, once sabotaged, and confirm
//! Tickwise names the tick where the two runs stopped agreeing.

mod common;

use common::{Chaos, Coverage, record, temp_path};
use tickwise::compare::{HashKind, Outcome, first_divergence};

const TICKS: u64 = 600;
const STRIKE: u64 = 421;

fn clean() -> Chaos {
    Chaos::default()
}

fn sabotaged() -> Chaos {
    Chaos {
        from_tick: Some(STRIKE),
        nudge: 3,
    }
}

#[test]
fn two_clean_runs_are_identical() {
    let a = record(
        temp_path("clean-a"),
        TICKS,
        clean(),
        Coverage::PositionsInLightHash,
        false,
    );
    let b = record(
        temp_path("clean-b"),
        TICKS,
        clean(),
        Coverage::PositionsInLightHash,
        false,
    );

    let report = first_divergence(&a, &b).unwrap();
    assert!(
        matches!(
            report.outcome,
            Outcome::Identical {
                ticks_compared: TICKS,
                ..
            }
        ),
        "{:?}",
        report.outcome
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);

    std::fs::remove_file(a).unwrap();
    std::fs::remove_file(b).unwrap();
}

#[test]
fn a_covered_defect_is_found_on_the_tick_it_struck() {
    let clean_path = record(
        temp_path("covered-clean"),
        TICKS,
        clean(),
        Coverage::PositionsInLightHash,
        false,
    );
    let chaotic = record(
        temp_path("covered-chaotic"),
        TICKS,
        sabotaged(),
        Coverage::PositionsInLightHash,
        false,
    );

    let report = first_divergence(&clean_path, &chaotic).unwrap();
    match report.outcome {
        Outcome::Diverged(divergence) => {
            assert_eq!(divergence.tick, STRIKE);
            assert_eq!(divergence.detected_by, HashKind::Light);
            assert_eq!(divergence.last_agreeing_tick, Some(STRIKE - 1));
            assert_eq!(divergence.confirming_full_hash_tick, Some(450));
        }
        other => panic!("expected a divergence, got {other:?}"),
    }

    std::fs::remove_file(clean_path).unwrap();
    std::fs::remove_file(chaotic).unwrap();
}

#[test]
fn a_defect_outside_the_light_hash_is_caught_by_the_full_hash() {
    // Positions are in the full hash only, so the light hash cannot see a
    // position-only defect. This is the blind spot in miniature: compare
    // still finds the divergence, but later and through the other stream,
    // and it says so rather than pretending the light hash was enough.
    let clean_path = record(
        temp_path("blind-clean"),
        TICKS,
        clean(),
        Coverage::PositionsInFullHashOnly,
        false,
    );
    let chaotic = record(
        temp_path("blind-chaotic"),
        TICKS,
        sabotaged(),
        Coverage::PositionsInFullHashOnly,
        false,
    );

    let report = first_divergence(&clean_path, &chaotic).unwrap();
    match report.outcome {
        Outcome::Diverged(divergence) => {
            assert_eq!(divergence.detected_by, HashKind::Full);
            assert_eq!(divergence.tick, 450, "the next full hash after the strike");
            assert!(
                divergence.last_agreeing_tick.is_some_and(|tick| tick < 450),
                "{divergence:?}"
            );
        }
        other => panic!("expected a divergence, got {other:?}"),
    }

    std::fs::remove_file(clean_path).unwrap();
    std::fs::remove_file(chaotic).unwrap();
}

#[test]
fn an_extra_entity_diverges_from_the_first_tick() {
    // Component counts are part of the walk, so one machine spawning an
    // entity the other did not is caught immediately rather than when its
    // effects eventually reach the score.
    let four = record(
        temp_path("count-four"),
        30,
        clean(),
        Coverage::PositionsInLightHash,
        false,
    );
    let five = record(
        temp_path("count-five"),
        30,
        clean(),
        Coverage::PositionsInLightHash,
        true,
    );

    let report = first_divergence(&four, &five).unwrap();
    match report.outcome {
        Outcome::Diverged(divergence) => {
            assert_eq!(divergence.tick, 0);
            assert_eq!(divergence.last_agreeing_tick, None);
        }
        other => panic!("expected a divergence, got {other:?}"),
    }

    std::fs::remove_file(four).unwrap();
    std::fs::remove_file(five).unwrap();
}

#[test]
fn the_recording_is_readable_and_carries_what_it_should() {
    use tickwise::format::{Chunk, RecReader};

    let path = record(
        temp_path("structure"),
        150,
        clean(),
        Coverage::PositionsInLightHash,
        false,
    );
    let mut reader = RecReader::open(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(reader.tick_count(), 150);
    assert_eq!(reader.header().config.full_hash_interval, 50);
    // FNV-1a over the reflection walk, declared as caller defined.
    assert_eq!(reader.header().config.hash_algo_id, 0);

    let mut full_hashes = 0;
    let mut light_hashes = 0;
    let mut input_frames = 0;
    for chunk in reader.chunks().unwrap() {
        match chunk.unwrap() {
            Chunk::FullHash { .. } => full_hashes += 1,
            Chunk::LightHashBatch { hashes, .. } => light_hashes += hashes.len(),
            Chunk::InputFrame { .. } => input_frames += 1,
            _ => {}
        }
    }
    assert_eq!(light_hashes, 150);
    assert_eq!(full_hashes, 3, "ticks 0, 50, and 100");
    // Inputs change every 45 ticks and repeats are suppressed.
    assert!(
        (2..=5).contains(&input_frames),
        "{input_frames} input frames"
    );

    std::fs::remove_file(path).unwrap();
}
