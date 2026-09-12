# tickwise-godot

Record, replay, and diff deterministic Godot simulations.

[Tickwise](https://github.com/cosgunhalil/Tickwise) finds the first tick where two runs of the same simulation stopped agreeing. This crate is its Godot 4 bridge, a GDExtension built with [gdext](https://github.com/godot-rust/gdext) that talks to the Rust core directly rather than through the C ABI.

## Status

Under construction, milestone M8 of the Tickwise v2 roadmap. This first version covers Pass 1 of the workflow, recording and compare. Replay and the field-level diff follow.

## The shape of it

Put your simulation nodes in the `tickwise` group, add a `TickwiseRecorder` node, and start it:

```gdscript
extends Node

@onready var tickwise: TickwiseRecorder = $TickwiseRecorder

func _ready() -> void:
    tickwise.game_id = "my-game"
    tickwise.rng_seed = match_seed
    tickwise.start_recording("user://clean.rec")

func _physics_process(_delta: float) -> void:
    tickwise.set_inputs(PackedByteArray([input_bits]))
```

The recorder records one tick per physics frame, at the end of the frame, so it sees the state your simulation just produced. Run the game twice, or once on each of two machines, then compare the recordings offline:

```
cargo install tickwise-cli
tickwise compare clean.rec chaotic.rec
```

```
  verdict        first divergence at tick 421, caught by the light hash,
                 confirmed by the full hash at tick 450, last agreement at tick 420
```

## What gets covered

Only the script variables of nodes in the `tickwise` group. A Godot node carries transforms, visibility, and editor state that have no place in a determinism hash, so engine properties stay out and a `var` declared in your script goes in. Nodes also in the `tickwise_light` group enter the light hash, which runs every tick, so keep that group small.

Anything outside those groups is a blind spot where a desync can hide. The [hash coverage checklist](https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md) walks through the choice.

One consequence is worth stating plainly: a debug switch declared as a `var` on a grouped node is state like any other, and two runs that set it differently diverge from tick 0. That is the tool being right. Keep switches on nodes outside the groups.

Values are read into a flat list of path and value pairs, keyed by absolute node path:

```
/root/Game/World.score              a plain integer
/root/Game/Ball0.last_bounce.x      a Vector2 decomposes into components
/root/Game/Board.cells              a dictionary's length
/root/Game/Board.cells[a1]          one of its entries
```

## The node API

| Member | Purpose |
|---|---|
| `start_recording(path)` | Opens a recording. A `res://` or `user://` path resolves the way Godot resolves it. Returns false on failure |
| `stop_recording()` | Flushes and closes. Leaving the tree does this too |
| `set_inputs(bytes)` | The input bytes for this tick, never interpreted by Tickwise |
| `record_marker(label)` | A named point in the recording, for example a round start |
| `is_recording()`, `get_tick()`, `get_last_error()` | Session status |
| `get_covered_node_count()` | How many nodes are in scope. Zero means nothing is hashed |
| `state_hash()` | The full hash right now, without recording. Exchange it over the network for a live cross-client check |
| `state_snapshot()` | The covered state as a dictionary of path and value, for logging or an in-game inspector |
| `game_id`, `build_hash`, `tick_rate`, `rng_seed`, `full_hash_interval`, `input_format_id`, `group`, `light_group` | Exported properties, settable in the inspector |

## Determinism rules the walk follows

- **Nodes are sorted by absolute path**, because group membership order follows the order nodes entered the tree, which is an engine detail.
- **Dictionary keys are sorted** by their rendered form, so insertion order never reaches a hash.
- **Collections emit their length**, so a shorter array never hides behind a matching tail.
- **Object references are recorded as their class name**, never as an address, since an address differs on every machine. A node holding simulation state belongs in the group in its own right.
- **A value Tickwise cannot decompose is recorded as its type name** rather than dropped, so the gap is visible in the dump instead of hiding a desync.

Hashes are FNV-1a over paths and value bits, recorded under `hash_algo_id` 0, meaning caller defined. Recordings from this crate compare with each other, not with recordings whose hashes came from somewhere else.

## Building

```
cargo build --manifest-path bridges/tickwise-godot/Cargo.toml --release
```

The library lands in `target/release/`. Point a `.gdextension` file in your project at it, the way `godot/tickwise.gdextension` in this directory does. gdext 0.5 targets Godot 4.6; the extension declares `compatibility_minimum = 4.2`.

## Testing

`godot/` is a headless test project with no scenes: four balls and a world node, built from code in `main.gd`, stepped by integer math. `tests/headless.rs` builds the extension, copies it into the project, runs Godot twice, once with a bug planted at tick 421, and checks that `compare` finds exactly that tick. It also runs the clean session twice and requires the two recordings to agree everywhere, which is the determinism claim itself.

```
cargo test --manifest-path bridges/tickwise-godot/Cargo.toml
```

Godot is found through `GODOT_BIN` or as `godot` on the path. Without it the test reports that it was skipped and passes, so a contributor with no engine installed can still run the suite. Continuous integration sets `TICKWISE_REQUIRE_GODOT=1`, which turns a missing engine into a failure.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise.
