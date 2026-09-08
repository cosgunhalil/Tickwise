# Tickwise for Unity

Tickwise records what a deterministic simulation did, one tick at a time, so two machines that were supposed to agree can be compared afterwards and the first tick where they disagreed can be named.

This manual is written alongside the package and grows with it. The sections below describe the shape of the first version; the concrete API reference lands with the `Tickwise.Runtime` classes.

## What a recording holds

Once per tick your code hands Tickwise three things: the input bytes for that tick, a cheap hash of desync-critical state called the light hash, and, every N ticks, a full hash of all gameplay state. Tickwise writes them into a `.rec` file together with session metadata such as the game identifier, build hash, platform, tick rate, and seed.

Tickwise never runs your simulation and never calls back into your code. You compute the hashes; the package tells you when the expensive full hash is due.

## The two-pass workflow

1. **Record on both machines.** Each client records its own `.rec` file during the match.
2. **Compare offline.** `tickwise compare a.rec b.rec` prints the first divergent tick, which hash caught it, and the last tick where both sides agreed.

Pass 2, replaying to a tick and diffing the state field by field, is part of the Rust toolkit today and comes to this package in a later version.

## Installing the native library

Released versions of this package carry `tickwise_ffi` for Windows, macOS, Linux, Android, and iOS under `Runtime/Plugins`. When working from a source checkout, build it yourself with `bridges/tickwise-ffi/scripts/build-for-unity.ps1`.

## Getting the command line tool

```
cargo install tickwise-cli
```

Or download a release binary from the Tickwise repository.

## Further reading

- The Unity tutorial, fifteen minutes from install to a caught desync: https://github.com/cosgunhalil/Tickwise/blob/main/docs/unity-tutorial.md
- The Tickwise repository: https://github.com/cosgunhalil/Tickwise
- The Rust tutorial, which walks the same workflow on the reference simulation: https://github.com/cosgunhalil/Tickwise/blob/main/docs/tutorial.md
- The hash coverage checklist, which says what belongs in each hash: https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md
