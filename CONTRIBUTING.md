# Contributing to Tickwise

Thank you for your interest in Tickwise. This document explains how to contribute effectively. Reading it first saves time for both of us.

## Project status

Tickwise is in early development, before its first release. The API and the `.rec` recording format may change freely until 1.0. This shapes what kinds of contributions help most right now:

- **Very welcome:** bug reports, desync war stories, feedback on the API sketch, documentation fixes, and testing on platforms we do not cover.
- **Welcome with prior discussion:** new features and refactors. Open an issue before writing code, so nobody's work goes to waste.
- **Please hold:** anything on the non-goals list in the [README](README.md). Those decisions are settled for v1.

## Getting started

The project is a standard Cargo workspace. You need stable Rust, installed via [rustup](https://rustup.rs).

```
git clone https://github.com/cosgunhalil/Tickwise.git
cd Tickwise
cargo build
cargo test
```

Useful variations:

```
cargo test -p tickwise <test_name>   # run a single test in the core crate
cargo fmt --all                      # format, required before every commit
cargo clippy --all-targets -- -D warnings   # lint, warnings are errors
cargo bench                          # criterion benchmarks
```

## Code rules

All code follows [CODING_STANDARDS.md](CODING_STANDARDS.md). The short version: no async, no panics on user input, dependency additions need discussion first, and the hot path stays allocation-conscious. Read the full document before your first code PR.

## Commit messages

We follow [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/). The format:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`.

Scopes match the architecture: `probe`, `recorder`, `replayer`, `format`, `compare`, `diff`, `cli`, `refsim`. The scope is optional when a change spans the whole workspace.

Breaking changes append `!` after the type or scope and add a `BREAKING CHANGE:` footer. Until 1.0 the format may break, but the history must still say so honestly.

Examples:

```
feat(recorder): add snapshot policy configuration
fix(compare): report correct tick on light-hash mismatch
docs: clarify the self-check workflow in the readme
feat(format)!: replace header field ordering

BREAKING CHANGE: .rec files written before this commit are unreadable.
```

## Pull request flow

1. Open or find an issue first for anything beyond a trivial fix.
2. Fork, branch from `main`, and keep the branch focused on one change.
3. Make sure `cargo test`, `cargo fmt --all --check`, and `cargo clippy --all-targets -- -D warnings` all pass.
4. Fill in the pull request template. Small PRs are reviewed fast, large PRs slowly.
5. One approval merges. The maintainer squash-merges, and the squash commit message follows the commit rules above.

## Writing rules for prose

All prose in this project, meaning documentation, code comments, and commit messages, follows two rules:

1. No em dashes.
2. No parentheses for descriptions or explanations.

Write the aside as its own sentence instead. Code itself is exempt, and so are adopted standard documents like the code of conduct.

## Reporting issues

- **Bugs and desync reports:** use the issue templates. For desync reports, the more of your `.rec` and `.dump` context you can share, the better.
- **Security vulnerabilities:** never open a public issue. Follow [SECURITY.md](SECURITY.md).

## Licensing of contributions

Tickwise is dual licensed under MIT OR Apache-2.0. Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion is licensed the same way, without any additional terms or conditions. Ported or vendored third-party code must keep its original copyright notice and license text in a `THIRD_PARTY_LICENSES.md` next to it.

## Code of conduct

Participation in this project is covered by our [Code of Conduct](CODE_OF_CONDUCT.md). Be excellent to each other.
