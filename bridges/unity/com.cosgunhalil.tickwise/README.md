# Tickwise for Unity

Record and compare deterministic simulations to find desyncs, from inside a Unity project.

Tickwise is an observer. Your game keeps its own loop; once per tick you hand Tickwise the input bytes and a hash of your gameplay state, and it writes a compact `.rec` file. Two recordings of the same match, one per machine, go through the `tickwise compare` command line tool, which names the first tick where the simulations disagreed. The Rust core, the recording format, and the tooling live in the [Tickwise repository](https://github.com/cosgunhalil/Tickwise); this package is the C# layer over the same native library.

New here? [Find your first desync in Unity in 15 minutes](https://github.com/cosgunhalil/Tickwise/blob/main/docs/unity-tutorial.md) installs the package, runs the sample twice, and catches a planted bug with the command line tool.

## Status

Under construction, milestone M6 of the Tickwise v2 roadmap. Both passes of the workflow are in the package: recording and compare, state dumps on an interval for a replay-free `tickwise diff`, and `TickwiseReplayer` for the replay-based diff. The manual under `Documentation~` walks them.

## Requirements

- Unity 2022.3 or newer. Validated by hand on Unity 6000.3.
- The `tickwise` command line tool for comparing recordings: `cargo install tickwise-cli`, or a release binary from the repository.
- The native library `tickwise_ffi` for your target platform. Released package versions ship it under `Runtime/Plugins`; a source checkout builds it with `bridges/tickwise-ffi/scripts/build-for-unity.ps1`.

## Installation

Install by git URL from the Package Manager window, or add the line to `Packages/manifest.json`:

```
"com.cosgunhalil.tickwise": "https://github.com/cosgunhalil/Tickwise.git#unity/v0.1.0"
```

Released package versions live at the root of the `upm` branch, with the native binaries included, and carry tags of the form `unity/vX.Y.Z`. The `main` branch holds the package source under `bridges/unity/` without binaries; installing from `main` needs `?path=bridges/unity/com.cosgunhalil.tickwise` and a locally built native library.

## Layout

- `Runtime/`: the `Tickwise.Runtime` assembly. It has no reference to `UnityEngine`, so the same sources compile in a plain .NET test project and run in continuous integration where no editor exists.
- `Runtime/Plugins/`: the native library per platform, one folder each: `Windows/x86_64`, `macOS` as a universal binary, `Linux/x86_64`, `Android/arm64-v8a`, `Android/armeabi-v7a`, `Android/x86_64`, and `iOS` as a static library. The `.meta` beside each binary carries the platform and CPU settings. Both the binaries and their metas are absent on `main` and present on the `upm` branch; the release workflow generates the metas with `bridges/unity/scripts/new-plugin-meta.ps1`.
- `Documentation~/`: the manual.
- `Samples~/`: a deterministic mini game with a chaos toggle, importable from the Package Manager window.

## Platforms

| Platform | Binary | Notes |
|---|---|---|
| Windows x86_64, editor and player | `tickwise_ffi.dll` | Static C runtime, no redistributable needed |
| macOS, editor and player | `libtickwise_ffi.dylib` | Universal, Intel and Apple silicon |
| Linux x86_64, editor and player | `libtickwise_ffi.so` | |
| Android arm64-v8a, armeabi-v7a, x86_64 | `libtickwise_ffi.so` per ABI | Mono and IL2CPP |
| iOS device | `libtickwise_ffi.a` | Static, linked into the app; the simulator is not covered yet |

The C# side is identical on every platform except iOS, where the import name is `__Internal` because the library is linked statically.

## Releasing

A release is one manual run of the Unity release workflow in the repository's Actions tab, with the version as its input. The workflow builds the native library for Windows, Linux, Android, macOS, and iOS, runs `bridges/unity/scripts/assemble-upm.sh` to combine the package sources with the binaries and their generated plugin metas, commits the result to the `upm` branch, tags it `unity/v<version>`, and attaches the raw libraries with the C header to a GitHub Release for non-Unity hosts. `package.json` must already carry the version, or the workflow refuses.

## Testing

The wrapper is tested without an editor. `bridges/unity/Tickwise.Tests` is a plain .NET 8 xunit project that compiles the `Runtime/` sources directly, loads the native library from the `tickwise-ffi` release build, records sessions with dumps, replays them, and verifies the files with the `tickwise` command line tool built from this repository: `inspect`, `compare`, and `diff`. It runs in continuous integration on Windows, macOS, and Linux.

```
cd bridges/tickwise-ffi && cargo build --release && cd ../..
dotnet test bridges/unity/Tickwise.Tests
```

The sample scene and the platform plugin settings are validated by hand in the Unity editor.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise. See `LICENSE.md` and `Third Party Notices.md`.
