//! Record, replay, and diff deterministic Godot simulations.
//!
//! [Tickwise] finds the first tick where two runs of the same simulation
//! stopped agreeing. This crate is its Godot 4 bridge, a GDExtension
//! built with [gdext] that talks to the Rust core directly rather than
//! through the C ABI.
//!
//! # The shape of it
//!
//! Put your simulation nodes in the `tickwise` group, add a
//! [`TickwiseRecorder`] node, and start it:
//!
//! ```gdscript
//! extends Node
//!
//! @onready var tickwise: TickwiseRecorder = $TickwiseRecorder
//!
//! func _ready() -> void:
//!     tickwise.game_id = "my-game"
//!     tickwise.rng_seed = match_seed
//!     tickwise.start_recording("user://clean.rec")
//!
//! func _physics_process(_delta: float) -> void:
//!     tickwise.set_inputs(PackedByteArray([input_bits]))
//! ```
//!
//! Run the game twice, or once on each of two machines, then compare the
//! recordings offline:
//!
//! ```text
//! cargo install tickwise-cli
//! tickwise compare clean.rec chaotic.rec
//! ```
//!
//! # What gets covered
//!
//! Only the script variables of nodes in the `tickwise` group. A Godot
//! node carries transforms, visibility, and editor state that have no
//! place in a determinism hash, so a node's engine properties stay out
//! and a `var` declared in your script goes in. Nodes also in the
//! `tickwise_light` group enter the light hash, which runs every tick.
//!
//! Anything outside those groups is a blind spot where a desync can
//! hide, and the [hash coverage checklist] walks through the choice.
//!
//! Values are read into a flat list of path and value pairs, keyed by
//! absolute node path: `/root/Game/Ball1.velocity.x` for a `Vector2`
//! field, `/root/Game/Board.cells` for a dictionary's length, and
//! `/root/Game/Board.cells[a1]` for one of its entries.
//!
//! # Determinism rules the walk follows
//!
//! Nodes are sorted by absolute path and dictionary keys by their
//! rendered form, because group membership order and insertion order are
//! engine details rather than part of the simulation. Collections emit
//! their length, so a shorter array never hides behind a matching tail.
//! Object references are recorded as their class name and never as an
//! address, since an address differs on every machine.
//!
//! Hashes are FNV-1a over paths and value bits, recorded under
//! `hash_algo_id` 0, meaning caller defined. Recordings from this crate
//! compare with each other, not with recordings whose hashes came from
//! somewhere else.
//!
//! # Scope
//!
//! Pass 1 of the Tickwise workflow: recording, and `tickwise compare` to
//! find the first divergent tick. Pass 2, replaying to that tick and
//! diffing the state field by field, is available in the Rust core today
//! and reaches this crate later. The probe already produces a full state
//! dump, which [`TickwiseRecorder::state_snapshot`] exposes to GDScript.
//!
//! [Tickwise]: https://github.com/cosgunhalil/Tickwise
//! [gdext]: https://github.com/godot-rust/gdext
//! [hash coverage checklist]: https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md

#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

mod probe;
mod recorder;
mod walk;

pub use recorder::TickwiseRecorder;

use godot::prelude::*;

/// The GDExtension entry point. Godot loads the library and registers
/// every class in it, which for this crate is [`TickwiseRecorder`].
struct TickwiseExtension;

// SAFETY: the only requirement is that this is implemented once per
// dynamic library, which the crate type cdylib and this single
// declaration guarantee.
#[gdextension]
unsafe impl ExtensionLibrary for TickwiseExtension {}
