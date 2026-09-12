# Changelog

All notable changes to the Tickwise C++ layer are documented in this file. It is versioned with the `tickwise-ffi` crate it wraps.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `tickwise/tickwise.hpp`, a header-only C++14 wrapper over the C ABI: `Recorder` as a move-only RAII session, the `Probe` interface, `Hasher` for fixed-layout xxh3 hashing, `Status` codes, and an ABI check at open. No exceptions, no engine types.
- Pass 2. `Dump`, a move-only builder over the C dump handle with one setter per value type and `len` for collections; `Probe::state_dump`, a third virtual with an empty default; `Config::dump_interval` with `Recorder::wants_dump` and `Recorder::record_dump`, and the probe form of `record_tick` taking the dump on its own; `Replayer` with `ReplayOptions` and `Step`, `next_step`, `after_tick` in probe and push forms, `nearest_snapshot_before`, `seek_to`, and `finish` to a `.dump`. The six new `Status` values. `kExpectedAbiVersion` follows the header to 2. The harness runs both passes and CI diffs its files.
- A CMake project that finds the built `tickwise_ffi` library, and a harness that records a clean and a sabotaged session and drives every misuse path, compiled with warnings as errors on MSVC, GCC, and Clang.
