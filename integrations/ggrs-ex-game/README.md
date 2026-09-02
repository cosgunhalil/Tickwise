# Tickwise and GGRS ex_game

The first foreign-code validation of Tickwise: the simulation from the GGRS `ex_game` example, driven by a real GGRS `SyncTestSession`, recorded and analyzed by Tickwise.

## What it demonstrates

1. **Recording under rollback.** GGRS rolls back and re-simulates frames on every step. The harness records a tick only the first time a frame is reached, and verifies every re-simulation against the recorded hash. Two sessions with different rollback depths produce byte-identical recordings.
2. **One hash, two consumers.** The checksum GGRS verifies internally is the Tickwise full hash from `SerdeProbe`. No second hashing scheme, no hand-written probe: the game state is a plain serde struct.
3. **The two-pass workflow on foreign code.** A deterministic defect injected at frame 150 is located by `compare` at exactly tick 150 and named by `diff` as a single exact difference at `positions[0][0]`.

## Running

```
cargo test --manifest-path integrations/ggrs-ex-game/Cargo.toml
```

This crate is its own workspace so GGRS and its dependencies never touch the core `tickwise` crate.

## Attribution

The game logic is ported from `examples/ex_game/ex_game.rs` in the [GGRS repository](https://github.com/gschup/ggrs), copyright the GGRS authors, dual licensed under MIT OR Apache-2.0. The macroquad rendering was left out so the integration runs headless.
