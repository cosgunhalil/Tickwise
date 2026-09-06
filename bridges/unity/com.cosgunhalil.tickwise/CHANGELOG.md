# Changelog

All notable changes to the Tickwise Unity package are documented in this file. The package is versioned separately from the Rust crates and from `tickwise-ffi`; each release states the native library version it ships.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Package skeleton: `com.cosgunhalil.tickwise`, Unity 2022.3 minimum, the `Tickwise.Runtime` assembly with no engine references, license and third party notices, documentation stub.
- The C# wrapper over `tickwise_ffi`: `TickwiseRecorder` with `Create`, `RecordTick`, `WantsFullHash`, `WantsSnapshot`, `RecordSnapshot`, `RecordMarker`, `Finish`, and a `Dispose` that finishes an open recording; `IDeterminismProbe`; `RecorderConfig`; `TickwiseStatus` and `TickwiseException`; `Xxh3.Hash64` and the `HashAlgo` identifiers; `TickwiseNative` with the ABI compatibility check. Safe code only, C# 9, .NET Standard 2.1.
- The Deterministic Mini Game sample: eight balls in a box with integer math, a `MiniGameSim` probe with no engine code, a `TickwiseSampleRunner` recording from `FixedUpdate`, and an Inject Chaos toggle that leaks the wall clock into the simulation from tick 421.
- `bridges/unity/Tickwise.Tests`, a plain .NET 8 xunit project compiling the Runtime sources, recording through the native library, and verifying the recordings with the `tickwise` command line tool. Runs in CI on Windows, macOS, and Linux.
