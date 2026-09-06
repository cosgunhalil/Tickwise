# Tickwise Coding Standards

These are the rules for all code in this repository. They exist so that every module feels like it was written by one careful person. When a rule here conflicts with personal taste, the rule wins. When a rule seems wrong for a concrete case, open an issue and challenge the rule instead of silently breaking it.

## Toolchain

- Stable Rust, latest release for development. No nightly features.
- The minimum supported Rust version is declared as `rust-version` in the workspace manifest and enforced by a CI job that builds on exactly that toolchain. Raising it is a Changed entry in the CHANGELOG, never a silent side effect of using a new language feature.
- `cargo fmt` with the default rustfmt configuration. No `rustfmt.toml` overrides.
- `cargo clippy --all-targets -- -D warnings` must pass. Warnings are errors.
- CI enforces both. A PR that fails either does not get reviewed.

## Architectural rules

These come from settled design decisions and are not open for casual relitigation.

1. **Synchronous core.** No async, no tokio, no async-adjacent dependencies. Tickwise runs on the hot path of a game loop.
2. **Dependency discipline.** Every new dependency in the core `tickwise` crate needs discussion in an issue first. The core target is minimal mandatory dependencies, and even `serde` stays behind a feature flag. The CLI crate has more freedom, within reason.
3. **FFI-ready boundaries.** The `DeterminismProbe` trait and everything it touches must remain simple enough to cross an FFI boundary cleanly. No generic-heavy public APIs on the core trait path, no closures in FFI-facing signatures, no types without a stable layout story.
4. **The observer model is sacred.** Tickwise never drives the user's simulation. Any API that wants to own the game loop is wrong by definition.
5. **Feature flags stay additive.** Enabling a feature must never change existing behavior, only add capability.

## Error handling

1. **Library code never panics on input.** A corrupted or malicious `.rec` or `.dump` file must produce an error, never a panic. Fuzzing enforces this and a fuzz-found panic is a bug of the highest priority.
2. `unwrap` and `expect` are forbidden in library code except for cases that are provably unreachable, and each such case carries a comment stating the invariant that makes it safe.
3. Errors are typed. Public functions return concrete error enums, not boxed or stringly errors. Error messages state what failed and what the caller can do about it.
4. The CLI translates errors into human-readable output with next-step hints. Raw debug formatting never reaches the user.

## Performance rules

1. **The `light_hash` budget is 1 percent of a tick.** Code on the per-tick recording path is written and benchmarked against that budget.
2. **Hot paths do not allocate.** `record_tick` and everything under it avoids heap allocation in the steady state. Buffers are reused, not recreated.
3. Performance claims require criterion benchmarks. A PR that says "faster" without a benchmark says nothing.
4. Cold paths, meaning file finalization, comparison, and diffing, favor clarity over micro-optimization.

## Unsafe policy

`unsafe` is permitted in exactly one place: the `tickwise-ffi` crate under `bridges/`. Every other Rust crate in this repository, including the core, the CLI, the reference simulation, the integrations, and the Rust-native bridges for Bevy and Godot, carries `#![forbid(unsafe_code)]`.

Inside `tickwise-ffi`:

1. The crate sets `#![deny(unsafe_op_in_unsafe_fn)]`. Every unsafe operation sits in its own `unsafe` block, even inside an `unsafe fn`.
2. Every `unsafe` block carries a `// SAFETY:` comment stating the invariant that makes it sound and who upholds it, the caller or the crate.
3. Every exported function is `extern "C"`, documents its contract in the header comment, and is safe against null pointers, zero lengths, and misuse such as double finish or out-of-order ticks. Misuse returns an error code. It never panics across the boundary, and it never unwinds.
4. Panics are caught at the boundary and turned into an error code. A panic reaching C is a bug of the highest priority, on par with a fuzz-found panic in the reader.
5. A PR touching `unsafe` code contains nothing unrelated.

## Bridges and non-Rust code

Code under `bridges/` follows the same spirit with the idioms of its own language.

1. Each bridge is its own Cargo workspace or its own language project. Bridges depend on the core, never the reverse, and they never change what the crates.io crates publish.
2. Each bridge ships its own README, CHANGELOG, LICENSE files, and tests, and CI runs every test that can run without an editor. The Unity and Unreal editors cannot run in CI, so those bridges keep their engine-free code testable in plain .NET and plain C++, and their editor-facing layer stays thin.
3. C# follows the .NET naming conventions and the Unity package layout conventions. It uses `DllImport` declarations only, no function pointers across the boundary, so the same source compiles under Mono, IL2CPP, and plain .NET.
4. C++ follows the target engine's style guide, meaning the Unreal coding standard inside the Unreal plugin and the cocos2d-x conventions inside that wrapper.
5. Prebuilt native binaries are never committed to `main`. Release automation builds and publishes them.
6. The observer model, the determinism rules, and the prose rules apply unchanged in every language.

## Determinism rules

Tickwise is a determinism tool, so its own code is held to the standard it preaches.

1. No iteration over `HashMap` or `HashSet` where order can reach an output, a hash, or a file. Use `BTreeMap`, `BTreeSet`, or explicit sorting.
2. No wall-clock time, thread timing, or environment state in any recorded or compared value. Timestamps live only in metadata, never in payloads.
3. The refsim crate uses its own seeded LCG, never `rand`.
4. Serialization must be deterministic: the same logical state always produces identical bytes.

## Testing expectations

1. Every `.rec` and `.dump` format change ships with a round-trip test: write, read back, compare for equality.
2. Every chaos class in `tickwise-refsim` stays covered by an integration test that proves Tickwise catches it at the correct tick. A new chaos flag lands together with its test.
3. Bug fixes ship with a regression test that fails before the fix.
4. Tests are deterministic. A flaky test is treated as a bug, not a nuisance.

## Naming and documentation

1. Names follow the Rust API guidelines. Types are nouns, methods are verbs, no abbreviations that save three characters at the cost of clarity.
2. Every public item has a doc comment. The first line is a single sentence that stands alone, because docs.rs shows it in summaries.
3. Doc examples compile. Prefer `# Examples` sections with runnable code over prose descriptions of usage.
4. Comments explain why, never what. A comment that restates the code gets deleted in review.

## Prose rules

All prose, including doc comments, regular comments, commit messages, and markdown files, follows the project writing rules:

1. No em dashes.
2. No parentheses for descriptions or explanations. Write the aside as its own sentence.

Code itself is exempt, and so are adopted standard documents.

## Commits

Commit messages follow [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) as described in [CONTRIBUTING.md](CONTRIBUTING.md).
