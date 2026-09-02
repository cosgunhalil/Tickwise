# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Until 1.0, minor versions may break both the API and the recording format.

## [Unreleased]

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

[Unreleased]: https://github.com/cosgunhalil/Tickwise/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/cosgunhalil/Tickwise/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/cosgunhalil/Tickwise/releases/tag/v0.1.0
