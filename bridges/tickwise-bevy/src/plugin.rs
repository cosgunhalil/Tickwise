//! The plugin: records one tick at the end of every fixed step.

use crate::probe::{BevyProbe, TickwiseScope};
use bevy_app::{App, FixedLast, Plugin};
use bevy_ecs::component::Component;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use bevy_reflect::{Reflect, TypePath};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use tickwise::recorder::{RecordError, Recorder, RecorderConfig};

/// The input bytes for the current tick.
///
/// Tickwise never interprets these; they are whatever your netcode
/// already sends. Write them from a system that runs before
/// [`FixedLast`], and set `input_format_id` on the recorder config so a
/// later replay refuses a recording made with an older encoding.
#[derive(Resource, Default)]
pub struct TickwiseInputs {
    /// The bytes recorded for this tick. Cleared by nobody: a tick with
    /// unchanged inputs simply records the same bytes, and the format
    /// stores a frame only when they change.
    pub bytes: Vec<u8>,
}

impl TickwiseInputs {
    /// Replaces the bytes for this tick.
    pub fn set(&mut self, bytes: impl Into<Vec<u8>>) {
        self.bytes = bytes.into();
    }
}

/// The open recording.
///
/// The plugin inserts one and drives it. Reach for it to place a marker,
/// to finish early, or to check whether recording stopped on an error.
#[derive(Resource)]
pub struct TickwiseSession {
    recorder: Option<Recorder<BufWriter<std::fs::File>>>,
    path: PathBuf,
    next_tick: u64,
    error: Option<RecordError>,
}

impl TickwiseSession {
    /// Opens a recording at `path`.
    pub fn create(path: impl AsRef<Path>, config: RecorderConfig) -> Result<Self, RecordError> {
        let path = path.as_ref().to_path_buf();
        Ok(Self {
            recorder: Some(Recorder::create(&path, config)?),
            path,
            next_tick: 0,
            error: None,
        })
    }

    /// A session that never opened, carrying the reason.
    fn failed(path: &Path, error: RecordError) -> Self {
        Self {
            recorder: None,
            path: path.to_path_buf(),
            next_tick: 0,
            error: Some(error),
        }
    }

    /// Where the recording is being written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The tick the next record call will use, which is also the number
    /// of ticks recorded so far.
    pub fn tick(&self) -> u64 {
        self.next_tick
    }

    /// True while ticks are still being recorded.
    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// The error that stopped the recording, when one did.
    ///
    /// Recording stops at the first failure rather than repeating it
    /// sixty times a second, and the game keeps running.
    pub fn error(&self) -> Option<&RecordError> {
        self.error.as_ref()
    }

    /// Records a marker at the current tick, for example a round start.
    pub fn marker(&mut self, label: &str) {
        let Some(recorder) = self.recorder.as_mut() else {
            return;
        };
        let tick = self.next_tick.saturating_sub(1);
        if let Err(err) = recorder.record_marker(tick, label) {
            self.fail(err);
        }
    }

    /// Flushes and closes the recording. Later ticks are not recorded.
    ///
    /// Called for you when the world drops, so a test or a run that just
    /// ends still leaves a readable file.
    pub fn finish(&mut self) {
        let Some(recorder) = self.recorder.take() else {
            return;
        };
        if let Err(err) = recorder.finish() {
            self.error.get_or_insert(err);
        }
    }

    fn record(&mut self, world: &World, scope: &TickwiseScope, inputs: &[u8]) {
        let Some(recorder) = self.recorder.as_mut() else {
            return;
        };
        let probe = BevyProbe::new(world, scope);
        let tick = self.next_tick;
        if let Err(err) = recorder.record_tick(tick, inputs, &probe) {
            self.fail(err);
            return;
        }
        self.next_tick += 1;
    }

    fn fail(&mut self, err: RecordError) {
        self.error = Some(err);
        // Drop the recorder without finishing: the file is already
        // unusable, and continuing would only produce more errors.
        self.recorder = None;
    }
}

impl Drop for TickwiseSession {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Records every fixed tick into a `.rec` file.
///
/// Add the plugin, declare which components and resources are gameplay
/// state, and run the app. The recording system sits in [`FixedLast`], so
/// it observes the world after your own fixed systems have stepped it.
///
/// Nothing is recorded until at least one type is registered, because a
/// probe over an empty scope hashes nothing and would report two
/// different simulations as identical.
///
/// # Examples
///
/// ```no_run
/// use bevy_app::App;
/// use bevy_ecs::prelude::*;
/// use bevy_reflect::Reflect;
/// use tickwise_bevy::{TickwiseAppExt, TickwisePlugin};
///
/// #[derive(Component, Reflect)]
/// struct Position { x: i32, y: i32 }
///
/// #[derive(Resource, Reflect, Default)]
/// struct Rng { state: u32 }
///
/// App::new()
///     .add_plugins(TickwisePlugin::new("session.rec"))
///     .record_component::<Position>()
///     .record_resource_in_light_hash::<Rng>()
///     .run();
/// ```
pub struct TickwisePlugin {
    path: PathBuf,
    config: RecorderConfig,
}

impl TickwisePlugin {
    /// Records to `path` with the default configuration: a full hash
    /// every 300 ticks and no snapshots.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            config: RecorderConfig::default(),
        }
    }

    /// Replaces the recorder configuration. Set the session metadata
    /// here: the game id, the build hash, and the seed, which is what
    /// makes two recordings comparable and what `tickwise inspect` shows.
    pub fn with_config(mut self, config: RecorderConfig) -> Self {
        self.config = config;
        self
    }

    /// Sets how often a full hash is recorded. Zero disables full hashes,
    /// which leaves the light hash blind spot uncovered.
    pub fn with_full_hash_interval(mut self, ticks: u32) -> Self {
        self.config.full_hash_interval = ticks;
        self
    }

    /// Sets how often a state dump is recorded during Pass 1. Zero, the
    /// default, records none.
    ///
    /// With dumps in both recordings, `tickwise diff a.rec b.rec` reaches
    /// field level with no replay, which is the only route for a desync
    /// that does not reproduce. Each dump walks every registered type, so
    /// this is the most expensive thing the plugin does per tick; measure
    /// before choosing an interval.
    pub fn with_dump_interval(mut self, ticks: u32) -> Self {
        self.config.dump_interval = ticks;
        self
    }
}

impl Plugin for TickwisePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TickwiseScope>();
        app.init_resource::<TickwiseInputs>();
        // A game that cannot open its recording still runs. The session
        // resource is inserted either way, carrying the error, so the
        // failure is observable through `TickwiseSession::error` rather
        // than only in the log.
        let session = match TickwiseSession::create(&self.path, self.config.clone()) {
            Ok(session) => session,
            Err(err) => {
                eprintln!("tickwise: cannot record to {}: {err}", self.path.display());
                TickwiseSession::failed(&self.path, err)
            }
        };
        app.insert_resource(session);
        app.add_systems(FixedLast, record_tick);
    }
}

/// Records one tick. Exclusive, because the probe reads the whole world
/// while the session is written.
fn record_tick(world: &mut World) {
    world.resource_scope(|world: &mut World, mut session: Mut<TickwiseSession>| {
        let world: &World = world;
        let Some(scope) = world.get_resource::<TickwiseScope>() else {
            return;
        };
        let inputs = world
            .get_resource::<TickwiseInputs>()
            .map_or(&[][..], |inputs| inputs.bytes.as_slice());
        session.record(world, scope, inputs);
    });
}

/// Declares which types are gameplay state, on the `App` itself.
///
/// Coverage decides what a comparison can catch. Anything left out is a
/// blind spot where a desync can hide, which is why nothing is included
/// by default.
pub trait TickwiseAppExt {
    /// Covers a component in the full hash and the dump.
    fn record_component<C: Component + Reflect + TypePath>(&mut self) -> &mut Self;

    /// Covers a component in the light hash as well, which runs on every
    /// tick. Reserve it for small, desync-critical state.
    fn record_component_in_light_hash<C: Component + Reflect + TypePath>(&mut self) -> &mut Self;

    /// Covers a resource in the full hash and the dump.
    fn record_resource<R: Resource + Reflect + TypePath>(&mut self) -> &mut Self;

    /// Covers a resource in the light hash as well. A random generator's
    /// state and a score belong here.
    fn record_resource_in_light_hash<R: Resource + Reflect + TypePath>(&mut self) -> &mut Self;
}

impl TickwiseAppExt for App {
    fn record_component<C: Component + Reflect + TypePath>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<TickwiseScope>()
            .component::<C>();
        self
    }

    fn record_component_in_light_hash<C: Component + Reflect + TypePath>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<TickwiseScope>()
            .light_component::<C>();
        self
    }

    fn record_resource<R: Resource + Reflect + TypePath>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<TickwiseScope>()
            .resource::<R>();
        self
    }

    fn record_resource_in_light_hash<R: Resource + Reflect + TypePath>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<TickwiseScope>()
            .light_resource::<R>();
        self
    }
}
