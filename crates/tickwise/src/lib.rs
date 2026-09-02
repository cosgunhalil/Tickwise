//! Tickwise: record, replay, and diff deterministic simulations.
//!
//! Tickwise is an engine-agnostic recording, replay, and desync-debugging
//! toolkit for deterministic multiplayer games. It is an observer: you drive
//! your own game loop and call into the kit, and Tickwise never runs the
//! simulation itself.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod compare;
pub mod dump;
pub mod format;
pub mod probe;
pub mod recorder;

pub use dump::{StateDump, Value};
pub use format::{SessionMeta, SnapshotPolicy};
pub use probe::DeterminismProbe;
pub use recorder::{RecordError, Recorder, RecorderConfig};
