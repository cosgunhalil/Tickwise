# Tickwise for Unity

Record and compare deterministic simulations to find desyncs, from inside a Unity project.

Tickwise is an observer. Your game keeps its own loop; once per tick you hand Tickwise the input bytes and a hash of your gameplay state, and it writes a compact `.rec` file. Two recordings of the same match, one per machine, go through the `tickwise compare` command line tool, which names the first tick where the simulations disagreed. The Rust core, the recording format, and the tooling live in the [Tickwise repository](https://github.com/cosgunhalil/Tickwise); this package is the C# layer over the same native library.

## Status

Under construction, milestone M6 of the Tickwise v2 roadmap. This first version covers Pass 1 of the workflow, recording and compare. Replay and field-level diff over the C ABI follow in a later version.

## Requirements

- Unity 2022.3 or newer. Validated by hand on Unity 6000.3.
- The `tickwise` command line tool for comparing recordings: `cargo install tickwise-cli`, or a release binary from the repository.
- The native library `tickwise_ffi` for your target platform. Released package versions ship it under `Runtime/Plugins`; a source checkout builds it with `bridges/tickwise-ffi/scripts/build-for-unity.ps1`.

## Installation

Install by git URL from the Package Manager window, or add the line to `Packages/manifest.json`:

```
"com.cosgunhalil.tickwise": "https://github.com/cosgunhalil/Tickwise.git?path=bridges/unity/com.cosgunhalil.tickwise#unity/v0.1.0"
```

Released package versions live on the `upm` branch and carry tags of the form `unity/vX.Y.Z`. The `main` branch holds source only, without the native binaries.

## Layout

- `Runtime/`: the `Tickwise.Runtime` assembly. It has no reference to `UnityEngine`, so the same sources compile in a plain .NET test project and run in continuous integration where no editor exists.
- `Runtime/Plugins/`: the native library per platform. Empty on `main`.
- `Documentation~/`: the manual.
- `Samples~/`: a deterministic mini game with a chaos toggle, importable from the Package Manager window.

## Testing

The wrapper is tested without an editor. `bridges/unity/Tickwise.Tests` is a plain .NET 8 xunit project that compiles the `Runtime/` sources directly, loads the native library from the `tickwise-ffi` release build, records sessions, and verifies them with the `tickwise` command line tool built from this repository. It runs in continuous integration on Windows, macOS, and Linux.

```
cd bridges/tickwise-ffi && cargo build --release && cd ../..
dotnet test bridges/unity/Tickwise.Tests
```

The sample scene and the platform plugin settings are validated by hand in the Unity editor.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise. See `LICENSE.md` and `Third Party Notices.md`.
