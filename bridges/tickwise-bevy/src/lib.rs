//! Record, replay, and diff deterministic Bevy simulations.
//!
//! [Tickwise] finds the first tick where two runs of the same simulation
//! stopped agreeing. This crate is its Bevy bridge: a probe that reads
//! gameplay state through `Reflect`, and a plugin that records one tick at
//! the end of every fixed step.
//!
//! # The shape of it
//!
//! ```no_run
//! use bevy_app::App;
//! use bevy_ecs::prelude::*;
//! use bevy_reflect::Reflect;
//! use tickwise_bevy::{TickwiseAppExt, TickwisePlugin};
//!
//! #[derive(Component, Reflect)]
//! struct Position { x: i32, y: i32 }
//!
//! #[derive(Resource, Reflect, Default)]
//! struct Rng { state: u32 }
//!
//! App::new()
//!     .add_plugins(TickwisePlugin::new("session.rec"))
//!     .record_component::<Position>()
//!     .record_resource_in_light_hash::<Rng>()
//!     .run();
//! ```
//!
//! Run the game twice, or once on each of two machines, then compare the
//! recordings offline:
//!
//! ```text
//! cargo install tickwise-cli
//! tickwise compare a.rec b.rec
//! ```
//!
//! # What gets covered
//!
//! Nothing, until you say so. A Bevy world holds timers, window handles,
//! and asset ids that have no place in a determinism hash, so the four
//! [`TickwiseAppExt`] methods declare what counts as gameplay state.
//! Anything left out is a blind spot where a desync can hide, and the
//! [hash coverage checklist] walks through the choice.
//!
//! Registered types are read through `Reflect` into a flat list of path
//! and value pairs: `Position[3].x` for a component field on the entity
//! with index 3, `Rng.state` for a resource field, and `Position.len` for
//! the number of entities carrying the component. Collections emit their
//! length, so a shorter list never hides behind a matching tail, and maps
//! and sets are walked in sorted order, because a hash map's iteration
//! order is not part of your simulation and must never reach a hash.
//!
//! # Determinism notes
//!
//! Entities are walked in order of entity index rather than archetype
//! order, since archetype order is an allocation detail. The comparison
//! assumes the two runs allocated the same entity indices, which is true
//! of a simulation that is deterministic in the first place, and false in
//! a way worth knowing about if it is not.
//!
//! Hashes are FNV-1a over paths and value bits, recorded under
//! `hash_algo_id` 0, meaning caller defined. Recordings from this crate
//! are comparable with each other, not with recordings whose hashes came
//! from somewhere else.
//!
//! # Scope
//!
//! Pass 1 of the Tickwise workflow: recording, and `tickwise compare` to
//! find the first divergent tick. Pass 2, replaying to that tick and
//! diffing the state field by field, is available in the Rust core today
//! and reaches this crate later. [`BevyProbe`] already implements
//! `state_dump`, so the diff has everything it needs once the replay
//! driver lands.
//!
//! [Tickwise]: https://github.com/cosgunhalil/Tickwise
//! [hash coverage checklist]: https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod plugin;
mod probe;
mod walk;

pub use plugin::{TickwiseAppExt, TickwiseInputs, TickwisePlugin, TickwiseSession};
pub use probe::{BevyProbe, TickwiseScope};

// Re-exported so a game does not need a direct tickwise dependency just
// to configure a recording or read an error.
pub use tickwise::recorder::{RecordError, RecorderConfig};
pub use tickwise::{DeterminismProbe, SessionMeta, StateDump, Value};
