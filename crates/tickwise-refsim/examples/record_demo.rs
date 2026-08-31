//! Records a 6000 tick refsim session into the .rec file given as the
//! first argument, ready for tickwise inspect.

use tickwise::format::SnapshotPolicy;
use tickwise::{Recorder, RecorderConfig, SessionMeta};
use tickwise_refsim::{Lcg, PlayerInput, World, WorldConfig};

fn main() {
    let seed = 0x0DD_BA11u64;
    let config = RecorderConfig {
        session_meta: SessionMeta {
            game_id: "tickwise-refsim".to_string(),
            build_hash: "m1-dev".to_string(),
            platform: "windows-x86_64".to_string(),
            tick_rate: 60,
            rng_seed: seed,
            created_at: 1_756_400_000,
        },
        full_hash_interval: 300,
        snapshot: SnapshotPolicy::Every(1800),
        input_format_id: 1,
        ..RecorderConfig::default()
    };
    let mut world = World::new(WorldConfig {
        seed,
        ..WorldConfig::default()
    });
    let mut input_rng = Lcg::new(9001);
    let mut rec = Recorder::create(std::env::args().nth(1).unwrap(), config).unwrap();
    for tick in 0..6000u64 {
        let inputs: Vec<PlayerInput> = (0..2)
            .map(|_| PlayerInput {
                move_x: (input_rng.next_u64() % 3) as i8 - 1,
                move_y: (input_rng.next_u64() % 3) as i8 - 1,
            })
            .collect();
        let bytes: Vec<u8> = inputs
            .iter()
            .flat_map(|i| [i.move_x as u8, i.move_y as u8])
            .collect();
        world.step(&inputs);
        rec.record_tick(tick, &bytes, &world).unwrap();
        if rec.wants_snapshot(tick) {
            rec.record_snapshot(tick, b"demo snapshot bytes").unwrap();
        }
    }
    rec.record_marker(3000, "halfway point").unwrap();
    rec.finish().unwrap();
}
