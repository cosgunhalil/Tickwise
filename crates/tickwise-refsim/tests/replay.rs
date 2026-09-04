//! Pass 2 end to end on the reference simulation: record a session, then
//! replay it through a fresh world with hash verification on and capture
//! a state dump at a chosen tick.

use std::io::Cursor;
use tickwise::format::{Chunk, RecReader};
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, ReplayConfig, Replayer, Value};
use tickwise_refsim::{Lcg, PlayerInput, World, WorldConfig};

const TICKS: u64 = 1_000;
const SEED: u64 = 0x0DD_BA11;
const DUMP_TICK: u64 = 613;

fn encode(inputs: &[PlayerInput]) -> Vec<u8> {
    inputs
        .iter()
        .flat_map(|i| [i.move_x as u8, i.move_y as u8])
        .collect()
}

fn decode(bytes: &[u8]) -> Vec<PlayerInput> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| PlayerInput {
            move_x: pair[0] as i8,
            move_y: pair[1] as i8,
        })
        .collect()
}

fn record_session() -> (Vec<u8>, tickwise::StateDump) {
    let config = RecorderConfig {
        full_hash_interval: 100,
        input_format_id: 1,
        ..RecorderConfig::default()
    };
    let mut world = World::new(WorldConfig {
        seed: SEED,
        ..WorldConfig::default()
    });
    let mut input_rng = Lcg::new(4242);
    let mut rec = Recorder::new(Vec::new(), config).unwrap();
    let mut dump_at_target = None;
    for tick in 0..TICKS {
        let inputs: Vec<PlayerInput> = (0..2)
            .map(|_| PlayerInput {
                move_x: (input_rng.next_u64() % 3) as i8 - 1,
                move_y: (input_rng.next_u64() % 3) as i8 - 1,
            })
            .collect();
        world.step(&inputs);
        rec.record_tick(tick, &encode(&inputs), &world).unwrap();
        if tick == DUMP_TICK {
            dump_at_target = Some(world.state_dump());
        }
    }
    (rec.finish().unwrap(), dump_at_target.unwrap())
}

#[test]
fn replaying_a_session_verifies_every_hash_and_reproduces_the_dump() {
    let (recording, live_dump) = record_session();

    let mut reader = RecReader::open(Cursor::new(&recording)).unwrap();
    let mut rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            dump_at_ticks: vec![DUMP_TICK],
            verify_hashes: true,
            expected_input_format_id: Some(1),
        },
    )
    .unwrap();

    let mut world = World::new(WorldConfig {
        seed: SEED,
        ..WorldConfig::default()
    });
    let mut steps = 0;
    while let Some(step) = rep.next_step() {
        world.step(&decode(step.inputs()));
        rep.after_tick(&world).unwrap();
        steps += 1;
    }
    assert_eq!(steps, TICKS);

    let dump_file = rep.finish_into(Vec::new()).unwrap();
    let mut reader = RecReader::open(Cursor::new(&dump_file)).unwrap();
    let dumps: Vec<(u64, tickwise::StateDump)> = reader
        .chunks()
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|c| match c {
            Chunk::StateDump { tick, dump } => Some((tick, dump)),
            _ => None,
        })
        .collect();
    assert_eq!(dumps.len(), 1);
    assert_eq!(dumps[0].0, DUMP_TICK);
    // Tick counter is post-step, so the dump reports the completed tick.
    assert_eq!(dumps[0].1.get("tick"), Some(&Value::U64(DUMP_TICK + 1)));
    assert_eq!(dumps[0].1, live_dump);
    reader.verify_checksum().unwrap();
}
