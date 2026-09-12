# Changelog

All notable changes to the Tickwise C++ layer are documented in this file. It is versioned with the `tickwise-ffi` crate it wraps.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `tickwise/tickwise.hpp`, a header-only C++14 wrapper over the C ABI: `Recorder` as a move-only RAII session, the `Probe` interface, `Hasher` for fixed-layout xxh3 hashing, `Status` codes, and an ABI check at open. No exceptions, no engine types.
- A CMake project that finds the built `tickwise_ffi` library, and a harness that records a clean and a sabotaged session and drives every misuse path, compiled with warnings as errors on MSVC, GCC, and Clang.
