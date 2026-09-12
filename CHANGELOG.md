# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Until 1.0, minor versions may break both the API and the recording format.

## [Unreleased]

### Changed
- A `SerdeProbe` recorded under `RecorderConfig::default()` now fails the first tick with `HashAlgoMismatch`, because the default header says caller-defined hashing while the probe uses xxh3. Add `.with_hash_algo(HashAlgo::Xxh3)` to the config; every example and the tutorial do.
- `tickwise inspect` exits with 2 when it cannot read the file, matching `compare` and `diff`. Exit 1 remains the verdict for a corrupt recording. The usage text documents the exit codes of all three commands.

### Added
- Dumps at recording time. `RecorderConfig::dump_interval` schedules a full state dump every N ticks during Pass 1, stored in the `.rec` file; `Recorder::record_dump` takes one on demand. `compare` reports the dumps each recording carries and points at the first shared dump after the divergence, and `tickwise diff` accepts `.rec` files that carry dumps, with `--at <tick>` to pick one. Field level for a desync that does not reproduce, with no replay. The header gains a `dump_interval` field, appended so recordings made before it read back as having none. The `record_demo` example takes `--dump-every`.
- `Recorder::wants_full_hash(tick)` reports whether the next `record_tick` will request a full hash at that tick, the twin of `wants_snapshot`. Callers that compute hashes themselves, such as the engine bridges over the C ABI, use it to skip the expensive full hash on every other tick. Probe-based Rust callers are unaffected.
- `DeterminismProbe::hash_algo_id`, a defaulted method returning zero, meaning no claim. `SerdeProbe` reports its algorithm through it, and `Recorder::record_tick` fails with the new `RecordError::HashAlgoMismatch` when a probe's nonzero id disagrees with `RecorderConfig::hash_algo_id`, instead of writing a recording whose header names the wrong hash. `RecorderConfig::with_hash_algo_id` sets the field, and `with_hash_algo(HashAlgo)` does the same behind the `serde` feature. Hand-written probes are unaffected.
- `tickwise inspect` renders `created at` as a UTC date with the unix value beside it, and `record_demo` stamps the real clock instead of a constant.
- A second fuzz target, `compare_pairs`, mutates pairs of recordings and runs first-divergence search and the structural diff over them; CI runs it beside `rec_reader`.
- Push-model primitives for callers without a probe, the shape the C ABI needs: `Recorder::record_tick_hashes` and `Recorder::record_state_dump` take hashes and dumps computed elsewhere, and `Replayer::wants_dump`, `Replayer::wants_full_hash`, and `Replayer::after_tick_hashes` do the same on the replay side. `ReplayError::MissingDump` reports a dump owed at a tick and not given, leaving the step pending so the call can be repeated with one. `Replayer::check_protocol` is public, so a caller can run the finish-time check without consuming the replayer. The probe forms delegate to these and behave as before.

## [0.2.2] - 2026-09-05

### Added
- `documentation` metadata on both published crates, pointing at their docs.rs pages.

### Changed
- The `uninit-read` chaos mode in the reference simulation is renamed `stale-value`. Safe Rust cannot read uninitialized memory; the mode simulates a stale scratch value that was never reset, and the name now says so. The old name is still accepted on the command line.

## [0.2.1] - 2026-09-04

### Changed
- Relicensed from MIT to MIT OR Apache-2.0, at your option. Apache-2.0 adds an explicit patent grant.

### Fixed
- The declared minimum supported Rust version was 1.85 while the code uses let chains, which need 1.88. It is now 1.88, and CI builds on exactly that toolchain.
- README links were relative and therefore dead on crates.io and docs.rs. They are absolute now, and the README gained status badges.
- Published packages carried no license text. Both license files now ship inside each crate.

### Added
- The "Find your first desync in 15 minutes" tutorial, the hash coverage checklist, and the light hash budget guide under `docs/`.
- `callback_probe` and `serde_probe` examples running the full two-pass workflow in memory.
- Criterion benchmarks for the recorder, compare, and the reference simulation tick budget.
- `replay_demo` example in the reference simulation, completing the command-line two-pass workflow.
- Integrations under `integrations/`: a GGRS `ex_game` synctest harness and a schema-walking probe for Bones ECS worlds.
- Documentation builds with warnings denied in CI.

## [0.2.0] - 2026-09-02

The full two-pass workflow: record, compare, replay, diff.

### Added

- `compare::first_divergence` and the `tickwise compare` command. Finds the first divergent tick between two recordings, confirms it with the full hash, and reports a blind spot when the full hash catches what the light hash missed.
- `Replayer`. Feeds recorded inputs back into your own loop, verifies live hashes against the recording, captures state dumps at requested ticks, locates snapshots with `nearest_snapshot_before` and `seek_to`, and refuses recordings whose input format id does not match.
- `StateDump`, a flat sorted map of field paths to typed values with explicit collection lengths, plus the `STATE_DUMP` chunk kind. `.dump` files share the `.rec` container, so they get the header, seek index, checksum, and fuzz coverage.
- `diff::structural` and the `tickwise diff` command. Field-level differences classified as structural, exact, or sub-epsilon float drift, with `--strict`, `--epsilon-f32`, `--epsilon-f64`, `--all`, and colored output that honors `NO_COLOR`.
- The `serde` feature: `SerdeProbe` as an automatic probe for any `Serialize` state, `to_dump` for structural dumps of any `Serialize` value, typed inputs through `Recorder::record_tick_typed` and `Step::inputs_typed`, `format_id` for input format ids, xxh3 hashing by default, and blake3 behind the `blake3` feature.
- Chaos flags in the reference simulation: float-drift, hashmap-iter, uninit-read, and time-dependent, each caught at its strike tick in CI.
- A GGRS `ex_game` integration under `integrations/`, recording a real rollback session with re-simulation verification.

### Changed

- `tickwise compare` and `tickwise diff` use diff-style exit codes: 0 identical, 1 differences found, 2 trouble.
- `tickwise inspect` reports state dump chunks.
- Input frames are repeat suppressed: a frame is written only when the bytes change and applies until the next frame.

## [0.1.0] - 2026-09-01

### Added

- The `DeterminismProbe` trait, the callback core.
- `Recorder` with per-tick light hashes batched 64 to a chunk, periodic full hashes, snapshots, and markers.
- The versioned `.rec` container: `TKWS` magic, header with session metadata and config echo, chunk stream with skippable unknown kinds, seek index, and integrity checksum. Malformed input never panics.
- The `tickwise inspect` command.
- A cargo-fuzz target and a deterministic mutation sweep for the reader.

[Unreleased]: https://github.com/cosgunhalil/Tickwise/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/cosgunhalil/Tickwise/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/cosgunhalil/Tickwise/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/cosgunhalil/Tickwise/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cosgunhalil/Tickwise/releases/tag/v0.1.0
