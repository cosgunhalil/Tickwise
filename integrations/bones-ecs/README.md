# Tickwise and Bones ECS

A Tickwise probe for any world built on [Bones ECS](https://github.com/fishfolk/bones), the framework behind Fish Folk: Jumpy. Bones keeps a runtime schema for every component and resource, and this crate walks those schemas to hash and dump a `World` with no probe code in the game.

## Why Bones rather than Jumpy directly

The design doc planned a Jumpy integration. Research showed that Jumpy's rollback layer is entirely inside Bones: a `GgrsSessionRunner` snapshots by cloning the ECS world and passes no checksum to GGRS, so Bones games get desync detection only from remote peers and no field-level tooling. Integrating at the Bones layer therefore covers Jumpy and every other Bones game at once, and Bones ECS has no renderer dependency, so the tests run headless in seconds on every push.

## What it demonstrates

1. **Schema-derived dumps.** Paths like `Position[3].x` and `Score.value` come straight from the Bones schemas, with explicit `Len` entries for collections and the alive entity count.
2. **Snapshot completeness checking.** The Bones community's stated pain point is knowing whether a cloned world is a complete snapshot. The test runs a world ahead, restores a clone, runs again, and demands identical hashes every tick.
3. **The two-pass workflow on a Bones world.** A gameplay defect that skips one entity's update is located by `compare` at its first tick and named by `diff` under `Velocity[3]` and `Position[3]`.

## Using it in a Bones game

```rust
let probe = BonesProbe::new(&world)
    .component::<Position>()
    .component::<Velocity>()
    .resource::<Score>()
    .light_resource::<Tick>();
recorder.record_tick(tick, &input_bytes, &probe)?;
```

Coverage is explicit because Bones ECS exposes no iteration over all stores. Register every component and resource that influences a future tick, and keep the light set to a few small resources. The Tickwise hash coverage checklist applies unchanged.

For a networked Bones game, the natural hook is the `AdvanceFrame` handling in `GgrsSessionRunner`, recording a tick the first time a frame is reached and verifying the recorded hash on every rollback re-simulation, exactly as the GGRS `ex_game` integration in this repository does.

## Version

Pinned to Bones ECS 0.4.0 from crates.io. Jumpy tracks the Bones git repository, whose ECS `World` API matches this release as of September 2026.
