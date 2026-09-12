# Tickwise for cocos2d-x

Record and compare deterministic simulations to find desyncs, from inside a classic cocos2d-x 3.17 or 4.0 project.

[Tickwise](https://github.com/cosgunhalil/Tickwise) finds the first tick where two runs of the same simulation stopped agreeing. This bridge is a `cocos2d::Node` over the shared [C++ layer](../tickwise-cpp), which sits over the `tickwise-ffi` C ABI. It is the same design as the Unreal plugin with the engine swapped.

## Status

Under construction, milestone M10 of the Tickwise v2 roadmap. The node records, and with `dump_interval` set it dumps state on an interval so `tickwise diff` reaches field level with no replay; Pass 2 replay is a plain C++ loop over `tickwise::Replayer` from the shared header. The adapter is verified end to end in continuous integration against a declared shim of the engine calls it uses, not against a cocos2d-x build; see Testing for what that does and does not prove.

## The shape of it

```cpp
#include "tickwise/cocos2dx/TickwiseRecorder.h"

class MatchScene : public cocos2d::Scene, public tickwise::Probe {
    tickwise::cocos2dx::TickwiseRecorder* recorder_;
    mutable tickwise::Hasher hasher_;

    bool init() override {
        recorder_ = tickwise::cocos2dx::TickwiseRecorder::create();
        recorder_->config.game_id = "my-game";
        recorder_->config.rng_seed = seed_;
        recorder_->setProbe(this);
        addChild(recorder_);
        recorder_->startRecording("clean.rec");   // under the writable path
        return true;
    }

    // Your fixed step, after the simulation advanced:
    void afterTick(const uint8_t* input, size_t len) {
        recorder_->setInputs(input, len);
        recorder_->recordTick();
    }

    uint64_t light_hash() const override { return hasher_.reset().u64(score_).u32(rng_).finish(); }
    uint64_t full_hash() const override { /* every field */ }
    void state_dump(tickwise::Dump& dump) const override { /* every field, by name */ }
};
```

Leaving the scene finishes the recording. Run the game twice, or once on each of two devices, then compare offline:

```
cargo install tickwise-cli
tickwise compare clean.rec chaotic.rec
```

```
  verdict        first divergence at tick 421, caught by the light hash,
                 confirmed by the full hash at tick 450, last agreement at tick 420
```

Set `recorder_->config.dump_interval = 100` and override `state_dump`, and the recordings also carry state dumps, so `tickwise diff clean.rec chaotic.rec --at 500` names the field that moved with no replay. `recordDump()` adds one on demand at the last recorded tick, for example next to a round start marker. Each dump walks all of gameplay state, which is why the interval is opt-in. `sample/HelloTickwiseScene.cpp` does both, and its diff points at `balls[0].x`, where the planted bug lives.

## Why the node does not record on its own

`update(float)` is a variable timestep. A recording is only meaningful when every tick is a simulation tick, so the node records when you call `recordTick` and never guesses. `sample/HelloTickwiseScene.cpp` shows the usual shape: an accumulator in `update` that steps the simulation a whole number of times and records after each step.

## Adding it to a project

Two source files and two include paths. There is nothing to install.

1. Add `include/tickwise/cocos2dx/TickwiseRecorder.h` and `src/TickwiseRecorder.cpp` to your project's sources.
2. Add `bridges/tickwise-cpp/include` and `bridges/tickwise-ffi/include` to the include path.
3. Build the native library once with `cargo build --release` inside `bridges/tickwise-ffi`, and link `tickwise_ffi` for each platform you ship. The Unity release attaches prebuilt archives per platform to the GitHub Release tagged `unity/v<version>`, and they serve any C++ host.
4. Optionally drop `sample/HelloTickwiseScene.h` and `.cpp` in and make `HelloTickwiseScene::createScene(false)` your first scene.

For Android, the native library goes into `jniLibs/<abi>/` and the Android ABIs in the release archives match. For iOS, link the static library from the archive.

The CMake project here builds a static `tickwise_cocos2dx` library when `TICKWISE_COCOS2DX_ROOT` points at a cocos2d-x checkout, for projects that prefer linking to copying.

## Testing

```
cd bridges/tickwise-ffi && cargo build --release && cd ..
cmake -S tickwise-cocos2dx -B tickwise-cocos2dx/build
cmake --build tickwise-cocos2dx/build --config Release
ctest --test-dir tickwise-cocos2dx/build -C Release --output-on-failure
```

A cocos2d-x tree is hundreds of megabytes and a long build, too heavy for this repository's continuous integration. The adapter uses four engine facilities, all unchanged across 3.x and 4.0: `Ref` reference counting, `Node` with `init`, `onEnter`, and `onExit`, `FileUtils::getWritablePath`, and `log`. `shim/cocos2d.h` declares exactly those, and the harness drives the node the way a scene would, through the real native library, recording a clean and a sabotaged session with dumps every 100 ticks and at tick 421, and every lifecycle path. Continuous integration then runs `tickwise compare` and expects tick 421, and `tickwise diff --at 421` and expects the score to differ.

What that proves: the adapter's own logic, its use of the C++ layer, and the recordings it writes. What it does not prove: that the adapter compiles against a given cocos2d-x version's headers, which is one build of the sample scene in a real project away. The shim is deliberately small so that a drift in the engine surface it mirrors would be obvious.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise.
