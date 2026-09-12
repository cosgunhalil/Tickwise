// The C++ wrapper exercised end to end: the same four-body integer
// simulation as the C harness, recorded twice, once with a defect planted
// at tick 421, plus every misuse path checked for a status rather than a
// crash.
//
// Usage: harness <clean.rec> <chaotic.rec> <scratch-dir>
//
// Exit code 0 means every check passed. CI then runs `tickwise compare`
// on the two recordings and expects tick 421.

#include <tickwise/tickwise.hpp>

#include <cstdio>
#include <cstring>
#include <string>

namespace {

int failures = 0;

#define CHECK(cond)                                                                 \
    do {                                                                            \
        if (!(cond)) {                                                              \
            ++failures;                                                             \
            std::printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);            \
        }                                                                           \
    } while (0)

#define CHECK_STATUS(expr, expected)                                                \
    do {                                                                            \
        tickwise::Status got_ = (expr);                                             \
        if (got_ != (expected)) {                                                   \
            ++failures;                                                             \
            std::printf("FAIL %s:%d: %s returned %s, expected %s: %s\n", __FILE__,  \
                        __LINE__, #expr, tickwise::status_name(got_),              \
                        tickwise::status_name(expected),                            \
                        tickwise::last_error_message());                            \
        }                                                                           \
    } while (0)

const uint64_t kXxh3Empty = 0x2D06800538D394C2ULL;
const uint64_t kTicks = 600;
const uint64_t kDivergenceAt = 421;

struct Sim : tickwise::Probe {
    uint64_t pos[4] = {0, 0, 0, 0};
    uint64_t score = 0;
    uint32_t rng = 0;
    mutable tickwise::Hasher hasher;

    explicit Sim(uint32_t seed) : rng(seed) {}

    uint32_t next() {
        rng = rng * 1664525u + 1013904223u;
        return rng;
    }

    void step(const uint8_t inputs[2], bool defect) {
        for (size_t i = 0; i < 4; ++i) {
            pos[i] += inputs[i % 2] + (next() & 0xFu);
        }
        score += pos[0] % 7u;
        if (defect) {
            score += 1;
        }
    }

    // Score and generator only: cheap, and blind to positions on purpose
    // so the full hash has something the light hash misses.
    uint64_t light_hash() const override {
        return hasher.reset().u64(score).u32(rng).finish();
    }

    uint64_t full_hash() const override {
        hasher.reset();
        for (size_t i = 0; i < 4; ++i) {
            hasher.u64(pos[i]);
        }
        return hasher.u64(score).u32(rng).finish();
    }

    const std::vector<uint8_t>& snapshot() const {
        full_hash();
        return hasher.bytes();
    }
};

bool record_session(const std::string& path, bool diverge) {
    tickwise::Config config;
    config.game_id = "cpp-harness";
    config.platform = "ctest";
    config.tick_rate = 60;
    config.rng_seed = 12345;
    config.full_hash_interval = 50;
    config.snapshot_every = 100;
    config.hash_algo_id = tickwise::hash_algo::kXxh3;
    config.input_format_id = 42;

    tickwise::Recorder recorder;
    CHECK_STATUS(recorder.open(path, config), tickwise::Status::Ok);
    if (!recorder.is_recording()) {
        return false;
    }

    Sim sim(static_cast<uint32_t>(config.rng_seed));
    for (uint64_t tick = 0; tick < kTicks; ++tick) {
        uint8_t inputs[2];
        inputs[0] = static_cast<uint8_t>(tick / 30u);
        inputs[1] = static_cast<uint8_t>((tick / 45u) & 1u);
        sim.step(inputs, diverge && tick >= kDivergenceAt);

        CHECK_STATUS(recorder.record_tick(tick, inputs, sizeof inputs, sim), tickwise::Status::Ok);
        if (recorder.wants_snapshot(tick)) {
            const std::vector<uint8_t>& bytes = sim.snapshot();
            CHECK_STATUS(recorder.record_snapshot(tick, bytes.data(), bytes.size()), tickwise::Status::Ok);
        }
        if (tick == 300) {
            CHECK_STATUS(recorder.record_marker(tick, "round start"), tickwise::Status::Ok);
        }
    }
    CHECK_STATUS(recorder.finish(), tickwise::Status::Ok);
    CHECK(!recorder.is_recording());
    return true;
}

void check_version_and_hash() {
    CHECK(tickwise::abi_matches());
    CHECK(std::strlen(tickwise::native_version()) > 0);
    CHECK(tickwise::xxh3_64(nullptr, 0) == kXxh3Empty);

    tickwise::Hasher hasher;
    CHECK(hasher.finish() == kXxh3Empty);
    uint64_t a = hasher.reset().str("ab").str("c").finish();
    uint64_t b = hasher.reset().str("a").str("bc").finish();
    CHECK(a != b);
    uint64_t one = hasher.reset().f32(1.0f).finish();
    uint64_t next = hasher.reset().f32(1.0000001f).finish();
    CHECK(one != next);
    uint64_t again = hasher.reset().f32(1.0f).finish();
    CHECK(one == again);
}

void check_misuse(const std::string& scratch_dir) {
    tickwise::Recorder closed;
    CHECK(!closed.is_recording());
    CHECK_STATUS(closed.record_tick(0, nullptr, 0, 1, 0), tickwise::Status::NullPointer);
    CHECK_STATUS(closed.finish(), tickwise::Status::NullPointer);
    CHECK(!closed.wants_full_hash(0));
    CHECK(!closed.wants_snapshot(0));

    tickwise::Config config;
    tickwise::Recorder bad;
    CHECK_STATUS(bad.open("definitely/not/a/dir/x.rec", config), tickwise::Status::Io);
    CHECK(!bad.is_recording());
    CHECK(!bad.last_error().empty());

    tickwise::Recorder recorder;
    CHECK_STATUS(recorder.open(scratch_dir + "/misuse.rec", config), tickwise::Status::Ok);
    CHECK_STATUS(recorder.record_tick(0, nullptr, 0, 1, 0), tickwise::Status::Ok);
    CHECK_STATUS(recorder.record_tick(5, nullptr, 0, 1, 0), tickwise::Status::NonSequentialTick);
    CHECK(recorder.last_error().find("expected 1, got 5") != std::string::npos);
    std::string long_label(70000, 'x');
    CHECK_STATUS(recorder.record_marker(0, long_label), tickwise::Status::InvalidArgument);
    CHECK(recorder.wants_full_hash(300));
    CHECK(!recorder.wants_full_hash(301));
    CHECK_STATUS(recorder.finish(), tickwise::Status::Ok);
    CHECK_STATUS(recorder.finish(), tickwise::Status::AlreadyFinished);
    CHECK_STATUS(recorder.record_tick(1, nullptr, 0, 1, 0), tickwise::Status::AlreadyFinished);

    // Moving a recorder carries the session; the source is left empty.
    tickwise::Recorder source;
    CHECK_STATUS(source.open(scratch_dir + "/moved.rec", config), tickwise::Status::Ok);
    tickwise::Recorder target(static_cast<tickwise::Recorder&&>(source));
    CHECK(!source.is_recording());
    CHECK(target.is_recording());
    CHECK_STATUS(target.record_tick(0, nullptr, 0, 1, 0), tickwise::Status::Ok);
    // target's destructor finishes the file.
}

} // namespace

int main(int argc, char** argv) {
    if (argc != 4) {
        std::fprintf(stderr, "usage: %s <clean.rec> <chaotic.rec> <scratch-dir>\n", argv[0]);
        return 2;
    }

    check_version_and_hash();
    check_misuse(argv[3]);
    if (!record_session(argv[1], false) || !record_session(argv[2], true)) {
        return 1;
    }

    if (failures != 0) {
        std::printf("%d check(s) failed\n", failures);
        return 1;
    }
    std::printf("harness ok: recorded %llu ticks per session, defect injected at tick %llu\n",
                static_cast<unsigned long long>(kTicks), static_cast<unsigned long long>(kDivergenceAt));
    return 0;
}
