# tickwise-bevy

Record, replay, and diff deterministic Bevy simulations.

[Tickwise](https://github.com/cosgunhalil/Tickwise) finds the first tick where two runs of the same simulation stopped agreeing. This crate is its Bevy bridge: a probe that reads gameplay state through `Reflect`, and a plugin that records one tick at the end of every fixed step.

## Status

Under construction, milestone M7 of the Tickwise v2 roadmap. This first version covers Pass 1 of the workflow, recording and compare. Replay and the field-level diff follow.

## The shape of it

```rust
use bevy::prelude::*;
use tickwise_bevy::{TickwiseAppExt, TickwisePlugin};

#[derive(Component, Reflect)]
struct Position { x: i32, y: i32 }

#[derive(Resource, Reflect, Default)]
struct Rng { state: u32 }

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(TickwisePlugin::new("session.rec"))
    .record_component::<Position>()
    .record_resource_in_light_hash::<Rng>()
    .run();
```

Run the game twice, or once on each of two machines, then compare the recordings offline:

```
cargo install tickwise-cli
tickwise compare a.rec b.rec
```

```
  verdict        first divergence at tick 421, caught by the light hash,
                 confirmed by the full hash at tick 450, last agreement at tick 420
```

## What gets covered

Nothing, until you say so. A Bevy world holds timers, window handles, and asset ids that have no place in a determinism hash, so four methods on `App` declare what counts as gameplay state:

| Method | Covered by |
|---|---|
| `record_component::<C>()` | the full hash and the dump |
| `record_component_in_light_hash::<C>()` | the light hash as well, which runs every tick |
| `record_resource::<R>()` | the full hash and the dump |
| `record_resource_in_light_hash::<R>()` | the light hash as well |

Anything left out is a blind spot where a desync can hide. The [hash coverage checklist](https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md) walks through the choice, and the crate has a test proving that an unregistered type is invisible, because that cost is real and worth stating.

Registered types are read into a flat list of path and value pairs:

```
Position.len          the number of entities carrying the component
Position[3].x         a field on the entity with index 3
Rng.state             a resource field
Inventory.counts      a map's length
Inventory.counts[axe] one of its entries
```

## Determinism rules the walk follows

- **Maps and sets are walked in sorted order.** A hash map's iteration order is not part of your simulation and must never reach a hash.
- **Entities are walked in index order**, not archetype order, so adding an unrelated component to one entity does not change the hash.
- **Collections emit their length**, so a shorter list never hides behind a tail that happens to match.
- **A missing resource is recorded as null** rather than skipped, because present on one machine and absent on the other is exactly the kind of difference worth catching.
- **An unsupported reflection kind is recorded as its type path** rather than dropped, so a gap is visible in the dump instead of silently hiding a desync.

Hashes are FNV-1a over paths and value bits, recorded under `hash_algo_id` 0, meaning caller defined. Recordings from this crate compare with each other, not with recordings whose hashes came from somewhere else.

## Assumptions worth knowing

The comparison assumes both runs allocated the same entity indices. That is true of a simulation that is deterministic in the first place, and false in a way worth knowing about if it is not.

The recording system runs in `FixedLast`, so it sees the world after your own fixed systems have stepped it. Write your input bytes into the `TickwiseInputs` resource from any system that runs earlier in the fixed step.

## Dependencies

`bevy_app`, `bevy_ecs`, and `bevy_reflect` only, with default features off and `std` on. A determinism tool has no business pulling in rendering, audio, or windowing, and it keeps headless tests fast.

## Testing

```
cargo test --manifest-path bridges/tickwise-bevy/Cargo.toml
```

Everything runs headless, no window and no clock: the tests drive the fixed schedule directly with `run_schedule(FixedMain)`, which is also the most honest way to test a fixed timestep.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise.
