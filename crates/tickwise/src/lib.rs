//! Tickwise: record, replay, and diff deterministic simulations.
//!
//! Tickwise is an engine-agnostic recording, replay, and desync-debugging
//! toolkit for deterministic multiplayer games. It is an observer: you
//! drive your own game loop and call into the kit, and Tickwise never runs
//! the simulation itself. That is what keeps it engine-agnostic, and it is
//! why every API here is a callback into your code rather than a framework
//! around it.
//!
//! # The two-pass workflow
//!
//! A desync is found in two passes, and the split exists because Tickwise
//! cannot run your simulation for you.
//!
//! **Pass 1 is always on and cheap.** Each client records its inputs and a
//! hash per tick with a [`Recorder`]. Given two recordings,
//! [`compare::first_divergence`] reports the first tick where the hashes
//! disagree. It works offline from the `.rec` files alone, in
//! milliseconds.
//!
//! **Pass 2 is targeted.** You replay each recording through your own
//! simulation with a [`Replayer`], asking it to capture a [`StateDump`] at
//! the divergent tick. [`diff::structural`] then reports the field-level
//! differences between the two dumps, classified as structural, exact, or
//! sub-epsilon float drift.
//!
//! The `tickwise` command line tool in the `tickwise-cli` crate wraps
//! `compare` and `diff`, plus `inspect` for looking inside a recording.
//!
//! # The probe
//!
//! Everything hangs off one trait, [`DeterminismProbe`], with three
//! methods: a cheap [`light_hash`](DeterminismProbe::light_hash) called
//! every tick, a [`full_hash`](DeterminismProbe::full_hash) called every N
//! ticks that covers all gameplay state, and a
//! [`state_dump`](DeterminismProbe::state_dump) called only during Pass 2.
//! Implement it by hand for full control over cost and coverage, or enable
//! the `serde` feature and let [`serde_probe::SerdeProbe`] derive all
//! three from any `Serialize` state.
//!
//! # Recording
//!
//! ```
//! use tickwise::{DeterminismProbe, Recorder, RecorderConfig, StateDump};
//!
//! struct Game { tick: u64, score: u64 }
//!
//! impl DeterminismProbe for Game {
//!     fn light_hash(&self) -> u64 { self.tick ^ self.score.rotate_left(17) }
//!     fn full_hash(&self) -> u64 { self.light_hash() }
//!     fn state_dump(&self) -> StateDump {
//!         let mut dump = StateDump::empty();
//!         dump.insert("tick", self.tick);
//!         dump.insert("score", self.score);
//!         dump
//!     }
//! }
//!
//! # fn main() -> Result<(), tickwise::RecordError> {
//! let mut game = Game { tick: 0, score: 0 };
//! let mut rec = Recorder::new(Vec::new(), RecorderConfig::default())?;
//! for tick in 0..600 {
//!     let inputs = [0u8; 2];       // your own input encoding, opaque bytes
//!     game.tick += 1;              // your own simulation step
//!     rec.record_tick(tick, &inputs, &game)?;
//! }
//! let recording = rec.finish()?;  // the .rec bytes
//! # assert!(!recording.is_empty());
//! # Ok(())
//! # }
//! ```
//!
//! Inputs are opaque bytes; Tickwise records them and hands them back at
//! replay without ever interpreting them. Declare an
//! [`input_format_id`](RecorderConfig::input_format_id) so the replayer
//! can refuse recordings made with an older encoding.
//!
//! # Replaying
//!
//! ```no_run
//! use tickwise::{ReplayConfig, Replayer};
//! # use tickwise::{DeterminismProbe, StateDump};
//! # struct Game { tick: u64 }
//! # impl Game { fn apply(&mut self, _inputs: &[u8]) { self.tick += 1; } }
//! # impl DeterminismProbe for Game {
//! #     fn light_hash(&self) -> u64 { self.tick }
//! #     fn full_hash(&self) -> u64 { self.tick }
//! #     fn state_dump(&self) -> StateDump { StateDump::empty() }
//! # }
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut rep = Replayer::open("session.rec", ReplayConfig {
//!     dump_at_ticks: vec![4021],
//!     verify_hashes: true,
//!     ..ReplayConfig::default()
//! })?;
//! let mut game = Game { tick: 0 };
//! while let Some(step) = rep.next_step() {
//!     game.apply(step.inputs());   // your loop applies the inputs and ticks
//!     rep.after_tick(&game)?;      // dumps and hash verification happen here
//! }
//! rep.finish("session.dump")?;
//! # Ok(())
//! # }
//! ```
//!
//! With `verify_hashes` on, the replayer compares every live hash against
//! the recording and stops with [`ReplayError::HashMismatch`] the moment
//! your simulation stops reproducing the session. Snapshot resume follows
//! the same observer rule: [`Replayer::nearest_snapshot_before`] hands you
//! the bytes, you restore your own state, and [`Replayer::seek_to`] moves
//! the replay forward.
//!
//! # Module map
//!
//! - [`probe`]: the [`DeterminismProbe`] trait.
//! - [`recorder`]: Pass 1 recording.
//! - [`compare`]: first-divergence search over two recordings.
//! - [`replayer`]: Pass 2 replay with dumps and verification.
//! - [`dump`]: [`StateDump`] and [`Value`], the structural state model.
//! - [`diff`]: the classified structural diff over two dumps.
//! - [`format`](mod@format): the versioned `.rec` and `.dump` container.
//!   Malformed input never panics.
//! - [`serde_probe`]: the convenience layer behind the `serde` feature,
//!   with the automatic probe and typed inputs.
//!
//! # Features
//!
//! The core has zero dependencies. Two optional features add them:
//!
//! - `serde`: [`serde_probe`], typed inputs through
//!   [`Recorder::record_tick_typed`] and
//!   [`replayer::Step::inputs_typed`], and xxh3 hashing.
//! - `blake3`: the blake3 hash as an alternative, for audit scenarios.
//!   Implies `serde`.
//!
//! # What Tickwise is not
//!
//! Not a netcode or rollback library, not an engine plugin, not a
//! determinism linter, and not async. It records, replays, and diffs, and
//! it leaves the simulation to you.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod compare;
pub mod diff;
pub mod dump;
pub mod format;
pub mod probe;
pub mod recorder;
pub mod replayer;
#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
pub mod serde_probe;

pub use dump::{StateDump, Value};
pub use format::{SessionMeta, SnapshotPolicy};
pub use probe::DeterminismProbe;
pub use recorder::{RecordError, Recorder, RecorderConfig};
pub use replayer::{ReplayConfig, ReplayError, Replayer};
