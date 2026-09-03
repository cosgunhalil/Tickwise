//! A headless Bones ECS game recorded, replayed, and diffed through the
//! schema-walking probe, plus the snapshot completeness check that the
//! Bones community described as their pain point.

use bones_ecs::prelude::*;
use std::io::Cursor;
use tickwise::compare::{HashKind, Outcome, first_divergence_from};
use tickwise::diff::{DiffClass, FloatPolicy, diff_dumps};
use tickwise::format::RecReader;
use tickwise::{DeterminismProbe, Recorder, RecorderConfig, ReplayConfig, Replayer, StateDump};
use tickwise_bones_ecs::BonesProbe;

#[derive(HasSchema, Clone, Default, Debug)]
#[repr(C)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(HasSchema, Clone, Default, Debug)]
#[repr(C)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(HasSchema, Clone, Default, Debug)]
#[repr(C)]
struct Score {
    value: u64,
    bounces: u32,
}

#[derive(HasSchema, Clone, Default, Debug)]
#[repr(C)]
struct Tick {
    value: u64,
}

const ENTITIES: u32 = 6;
const TICKS: u64 = 500;

fn build_world() -> World {
    let mut world = World::new();
    world.init_resource::<Score>();
    world.init_resource::<Tick>();
    {
        let mut entities = world.resource_mut::<Entities>();
        let mut positions = world.components.get::<Position>().borrow_mut();
        let mut velocities = world.components.get::<Velocity>().borrow_mut();
        for i in 0..ENTITIES {
            let entity = entities.create();
            positions.insert(
                entity,
                Position {
                    x: 10.0 * (i as f32 + 1.0),
                    y: 5.0,
                },
            );
            velocities.insert(
                entity,
                Velocity {
                    x: 0.5 + i as f32 * 0.25,
                    y: -0.75 + i as f32 * 0.1,
                },
            );
        }
    }
    world
}

/// One simulation step. The bug, when enabled, skips the velocity update
/// for entity index 3 from tick 250 on: a plain gameplay defect.
fn step(world: &World, inputs: &[u8], buggy: bool) {
    let mut tick = world.resource_mut::<Tick>();
    tick.value += 1;
    let mut score = world.resource_mut::<Score>();
    let entities = world.resource::<Entities>();
    let mut positions = world.components.get::<Position>().borrow_mut();
    let mut velocities = world.components.get::<Velocity>().borrow_mut();
    let thrust = inputs.first().copied().unwrap_or(0) as f32 * 0.01;

    let targets: Vec<Entity> = entities.iter_with_bitset(positions.bitset()).collect();
    for entity in targets {
        let Some(vel) = velocities.get_mut(entity) else {
            continue;
        };
        if buggy && tick.value >= 250 && entity.index() == 3 {
            continue;
        }
        vel.x += thrust;
        let pos = positions.get_mut(entity).unwrap();
        pos.x += vel.x;
        pos.y += vel.y;
        if pos.x < 0.0 || pos.x > 100.0 {
            vel.x = -vel.x;
            pos.x = pos.x.clamp(0.0, 100.0);
            score.bounces += 1;
            score.value += 10;
        }
        if pos.y < 0.0 || pos.y > 50.0 {
            vel.y = -vel.y;
            pos.y = pos.y.clamp(0.0, 50.0);
            score.bounces += 1;
        }
    }
}

fn probe(world: &World) -> BonesProbe<'_> {
    BonesProbe::new(world)
        .component::<Position>()
        .component::<Velocity>()
        .resource::<Score>()
        .light_resource::<Tick>()
        .light_resource::<Score>()
}

fn inputs_for(tick: u64) -> [u8; 1] {
    [((tick / 20) % 3) as u8]
}

fn record(buggy: bool) -> Vec<u8> {
    let world = build_world();
    let mut rec = Recorder::new(
        Vec::new(),
        RecorderConfig {
            full_hash_interval: 50,
            input_format_id: 7,
            ..RecorderConfig::default()
        },
    )
    .unwrap();
    for tick in 0..TICKS {
        let inputs = inputs_for(tick);
        step(&world, &inputs, buggy);
        rec.record_tick(tick, &inputs, &probe(&world)).unwrap();
    }
    rec.finish().unwrap()
}

fn replay(recording: &[u8], buggy: bool, at: u64) -> StateDump {
    let mut reader = RecReader::open(Cursor::new(recording)).unwrap();
    let mut rep = Replayer::from_reader(
        &mut reader,
        ReplayConfig {
            dump_at_ticks: vec![at],
            verify_hashes: true,
            expected_input_format_id: Some(7),
        },
    )
    .unwrap();
    let world = build_world();
    while let Some(step_info) = rep.next_step() {
        step(&world, step_info.inputs(), buggy);
        rep.after_tick(&probe(&world)).unwrap();
    }
    rep.into_dumps().unwrap().remove(0).1
}

#[test]
fn the_dump_has_schema_derived_paths() {
    let world = build_world();
    let dump = probe(&world).state_dump();
    assert_eq!(dump.get("entities"), Some(&tickwise::Value::Len(6)));
    assert_eq!(dump.get("Position[0].x"), Some(&tickwise::Value::F32(10.0)));
    assert_eq!(
        dump.get("Velocity[5].y"),
        Some(&tickwise::Value::F32(-0.25))
    );
    assert_eq!(dump.get("Score.value"), Some(&tickwise::Value::U64(0)));
    assert_eq!(dump.get("Tick.value"), Some(&tickwise::Value::U64(0)));
    // 1 entity count + 2 score fields + 1 tick field + 6 entities * 4 fields.
    assert_eq!(dump.len(), 1 + 2 + 1 + 6 * 4);
}

#[test]
fn light_hash_covers_only_the_light_set() {
    let world = build_world();
    let before_light = probe(&world).light_hash();
    let before_full = probe(&world).full_hash();

    // Moving a position changes the full hash, not the light one.
    world
        .components
        .get::<Position>()
        .borrow_mut()
        .get_mut(world.resource::<Entities>().iter().next().unwrap())
        .unwrap()
        .x += 1.0;
    assert_eq!(probe(&world).light_hash(), before_light);
    assert_ne!(probe(&world).full_hash(), before_full);

    // Changing the score changes both.
    world.resource_mut::<Score>().value += 1;
    assert_ne!(probe(&world).light_hash(), before_light);
}

#[test]
fn snapshot_restore_reproduces_hashes_exactly() {
    // The Bones pain point: is a cloned world a complete snapshot? Run
    // ahead, restore the clone, run again, and demand identical hashes.
    let world = build_world();
    for tick in 0..100 {
        step(&world, &inputs_for(tick), false);
    }
    let snapshot = world.clone();
    let mut hashes_first = Vec::new();
    for tick in 100..200 {
        step(&world, &inputs_for(tick), false);
        hashes_first.push(probe(&world).full_hash());
    }

    // In Bones ECS 0.4.0 a snapshot is a cloned World, and restoring it
    // means continuing from the clone. Later Bones versions add
    // load_snapshot for the same purpose.
    let restored = snapshot;
    assert_eq!(probe(&restored).full_hash(), {
        let w = build_world();
        for tick in 0..100 {
            step(&w, &inputs_for(tick), false);
        }
        probe(&w).full_hash()
    });
    let mut hashes_second = Vec::new();
    for tick in 100..200 {
        step(&restored, &inputs_for(tick), false);
        hashes_second.push(probe(&restored).full_hash());
    }
    assert_eq!(hashes_first, hashes_second);
}

#[test]
fn a_gameplay_defect_is_located_and_named() {
    let clean = record(false);
    let buggy = record(true);

    let mut ra = RecReader::open(Cursor::new(&clean)).unwrap();
    let mut rb = RecReader::open(Cursor::new(&buggy)).unwrap();
    let report = first_divergence_from(&mut ra, &mut rb).unwrap();
    let tick = match report.outcome {
        Outcome::Diverged(d) => {
            // Positions are not in the light set, so the light hash never
            // notices the frozen entity. The full hash at the next
            // interval, tick 250, does: a blind spot, reported as such.
            assert_eq!(d.detected_by, HashKind::Full);
            d.tick
        }
        other => panic!("expected divergence, got {other:?}"),
    };
    assert_eq!(tick, 250);

    let dump_clean = replay(&clean, false, tick);
    let dump_buggy = replay(&buggy, true, tick);
    let diff = diff_dumps(tick, &dump_clean, &dump_buggy, &FloatPolicy::default());
    let paths: Vec<&str> = diff.differences.iter().map(|d| d.path.as_str()).collect();
    assert!(
        paths
            .iter()
            .all(|p| p.starts_with("Position[3]") || p.starts_with("Velocity[3]"))
    );
    // The skipped update freezes the position. Velocity may or may not
    // differ, depending on whether the input applied thrust that tick.
    assert!(paths.contains(&"Position[3].x"));
    assert!(paths.contains(&"Position[3].y"));
    assert!(diff.differences.iter().all(|d| d.class == DiffClass::Exact));
}
