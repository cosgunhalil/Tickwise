//! The M0 definition of done: the reference simulation runs 10,000 ticks
//! deterministically.
//!
//! Two worlds built from the same seed and fed the same scripted inputs
//! must produce identical hashes on every single tick. This exercises the
//! world through the public probe interface, exactly the way the recorder
//! will in M1.

use tickwise::DeterminismProbe;
use tickwise_refsim::{Lcg, PlayerInput, World, WorldConfig};

const TICKS: u64 = 10_000;
const FULL_HASH_INTERVAL: u64 = 300;
const WORLD_SEED: u64 = 0x0DD_BA11;

/// Deterministic input script, generated from its own LCG so the test
/// covers moving players without hardcoding 10,000 input frames.
fn scripted_inputs(rng: &mut Lcg, player_count: usize) -> Vec<PlayerInput> {
    (0..player_count)
        .map(|_| PlayerInput {
            move_x: (rng.next_u64() % 3) as i8 - 1,
            move_y: (rng.next_u64() % 3) as i8 - 1,
        })
        .collect()
}

#[test]
fn same_seed_and_inputs_give_identical_hashes_over_10k_ticks() {
    let config = WorldConfig {
        seed: WORLD_SEED,
        ..WorldConfig::default()
    };
    let player_count = config.player_count as usize;

    let mut world_a = World::new(config.clone());
    let mut world_b = World::new(config);
    let mut inputs_a = Lcg::new(9001);
    let mut inputs_b = Lcg::new(9001);

    assert_eq!(world_a.full_hash(), world_b.full_hash());

    for tick in 0..TICKS {
        let frame_a = scripted_inputs(&mut inputs_a, player_count);
        let frame_b = scripted_inputs(&mut inputs_b, player_count);
        assert_eq!(frame_a, frame_b, "input script diverged at tick {tick}");

        world_a.step(&frame_a);
        world_b.step(&frame_b);

        assert_eq!(
            world_a.light_hash(),
            world_b.light_hash(),
            "light hash diverged at tick {tick}"
        );

        if tick % FULL_HASH_INTERVAL == 0 {
            assert_eq!(
                world_a.full_hash(),
                world_b.full_hash(),
                "full hash diverged at tick {tick}"
            );
        }
    }

    assert_eq!(world_a.full_hash(), world_b.full_hash());
    assert_eq!(world_a.tick_count(), TICKS);

    // A frozen or trivial world would pass the equality checks above, so
    // prove the simulation actually did something.
    assert!(world_a.score() > 0, "no scoring events in 10k ticks");
}

#[test]
fn different_seeds_diverge_quickly() {
    let mut world_a = World::new(WorldConfig {
        seed: 1,
        ..WorldConfig::default()
    });
    let mut world_b = World::new(WorldConfig {
        seed: 2,
        ..WorldConfig::default()
    });

    assert_ne!(world_a.full_hash(), world_b.full_hash());

    for _ in 0..10 {
        world_a.step(&[]);
        world_b.step(&[]);
    }
    assert_ne!(world_a.light_hash(), world_b.light_hash());
}
