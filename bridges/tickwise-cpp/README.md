# tickwise-cpp

The C++ face of Tickwise: one header over the `tickwise-ffi` C ABI, shared by the Unreal plugin and the cocos2d-x bridge, and usable from any C++14 game on its own.

## What it adds over the C header

- `tickwise::Recorder`, a move-only RAII session. The destructor finishes an open recording, so a session that goes out of scope still leaves a readable file.
- `tickwise::Probe`, the two-method interface your simulation implements: `light_hash` every tick, `full_hash` on the ticks the recorder keeps.
- `tickwise::Hasher`, which builds a byte buffer field by field in a fixed little-endian layout and hashes it with xxh3, so a probe never thinks about padding or byte order. Strings are length prefixed, floats hash by bit pattern.
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
};

tickwise::Config config;
config.game_id = "my-game";
config.rng_seed = seed;
config.hash_algo_id = tickwise::hash_algo::kXxh3;

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
```

## Rules the header keeps

- **No exceptions, no RTTI required.** Every failure is a `Status`; the message is in `last_error()`.
- **No engine types.** Standard library only, C++14, so the same header compiles under Unreal, cocos2d-x, and a plain CMake project.
- **No hidden allocation on the tick path.** `record_tick` borrows your bytes. The only buffer is the `Hasher`'s, which you own and reuse with `reset()`.
- **The ABI is checked at open.** A stale native library fails `open` with a clear message instead of writing a recording nobody can read.

## Building and testing

The header needs the native library built once:

```
cd bridges/tickwise-ffi && cargo build --release && cd ..
cmake -S tickwise-cpp -B tickwise-cpp/build
cmake --build tickwise-cpp/build --config Release
ctest --test-dir tickwise-cpp/build -C Release --output-on-failure
```

`tests/harness.cpp` records the same integer simulation as the C harness twice, once with a defect planted at tick 421, and drives every misuse path. It compiles at `/W4 /WX` or `-Wall -Wextra -Werror -pedantic`. CI then runs `tickwise compare` on its recordings and expects tick 421.

`TICKWISE_FFI_DIR` and `TICKWISE_FFI_PROFILE` are CMake cache variables for a native library that lives somewhere else.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise.
