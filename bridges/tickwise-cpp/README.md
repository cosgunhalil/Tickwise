# tickwise-cpp

The C++ face of Tickwise: one header over the `tickwise-ffi` C ABI, shared by the Unreal plugin and the cocos2d-x bridge, and usable from any C++14 game on its own.

## What it adds over the C header

- `tickwise::Recorder`, a move-only RAII session. The destructor finishes an open recording, so a session that goes out of scope still leaves a readable file.
- `tickwise::Probe`, the interface your simulation implements: `light_hash` every tick, `full_hash` on the ticks the recorder keeps, and `state_dump`, which writes state by field name on dump ticks only and defaults to writing nothing.
- `tickwise::Hasher`, which builds a byte buffer field by field in a fixed little-endian layout and hashes it with xxh3, so a probe never thinks about padding or byte order. Strings are length prefixed, floats hash by bit pattern.
- `tickwise::Dump`, the state dump builder: one setter per value type under a dotted path, `len` for every collection, reused across ticks with `clear`.
- `tickwise::Replayer`, Pass 2: steps through a recording's inputs while your simulation replays them, verifies the live hashes, collects dumps at the ticks you name, and writes them as a `.dump` for `tickwise diff`.
- `tickwise::Status` and `last_error()`, because engine code builds without exceptions and this header never throws.

```cpp
#include <tickwise/tickwise.hpp>

struct Match : tickwise::Probe {
    uint64_t score = 0;
    uint32_t rng = 12345;
    int32_t units[64][2];
    mutable tickwise::Hasher hasher;

    uint64_t light_hash() const override {
        return hasher.reset().u64(score).u32(rng).finish();
    }
    uint64_t full_hash() const override {
        hasher.reset().u64(score).u32(rng);
        for (auto& u : units) hasher.i32(u[0]).i32(u[1]);
        return hasher.finish();
    }
    // The same fields by name, for the diff. Called on dump ticks only.
    void state_dump(tickwise::Dump& dump) const override {
        dump.u64("score", score).u64("rng", rng).len("units", 64);
        for (int i = 0; i < 64; ++i) {
            std::string unit = "units[" + std::to_string(i) + "]";
            dump.i64(unit + ".x", units[i][0]).i64(unit + ".y", units[i][1]);
        }
    }
};

tickwise::Config config;
config.game_id = "my-game";
config.rng_seed = seed;
config.hash_algo_id = tickwise::hash_algo::kXxh3;
config.dump_interval = 300;   // optional: dumps in the .rec, diff with no replay

tickwise::Recorder recorder;
if (recorder.open("session.rec", config) != tickwise::Status::Ok) {
    log(recorder.last_error());
}
// once per simulation tick, after the step:
recorder.record_tick(tick, input_bytes, input_len, match);
// at the end, or let the destructor do it:
recorder.finish();
```

Then, on any machine:

```
tickwise compare a.rec b.rec
tickwise diff a.rec b.rec --at 4200
```

Pass 2, when the recordings carry no dumps or you want the exact tick compare named:

```cpp
tickwise::ReplayOptions options;
options.dump_at_ticks = {4021};
options.check_input_format = true;
options.expected_input_format_id = kInputFormat;

tickwise::Replayer replayer;
if (replayer.open("a.rec", options) != tickwise::Status::Ok) {
    log(replayer.last_error());
}
Match match(seed);
tickwise::Step step;
while (replayer.next_step(step)) {
    match.advance(step.inputs, step.inputs_len);
    if (replayer.after_tick(step.tick, match) != tickwise::Status::Ok) {
        log(replayer.last_error());   // HashMismatch: the replay is not reproducing the recording
        break;
    }
}
replayer.finish("a.dump");
```

Then `tickwise diff a.dump b.dump`. The replayer never runs your simulation; the loop is yours, and `nearest_snapshot_before` plus `seek_to` let you start from a snapshot you restore yourself.

## Rules the header keeps

- **No exceptions, no RTTI required.** Every failure is a `Status`; the message is in `last_error()`.
- **No engine types.** Standard library only, C++14, so the same header compiles under Unreal, cocos2d-x, and a plain CMake project.
- **No hidden allocation on the tick path.** `record_tick` borrows your bytes. The buffers are the `Hasher`'s, which you own and reuse with `reset()`, and one `Dump` per session that is touched on dump ticks only.
- **The ABI is checked at open.** A stale native library fails `open` with a clear message instead of writing a recording nobody can read.

## Building and testing

The header needs the native library built once:

```
cd bridges/tickwise-ffi && cargo build --release && cd ..
cmake -S tickwise-cpp -B tickwise-cpp/build
cmake --build tickwise-cpp/build --config Release
ctest --test-dir tickwise-cpp/build -C Release --output-on-failure
```

`tests/harness.cpp` records the same integer simulation as the C harness twice with a dump every 100 ticks, once with a defect planted at tick 421, replays the clean session with every hash verified into a `.dump`, and drives every misuse path of both sessions. It compiles at `/W4 /WX` or `-Wall -Wextra -Werror -pedantic`. CI then runs `tickwise compare` on its recordings and expects tick 421, `tickwise diff --at 500` and expects the score to differ, and diffs the replayed `.dump` against the recording and expects agreement.

`TICKWISE_FFI_DIR` and `TICKWISE_FFI_PROFILE` are CMake cache variables for a native library that lives somewhere else.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise.
