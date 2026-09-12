//! C ABI over the Tickwise core.
//!
//! This crate is the foundation every non-Rust engine bridge stands on:
//! the Unity package over P/Invoke, the Unreal plugin, and the cocos2d-x
//! wrapper through the shared C++ layer. It exposes both passes of the
//! workflow as push calls: the caller computes hashes and builds dumps
//! and passes them in, so no callback ever crosses the boundary.
//!
//! # Contract
//!
//! Every exported function is `extern "C"`, prefixed `tickwise_`, and
//! obeys three rules:
//!
//! 1. It never panics across the boundary and never unwinds. A panic
//!    inside the crate is caught and reported as an error code.
//! 2. It is safe against misuse: null handles, zero lengths, double
//!    finish, out-of-order ticks, and protocol slips all return an error
//!    code.
//! 3. Pointers passed in are borrowed for the duration of the call only.
//!    The crate never stores a caller's pointer.
//!
//! # Surface
//!
//! - `tickwise_recorder_*`: Pass 1. Record inputs and hashes per tick,
//!   snapshots, markers, and state dumps on an interval or on demand.
//! - `tickwise_dump_*`: the dump builder, a state dump assembled field
//!   by field on the engine's side.
//! - `tickwise_replayer_*`: Pass 2. Step through a recording's inputs,
//!   verify live hashes, collect dumps at named ticks, write a `.dump`.
//! - `tickwise_xxh3_64` and the version queries.
//!
//! # Unsafe policy
//!
//! This is the only crate in the repository allowed to use `unsafe`.
//! `unsafe_op_in_unsafe_fn` is denied, so every unsafe operation sits in
//! its own block, and clippy's `undocumented_unsafe_blocks` lint makes a
//! missing `// SAFETY:` comment a build error.
//!
//! # Versioning
//!
//! The C surface may change freely while this crate is below 1.0. Once a
//! bridge is announced publicly, every change to the surface ships with a
//! migration note in the CHANGELOG, and [`TICKWISE_ABI_VERSION`] increments
//! on any incompatible change so a bridge can refuse a mismatched library
//! at load time.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(missing_docs)]

mod dump;
mod error;
mod hash;
mod recorder;
mod replayer;

pub use dump::{
    TickwiseDump, tickwise_dump_clear, tickwise_dump_destroy, tickwise_dump_len, tickwise_dump_new,
    tickwise_dump_set_bool, tickwise_dump_set_bytes, tickwise_dump_set_f32, tickwise_dump_set_f64,
    tickwise_dump_set_i64, tickwise_dump_set_len, tickwise_dump_set_null, tickwise_dump_set_str,
    tickwise_dump_set_u64,
};
pub use error::{TickwiseStatus, tickwise_last_error_message, tickwise_status_name};
pub use hash::{
    TICKWISE_HASH_ALGO_BLAKE3, TICKWISE_HASH_ALGO_USER_DEFINED, TICKWISE_HASH_ALGO_XXH3,
    tickwise_xxh3_64,
};
pub use recorder::{
    TickwiseRecorder, TickwiseRecorderConfig, tickwise_recorder_config_default,
    tickwise_recorder_create, tickwise_recorder_destroy, tickwise_recorder_finish,
    tickwise_recorder_record_dump, tickwise_recorder_record_marker,
    tickwise_recorder_record_snapshot, tickwise_recorder_record_tick, tickwise_recorder_wants_dump,
    tickwise_recorder_wants_full_hash, tickwise_recorder_wants_snapshot,
};
pub use replayer::{
    TickwiseReplayer, tickwise_replayer_after_tick, tickwise_replayer_destroy,
    tickwise_replayer_finish, tickwise_replayer_nearest_snapshot_before,
    tickwise_replayer_next_step, tickwise_replayer_open, tickwise_replayer_seek_to,
    tickwise_replayer_tick_range, tickwise_replayer_wants_dump, tickwise_replayer_wants_full_hash,
};

use std::ffi::c_char;

/// Version of the C surface. Increments on every incompatible change.
///
/// Bridges compare this against the value they were compiled for and
/// refuse to load a library that disagrees. Version 2 added the dump
/// builder, the replayer, `dump_interval` in the recorder config, and
/// six status codes.
pub const TICKWISE_ABI_VERSION: u32 = 2;

/// Crate version as a NUL-terminated string, for `tickwise_ffi_version`.
const VERSION_CSTR: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");

// Symbols are exported with the `tickwise_` prefix, which no other library
// in a game process is expected to use, so the unsafe(no_mangle) contract
// of unique symbol names holds.

/// Returns the version of the C surface, see `TICKWISE_ABI_VERSION`.
#[unsafe(no_mangle)]
pub extern "C" fn tickwise_ffi_abi_version() -> u32 {
    TICKWISE_ABI_VERSION
}

/// Returns the crate version as a NUL-terminated UTF-8 string, for
/// example `0.1.0`.
///
/// The pointer refers to static storage. It stays valid for the lifetime
/// of the process and must not be freed.
#[unsafe(no_mangle)]
pub extern "C" fn tickwise_ffi_version() -> *const c_char {
    VERSION_CSTR.as_ptr().cast()
}
