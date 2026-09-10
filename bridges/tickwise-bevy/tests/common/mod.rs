//! A small deterministic Bevy simulation shared by the tests.
//!
//! Integer math and a private generator only, so two runs of the same
//! code agree bit for bit unless a test asks them not to.

use bevy_app::{App, FixedMain, FixedUpdate};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use std::path::PathBuf;
use tickwise_bevy::{TickwiseAppExt, TickwiseInputs, TickwisePlugin, TickwiseSession};

pub const BALLS: u32 = 4;
pub const ARENA: i32 = 16000;

#[derive(Component, Reflect, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Component, Reflect, Debug)]
pub struct Velocity {
    pub x: i32,
    pub y: i32,
}

#[derive(Resource, Reflect, Default, Debug)]
pub struct Rng {
    pub state: u32,
}

impl Rng {
    fn next(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        self.state
    }
}

#[derive(Resource, Reflect, Default, Debug)]
pub struct Score {
    pub value: u64,
}

/// The planted bug. From `from_tick` on, `nudge` is added to the first
/// ball's position, standing in for a value that should never have
/// reached the simulation.
#[derive(Resource, Reflect, Default, Debug)]
pub struct Chaos {
    pub from_tick: Option<u64>,
    pub nudge: i32,
}

/// Counts fixed steps so the chaos can strike at a known tick.
#[derive(Resource, Reflect, Default, Debug)]
pub struct SimTick {
    pub value: u64,
}

fn step(
    mut positions: Query<(&mut Position, &mut Velocity)>,
    mut rng: ResMut<Rng>,
    mut score: ResMut<Score>,
    mut tick: ResMut<SimTick>,
    chaos: Res<Chaos>,
    mut inputs: ResMut<TickwiseInputs>,
) {
    // Scripted inputs, so two runs differ only by the planted bug.
    let input = ((tick.value / 45) % 4) as u8;
    inputs.set(vec![input]);

    let nudge_x = if input & 1 != 0 { 8 } else { 0 };
    let nudge_y = if input & 2 != 0 { 8 } else { 0 };

    // Ordered by entity id, so the generator is drawn from in a stable
    // order no matter how the query iterates.
    let mut all: Vec<(Mut<Position>, Mut<Velocity>)> = positions.iter_mut().collect();
    all.sort_by_key(|(position, _)| (position.x, position.y));

    let mut first = true;
    for (mut position, mut velocity) in all {
        velocity.x = (velocity.x + nudge_x).clamp(-300, 300);
        velocity.y = (velocity.y + nudge_y).clamp(-300, 300);
        position.x += velocity.x;
        position.y += velocity.y;

        if position.x < 0 || position.x > ARENA {
            velocity.x = -velocity.x;
            position.x = position.x.clamp(0, ARENA);
            score.value += 1;
        }
        if position.y < 0 || position.y > ARENA {
            velocity.y = -velocity.y;
            position.y = position.y.clamp(0, ARENA);
            score.value += 1;
        }

        // Applied last, after the bounce, so the nudge is still in the
        // state when the tick is recorded. Applying it before the bounce
        // let a clamp erase it on the tick it struck, which moved the
        // divergence one tick later and made the test lie about when the
        // bug happened.
        if first && chaos.from_tick.is_some_and(|from| tick.value >= from) {
            position.x += chaos.nudge;
        }
        first = false;
    }

    let _ = rng.next();
    tick.value += 1;
}

/// How much of the world the recording covers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Positions in the full hash, score and generator in the light hash.
    /// A position-only bug is invisible to the light hash on the tick it
    /// strikes, which is the blind spot the full hash exists to close.
    PositionsInFullHashOnly,
    /// Positions in the light hash as well, so a position-only bug is
    /// caught on the tick it strikes.
    PositionsInLightHash,
}

/// Builds the app, runs `ticks` fixed steps, finishes the recording, and
/// returns the path it was written to.
pub fn record(
    path: PathBuf,
    ticks: u64,
    chaos: Chaos,
    coverage: Coverage,
    extra_entity: bool,
) -> PathBuf {
    let mut app = App::new();
    app.add_plugins(TickwisePlugin::new(&path).with_full_hash_interval(50));
    app.add_systems(FixedUpdate, step);
    app.insert_resource(Rng { state: 12345 });
    app.insert_resource(Score::default());
    app.insert_resource(SimTick::default());
    app.insert_resource(chaos);

    app.record_resource_in_light_hash::<Score>();
    app.record_resource_in_light_hash::<Rng>();
    app.record_component::<Velocity>();
    match coverage {
        Coverage::PositionsInFullHashOnly => app.record_component::<Position>(),
        Coverage::PositionsInLightHash => app.record_component_in_light_hash::<Position>(),
    };

    for index in 0..BALLS + u32::from(extra_entity) {
        let seed = index as i32;
        app.world_mut().spawn((
            Position {
                x: 1000 + seed * 700,
                y: 2000 + seed * 300,
            },
            Velocity {
                x: 40 + seed * 11,
                y: 55 - seed * 7,
            },
        ));
    }

    for _ in 0..ticks {
        app.world_mut().run_schedule(FixedMain);
    }

    let mut session = app.world_mut().resource_mut::<TickwiseSession>();
    assert!(session.error().is_none(), "{:?}", session.error());
    assert_eq!(session.tick(), ticks, "recorded tick count");
    session.finish();
    path
}

/// A unique temporary path per test, so tests can run in parallel.
pub fn temp_path(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("tickwise-bevy-{}-{name}.rec", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}
