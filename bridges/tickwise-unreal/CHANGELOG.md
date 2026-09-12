# Changelog

All notable changes to the Tickwise Unreal plugin are documented in this file. The plugin is versioned separately from the Rust crates; each release states the native library version it ships.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- The `Tickwise` runtime module, built against Unreal Engine 4.26 over the `tickwise-ffi` C ABI through the shared C++ layer.
- `UTickwiseRecorderComponent`, recording one tick per `RecordTick` call from your own fixed step, with the session metadata as editable properties and an opt-in per-component-tick mode.
- `ITickwiseProbe`, a Blueprint-implementable interface for the light and full hashes.
- `FTickwiseHasher` for C++ and `UTickwiseHashLibrary` for Blueprint, both producing xxh3 over a fixed byte layout.
- `scripts/stage-ffi.ps1`, which builds the native library and stages headers and Windows binaries into the plugin's `ThirdParty` folder.
- State dumps. `DumpInterval` on the recorder component records a state dump every N ticks, `RecordDump` takes one on demand at the last recorded tick, and `ITickwiseStateWriter`, a C++ only interface implemented beside the probe, writes the state into `FTickwiseDump` by field name with overloads for the common engine types. With dumps in both recordings, `tickwise diff a.rec b.rec` names the fields that differ with no replay. Pass 2 replay in Unreal goes through `tickwise::Replayer` from the shared C++ header, which `TickwiseNative.h` already includes.
