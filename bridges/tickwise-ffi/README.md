# tickwise-ffi

The C ABI over the Tickwise core. Every non-Rust engine bridge stands on this crate: the Unity package over P/Invoke, the Unreal plugin, and the cocos2d-x wrapper.

## Status

Under construction, milestone M5 of the v2 roadmap. The first version covers Pass 1 only: recording with caller-provided hashes, so that `tickwise compare` works on sessions recorded from any engine. The replayer and a dump builder follow in a second iteration.

## Shape

Tickwise in Rust pulls hashes from a probe trait. Across a C boundary there is no trait object, and callbacks into managed code are the classic pain point of engine plugins, so the C surface flips to a push model: the caller computes its hashes and passes them in as integers. `tickwise_recorder_wants_full_hash` tells the caller when the expensive full hash is due, so it is computed only on the ticks the recorder will keep.

The crate is built three ways from one source:

- `cdylib`, the shared library that Unity loads through `DllImport` and that C or C++ hosts load dynamically
- `staticlib`, for iOS and for linking straight into an engine plugin
- `rlib`, so the crate's own Rust tests call the exported functions directly

## Building

This crate is its own Cargo workspace. From the repository root:

```
cargo build --manifest-path bridges/tickwise-ffi/Cargo.toml --release
cargo test --manifest-path bridges/tickwise-ffi/Cargo.toml
```

The shared library lands in `bridges/tickwise-ffi/target/release/` as `tickwise_ffi.dll`, `libtickwise_ffi.so`, or `libtickwise_ffi.dylib` depending on the platform.

## Rules

This is the only crate in the repository allowed to use `unsafe`. The rules are enforced by the compiler and clippy, not by review alone:

- `unsafe_op_in_unsafe_fn` is denied, so every unsafe operation sits in its own block
- clippy's `undocumented_unsafe_blocks` is denied, so every block carries a `// SAFETY:` comment or the build fails
- no function panics across the boundary or unwinds; panics are caught and returned as error codes
- misuse such as null handles, wrong lengths, double finish, or out-of-order ticks returns an error code

The full policy is in [CODING_STANDARDS.md](https://github.com/cosgunhalil/Tickwise/blob/main/CODING_STANDARDS.md).

## ABI versioning

`tickwise_ffi_abi_version()` returns the version of the C surface. It increments on every incompatible change, and bridges refuse to load a library whose value differs from the one they were compiled for. The surface may change freely while this crate is below 1.0. Once a bridge is announced publicly, every surface change ships with a migration note in the CHANGELOG.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise.
