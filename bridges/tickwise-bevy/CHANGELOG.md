# Changelog

All notable changes to `tickwise-bevy` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Until 1.0, minor versions may break both the API and the recording format.

## [Unreleased]

### Added
- `TickwisePlugin`, recording one tick into a `.rec` file at the end of every fixed step, from `FixedLast`.
- `TickwiseScope` and the four `TickwiseAppExt` methods that declare which components and resources are gameplay state, and which of them also enter the light hash.
- `BevyProbe`, a `DeterminismProbe` over a `World` that reads registered types through `Reflect` into paths and values, with maps and sets walked in sorted order and entities in index order.
- `TickwiseSession` for markers, early finish, and the error that stopped a recording. Dropping the world finishes the file, so a run that simply ends still leaves a readable recording.
- `TickwiseInputs`, the per-tick input bytes, written by the game and never interpreted by Tickwise.
