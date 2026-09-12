# Tickwise

[![crates.io](https://img.shields.io/crates/v/tickwise.svg)](https://crates.io/crates/tickwise)
[![docs.rs](https://docs.rs/tickwise/badge.svg)](https://docs.rs/tickwise)
[![CI](https://github.com/cosgunhalil/Tickwise/actions/workflows/ci.yml/badge.svg)](https://github.com/cosgunhalil/Tickwise/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/cosgunhalil/Tickwise#license)

**Record, replay, and diff deterministic simulations.**

Tickwise is an engine-agnostic recording, replay, and desync-debugging toolkit for deterministic multiplayer games, written in Rust. Determinism is a promise that must be verified every single tick, and Tickwise exists to make that vigilance cheap.

> ⚠️ **Status: early development.** Version 0.2.2 is on crates.io as [tickwise](https://crates.io/crates/tickwise) and [tickwise-cli](https://crates.io/crates/tickwise-cli) and covers the full two-pass workflow: record, compare, replay, diff. The API and the recording format may change freely until 1.0.

## Try it

```
cargo add tickwise --features serde    # the library, with the serde convenience layer
cargo install tickwise-cli             # the tickwise binary
```

With the `serde` feature, any `Serialize` state becomes a probe in a few lines:

```rust
use serde::Serialize;
use tickwise::serde_probe::SerdeProbe;
use tickwise::{Recorder, RecorderConfig};

#[derive(Serialize)]
struct Game { tick: u64, score: u64, positions: Vec<(f32, f32)> }

let mut game = Game { tick: 0, score: 0, positions: vec![(0.0, 0.0)] };
let mut rec = Recorder::create("session.rec", RecorderConfig::default())?;
for tick in 0..600 {
    let input = (1u8, 0u8);         // your own input type
    game.tick += 1;                 // your own simulation step
    rec.record_tick_typed(tick, &input, &SerdeProbe::new(&game))?;
}
rec.finish()?;
```

Performance-sensitive code implements the three-method `DeterminismProbe` trait by hand instead. Both paths are shown end to end, record, compare, replay, and diff in one file each, in [crates/tickwise/examples](https://github.com/cosgunhalil/Tickwise/tree/main/crates/tickwise/examples). Then the CLI takes over:

```
tickwise inspect session.rec        # what is in a recording
tickwise compare a.rec b.rec        # first divergent tick between two sessions
tickwise diff a.dump b.dump         # field-level differences at that tick
```

New here? [Find your first desync in 15 minutes](https://github.com/cosgunhalil/Tickwise/blob/main/docs/tutorial.md) walks the whole workflow on the reference simulation, including a real bug caught and named. Unity developer? [The Unity tutorial](https://github.com/cosgunhalil/Tickwise/blob/main/docs/unity-tutorial.md) does the same inside the editor with the Tickwise package. Wiring up your own game? The [hash coverage checklist](https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md) says what belongs in each hash and why, and the [light hash budget guide](https://github.com/cosgunhalil/Tickwise/blob/main/docs/light-hash-budget.md) shows the measured per-tick cost, 20 nanoseconds for the recorder itself.

## The problem

Deterministic simulation is the foundation of lockstep and rollback netcode. Every client runs the same simulation from the same inputs and must arrive at the same state. When that promise breaks, even by a single divergent bit, two players fork into different realities. This failure mode is called a desync, and it is uniquely expensive to debug for three reasons.

1. **The symptom appears far from the cause.** A divergence at tick 4,021 typically surfaces to a human minutes later, as impossible gameplay or a checksum kick.
2. **Reproduction dominates the cost.** Without recorded inputs and hashes, reproducing a desync locally is guesswork, and reproduction is most of the work.
3. **The tooling is always bespoke.** Studios that ship deterministic multiplayer keep rebuilding the same three components privately: input and hash recording, first-divergence search, and structural state diff.

No open-source, engine-agnostic equivalent of that in-house tooling exists. Tickwise fills the gap. It records sessions cheaply, finds the first divergent tick in seconds, and reports the diverging subsystem and field.

## How it works

Tickwise is an observer. It never runs your simulation. You drive your own game loop and call into the kit, which keeps it invasion-free and engine-agnostic. The analysis happens in two passes.

```
┌─ PASS 1 (always on, cheap) ───────────────────────────────┐
│ Client A plays → a.rec   (inputs + per-tick hashes)       │
│ Client B plays → b.rec                                    │
│                                                           │
│ $ tickwise compare a.rec b.rec                            │
│ → "First divergence: tick 4021 (light-hash mismatch,      │
│    confirmed by full hash at tick 4200)"                  │
└───────────────────────────────────────────────────────────┘
┌─ PASS 2 (targeted, on demand) ────────────────────────────┐
│ Replay a.rec in your own loop with                        │
│ dump_at_tick = 4021 → a.dump                              │
│ Same for b.rec → b.dump                                   │
│                                                           │
│ $ tickwise diff a.dump b.dump                             │
│ → "tick 4021: players[2].velocity.x                       │
│    A: 3.5  B: 3.5000001  (sub-epsilon float drift)"       │
│ → "tick 4021: projectiles.len  A: 14  B: 15 (structural)" │
└───────────────────────────────────────────────────────────┘
```

There is an even simpler entry point: the **self-check**. Play a session once, replay its recorded inputs through your simulation, record that too, and compare:

```
tickwise compare original.rec replayed.rec
```

If the verdict is anything but identical, your simulation is not deterministic, and you just found out before your players did.

## CLI

Three commands in v1, no more:

```
tickwise compare a.rec b.rec   # first divergent tick + hash kind + summary
tickwise diff a.dump b.dump    # structural diff, float-classified, colored output
tickwise inspect session.rec   # metadata + statistics
```

The diff classifies rather than judges. Differences are reported as `Structural`, `Exact`, or `SubEpsilonFloat`, so both float-based and fixed-point simulations are first-class citizens.

## How Tickwise compares

| | Photon Quantum | GGRS SyncTest | rr debugger | In-house tools | **Tickwise** |
|---|---|---|---|---|---|
| Open source | ✗ commercial | ✓ | ✓ | ✗ | ✓ |
| Engine-agnostic | ✗ Quantum only | ✗ GGRS sessions only | n/a | ✗ project-specific | ✓ |
| Recording format + offline compare | ✓ replay files | ✗ | ✓ syscall level | partial | ✓ |
| Field-level state diff | partial | ✗ checksum only | ✗ | ✓ bespoke | ✓ |
| Simulation-level semantics: ticks, game state | ✓ | ✓ | ✗ | ✓ | ✓ |

rr records execution at the syscall level. Tickwise records simulation at the tick level, which is the layer where "tick 4021, `players[2].velocity.x` diverged" is even expressible. GGRS users are especially welcome: Tickwise complements SyncTest with a persistent recording format, offline comparison, and structural diffs.

## Roadmap

| Milestone | Content | Definition of done | Status |
|---|---|---|---|
| **M0** | Workspace skeleton, probe trait, reference simulation | Refsim runs 10k ticks deterministically, CI green | ✓ |
| **M1** | Recorder, `.rec` format, `inspect` | Recording round-trip tests pass, 0.1.0 on crates.io | ✓ |
| **M2** | `compare` for first divergence, chaos flags | All chaos classes caught at the correct tick | ✓ |
| **M3** | Replayer, dumps, `diff`, serde layer, GGRS integration | Two-pass workflow end-to-end, 0.2.0 on crates.io | ✓ |
| **M4** | Launch package: docs, examples, tutorial, benchmarks | A stranger finds their first desync in 15 minutes, unaided | in progress |

## v2 roadmap: engine bridges

v1 is the Rust core and the command line tool. v2 brings the same two-pass workflow to engines, in this order:

| Milestone | Bridge | Shape |
|---|---|---|
| **M5** | `tickwise-ffi` | C ABI over the core, shared and static library, generated header, prebuilt binaries for Windows, macOS, Linux, Android, and iOS |
| **M6** | Unity | C# package over the C ABI, installable by git URL |
| **M7** | Bevy | Native crate: a Reflect-walking probe and a fixed timestep plugin, see [bridges/tickwise-bevy](https://github.com/cosgunhalil/Tickwise/tree/main/bridges/tickwise-bevy) |
| **M8** | Godot | GDExtension built with the gdext crate against the core, see [bridges/tickwise-godot](https://github.com/cosgunhalil/Tickwise/tree/main/bridges/tickwise-godot) |
| **M9** | Unreal | C++ plugin over the C ABI, see [bridges/tickwise-unreal](https://github.com/cosgunhalil/Tickwise/tree/main/bridges/tickwise-unreal), on a shared C++ layer in [bridges/tickwise-cpp](https://github.com/cosgunhalil/Tickwise/tree/main/bridges/tickwise-cpp) |
| **M10** | cocos2d-x | C++ wrapper over the C ABI, see [bridges/tickwise-cocos2dx](https://github.com/cosgunhalil/Tickwise/tree/main/bridges/tickwise-cocos2dx) |

Every bridge shipped Pass 1 first, meaning recording with caller-provided hashes so that `tickwise compare` works on sessions from that engine. The C ABI now carries Pass 2 as well, a dump builder and the replayer, so Unity, Unreal, and cocos2d-x reach `tickwise diff` both from dumps recorded on an interval and from a replay; engine reflection probes follow. Bridges live under `bridges/` in this repository, each as its own workspace or language project. They never touch the crates.io crates, and the core keeps `#![forbid(unsafe_code)]`. Unsafe code exists only inside `tickwise-ffi`.

## Non-goals

Tickwise deliberately does not include, in v1 or later:

- ❌ Network or transport layer, netcode, or a rollback engine. GGRS and friends own that space.
- ❌ Determinism linter or static analysis.
- ❌ A fixed-point math library.
- ❌ GUI or TUI visualizer, live monitoring.
- ❌ Async API or tokio dependency. The core stays synchronous and allocation-conscious.

Two former v1 non-goals, the Unity bridge and engine plugins, moved to the v2 roadmap above. They were never out of scope for the project, only for v1, and the core API was designed for them from the first decision.

## Contributing

Contributions are welcome. Start with [CONTRIBUTING.md](https://github.com/cosgunhalil/Tickwise/blob/main/CONTRIBUTING.md) for the workflow and commit conventions, [CODING_STANDARDS.md](https://github.com/cosgunhalil/Tickwise/blob/main/CODING_STANDARDS.md) for the code rules, and [CODE_OF_CONDUCT.md](https://github.com/cosgunhalil/Tickwise/blob/main/CODE_OF_CONDUCT.md) for community expectations. Security reports go through the process in [SECURITY.md](https://github.com/cosgunhalil/Tickwise/blob/main/SECURITY.md).

## License

Licensed under either of the [MIT License](https://github.com/cosgunhalil/Tickwise/blob/main/LICENSE-MIT) or the [Apache License, Version 2.0](https://github.com/cosgunhalil/Tickwise/blob/main/LICENSE-APACHE), at your option. Apache-2.0 adds an explicit patent grant, which some organizations require.

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in Tickwise is dual licensed as above, without any additional terms or conditions.
