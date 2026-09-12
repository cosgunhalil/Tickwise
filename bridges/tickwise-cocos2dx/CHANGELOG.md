# Changelog

All notable changes to the Tickwise cocos2d-x bridge are documented in this file. It is versioned with the `tickwise-ffi` crate it depends on.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- `tickwise::cocos2dx::TickwiseRecorder`, a `cocos2d::Node` recording one tick per `recordTick` call from the game's own fixed step, resolving relative paths under the engine's writable path and finishing the recording when it leaves the scene.
- A CMake project that builds against a real cocos2d-x tree when `TICKWISE_COCOS2DX_ROOT` is set, and against a declared shim of the four engine calls the adapter uses otherwise.
- A harness that drives the node the way a scene would and records a clean and a sabotaged session through the native library, run in continuous integration on three operating systems.
- `sample/HelloTickwiseScene`, a drop-in scene for a real project with a planted bug at tick 421.
- State dumps through the shared C++ layer: `config.dump_interval` on the node records the probe's `state_dump` every N ticks from `recordTick`, and `recordDump` takes one on demand at the last recorded tick. The sample scene dumps every 100 ticks and names `balls[0].x` as the field the planted bug moves. The harness records dumps on the interval and at tick 421, and CI runs `tickwise diff --at 421` on its recordings.
