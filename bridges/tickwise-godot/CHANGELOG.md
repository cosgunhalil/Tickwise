# Changelog

All notable changes to `tickwise-godot` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Until 1.0, minor versions may break both the API and the recording format.

## [Unreleased]

### Added
- `TickwiseRecorder`, a Godot 4 node that records one tick per physics frame into a `.rec` file. Exported properties for the session metadata, the full hash interval, the input format id, and the two group names.
- Coverage by group: the script variables of nodes in `tickwise` are hashed and dumped, and nodes also in `tickwise_light` enter the light hash. Engine properties stay out.
- A variant walk covering every scalar, every vector and matrix type, arrays, dictionaries, and the packed arrays. Dictionaries are walked in sorted key order and nodes in sorted path order, so no iteration order reaches a hash.
- `state_hash` and `state_snapshot` for live checks and in-game inspection, both usable from GDScript and C#.
- A headless test project under `godot/` and an integration test that records a clean and a sabotaged session, requires two clean runs to agree, and requires `compare` to find the planted bug at tick 421.
