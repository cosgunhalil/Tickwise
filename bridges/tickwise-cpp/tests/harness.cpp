// The C++ wrapper exercised end to end, both passes: the same four-body
// integer simulation as the C harness recorded twice with a dump every
// 100 ticks, once with a defect planted at tick 421, then the clean
// recording replayed with dumps collected at two ticks, plus every misuse
// path checked for a status rather than a crash.
//
// Usage: harness <clean.rec> <chaotic.rec> <scratch-dir>
//
// Writes <scratch-dir>/clean.dump. Exit code 0 means every check passed.
// CI then runs `tickwise compare` on the two recordings and expects tick
// 421.

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
    uint64_t tick = 0;
    mutable tickwise::Hasher hasher;

    explicit Sim(uint32_t seed) : rng(seed) {}

    uint32_t next() {
        rng = rng * 1664525u + 1013904223u;
        return rng;
    }

    static void inputs_for(uint64_t tick, uint8_t out[2]) {
        out[0] = static_cast<uint8_t>(tick / 30u);
        out[1] = static_cast<uint8_t>((tick / 45u) & 1u);
    }

    void step(const uint8_t inputs[2], bool defect) {
        for (size_t i = 0; i < 4; ++i) {
            pos[i] += inputs[i % 2] + (next() & 0xFu);
        }
        score += pos[0] % 7u;
        if (defect) {
            score += 1;
        }
        ++tick;
    }

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

    // The dump: every field the full hash covers, by name, with a length
    // for the collection so the diff can tell a shorter list from a
    // matching tail.
    void state_dump(tickwise::Dump& dump) const override {
        dump.u64("score", score).u64("rng", rng).len("pos", 4);
        for (size_t i = 0; i < 4; ++i) {
            dump.u64("pos[" + std::to_string(i) + "]", pos[i]);
        }
    }

    const std::vector<uint8_t>& snapshot() const {
        full_hash();
        return hasher.bytes();
    }

    // Decision #14 in practice: the replayer hands back the bytes, the
    // simulation restores itself from them.
    void restore(const uint8_t* data, size_t len, uint64_t at_tick) {
        if (len != 44) {
            return;
        }
        for (size_t i = 0; i < 4; ++i) {
            std::memcpy(&pos[i], data + i * 8, 8);
        }
        std::memcpy(&score, data + 32, 8);
        std::memcpy(&rng, data + 40, 4);
        tick = at_tick + 1;
    }
};

tickwise::Config session_config() {
    tickwise::Config config;
    config.game_id = "cpp-harness";
    config.platform = "ctest";
    config.tick_rate = 60;
    config.rng_seed = 12345;
    config.full_hash_interval = 50;
    config.snapshot_every = 100;
    config.dump_interval = 100;
    config.hash_algo_id = tickwise::hash_algo::kXxh3;
    config.input_format_id = 42;
    return config;
}

bool record_session(const std::string& path, bool diverge) {
    tickwise::Recorder recorder;
    CHECK_STATUS(recorder.open(path, session_config()), tickwise::Status::Ok);
    if (!recorder.is_recording()) {
        return false;
    }

    Sim sim(12345);
    for (uint64_t tick = 0; tick < kTicks; ++tick) {
        uint8_t inputs[2];
        Sim::inputs_for(tick, inputs);
        sim.step(inputs, diverge && tick >= kDivergenceAt);

        CHECK(recorder.wants_dump(tick) == (tick % 100 == 0));
        // One call: hashes and, on dump ticks, the dump via the probe.
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

// Pass 2: the clean recording replayed by the same simulation, every hash
// verified, dumps collected at ticks 100 and 421.
bool replay_session(const std::string& rec_path, const std::string& dump_path) {
    tickwise::ReplayOptions options;
    options.dump_at_ticks = {100, kDivergenceAt};
    options.check_input_format = true;
    options.expected_input_format_id = 42;

    tickwise::Replayer replayer;
    CHECK_STATUS(replayer.open(rec_path, options), tickwise::Status::Ok);
    if (!replayer.is_open()) {
        return false;
    }
    uint64_t first = 99, last = 99;
    CHECK_STATUS(replayer.tick_range(first, last), tickwise::Status::Ok);
    CHECK(first == 0 && last == kTicks - 1);

    Sim sim(12345);
    tickwise::Step step;
    uint64_t steps = 0;
    while (replayer.next_step(step)) {
        uint8_t expected[2];
        Sim::inputs_for(step.tick, expected);
        CHECK(step.inputs_len == 2 && std::memcmp(step.inputs, expected, 2) == 0);
        sim.step(step.inputs, false);
        tickwise::Status status = replayer.after_tick(step.tick, sim);
        if (status != tickwise::Status::Ok) {
            std::printf("FAIL replay at tick %llu: %s\n",
                        static_cast<unsigned long long>(step.tick), replayer.last_error().c_str());
            ++failures;
            return false;
        }
        ++steps;
    }
    CHECK(steps == kTicks);
    CHECK_STATUS(replayer.finish(dump_path), tickwise::Status::Ok);
    CHECK(!replayer.is_open());
    return true;
}

// This harness is written against ABI 2; a header from another version
// would exercise the wrong surface.
static_assert(tickwise::kExpectedAbiVersion == 2, "harness.cpp is written against ABI 2");

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

    tickwise::Dump dump;
    CHECK(dump.size() == 0);
    dump.u64("a", 1).u64("a", 2).f32("b", 1.5f).str("c", "text").len("d", 3).null("e");
    CHECK(dump.size() == 5);
    CHECK(dump.clear().size() == 0);
    tickwise::Dump moved(static_cast<tickwise::Dump&&>(dump));
    CHECK(moved.raw() != nullptr && dump.raw() == nullptr);
}

void check_misuse(const std::string& scratch_dir, const std::string& clean_rec) {
    tickwise::Config config;

    tickwise::Recorder closed;
    CHECK(!closed.is_recording());
    CHECK_STATUS(closed.record_tick(0, nullptr, 0, 1, 0), tickwise::Status::NullPointer);
    CHECK_STATUS(closed.finish(), tickwise::Status::NullPointer);
    CHECK(!closed.wants_full_hash(0) && !closed.wants_snapshot(0) && !closed.wants_dump(0));

    tickwise::Recorder bad;
    CHECK_STATUS(bad.open("definitely/not/a/dir/x.rec", config), tickwise::Status::Io);
    CHECK(!bad.is_recording() && !bad.last_error().empty());

    tickwise::Recorder recorder;
    CHECK_STATUS(recorder.open(scratch_dir + "/misuse.rec", config), tickwise::Status::Ok);
    CHECK_STATUS(recorder.record_tick(0, nullptr, 0, 1, 0), tickwise::Status::Ok);
    CHECK_STATUS(recorder.record_tick(5, nullptr, 0, 1, 0), tickwise::Status::NonSequentialTick);
    CHECK(recorder.last_error().find("expected 1, got 5") != std::string::npos);
    std::string long_label(70000, 'x');
    CHECK_STATUS(recorder.record_marker(0, long_label), tickwise::Status::InvalidArgument);
    tickwise::Dump on_demand;
    on_demand.u64("x", 1);
    CHECK(!recorder.wants_dump(0));
    CHECK_STATUS(recorder.record_dump(0, on_demand), tickwise::Status::Ok);
    CHECK_STATUS(recorder.finish(), tickwise::Status::Ok);
    CHECK_STATUS(recorder.finish(), tickwise::Status::AlreadyFinished);

    // The replayer: refusals at open, then protocol slips on a live one.
    tickwise::Replayer never;
    tickwise::Step step;
    CHECK(!never.next_step(step));
    CHECK_STATUS(never.after_tick(0, 0, nullptr), tickwise::Status::NullPointer);

    tickwise::ReplayOptions wrong_format;
    wrong_format.check_input_format = true;
    wrong_format.expected_input_format_id = 41;
    tickwise::Replayer refused;
    CHECK_STATUS(refused.open(clean_rec, wrong_format), tickwise::Status::InputFormatMismatch);
    CHECK(!refused.is_open());

    tickwise::ReplayOptions far;
    far.dump_at_ticks = {kTicks + 5};
    CHECK_STATUS(refused.open(clean_rec, far), tickwise::Status::TickOutOfRange);

    tickwise::ReplayOptions options;
    options.dump_at_ticks = {2};
    tickwise::Replayer replayer;
    CHECK_STATUS(replayer.open(clean_rec, options), tickwise::Status::Ok);
    CHECK_STATUS(replayer.after_tick(0, 0, nullptr), tickwise::Status::ProtocolMisuse);
    Sim sim(12345);
    CHECK(replayer.next_step(step));
    sim.step(step.inputs, false);
    CHECK_STATUS(replayer.after_tick(step.tick, sim), tickwise::Status::Ok);
    CHECK(replayer.next_step(step));
    sim.step(step.inputs, false);
    CHECK_STATUS(replayer.after_tick(step.tick, sim), tickwise::Status::Ok);
    // Tick 2 owes a dump: the push form without one is refused, the probe
    // form supplies it.
    CHECK(replayer.next_step(step) && step.tick == 2 && replayer.wants_dump(2));
    sim.step(step.inputs, false);
    CHECK_STATUS(replayer.after_tick(sim.light_hash(), 0, nullptr), tickwise::Status::MissingDump);
    CHECK_STATUS(replayer.after_tick(step.tick, sim), tickwise::Status::Ok);
    // A wrong hash is a mismatch at its tick.
    CHECK(replayer.next_step(step));
    sim.step(step.inputs, false);
    CHECK_STATUS(replayer.after_tick(sim.light_hash() ^ 1, 0, nullptr), tickwise::Status::HashMismatch);
    CHECK(replayer.last_error().find("tick 3") != std::string::npos);
    // A snapshot restores the simulation and the replay seeks past it, so
    // the hashes at 201 and 202 come out right. The step at 201 is left
    // without its after_tick, which finish catches.
    uint64_t snap_tick = 0;
    const uint8_t* data = nullptr;
    size_t len = 0;
    CHECK(replayer.nearest_snapshot_before(250, snap_tick, data, len));
    CHECK(snap_tick == 200 && len == 44 && data != nullptr);
    sim.restore(data, len, snap_tick);
    CHECK_STATUS(replayer.seek_to(snap_tick + 1), tickwise::Status::Ok);
    CHECK(replayer.next_step(step) && step.tick == 201);
    sim.step(step.inputs, false);
    CHECK(replayer.next_step(step) && step.tick == 202);
    sim.step(step.inputs, false);
    CHECK_STATUS(replayer.after_tick(step.tick, sim), tickwise::Status::Ok);
    CHECK_STATUS(replayer.finish(scratch_dir + "/misuse.dump"), tickwise::Status::ProtocolMisuse);
    CHECK(replayer.is_open());
    CHECK_STATUS(replayer.seek_to(kTicks + 10), tickwise::Status::TickOutOfRange);
}

} // namespace

int main(int argc, char** argv) {
    if (argc != 4) {
        std::fprintf(stderr, "usage: %s <clean.rec> <chaotic.rec> <scratch-dir>\n", argv[0]);
        return 2;
    }
    std::string clean = argv[1];
    std::string chaotic = argv[2];
    std::string scratch = argv[3];

    check_version_and_hash();
    if (!record_session(clean, false) || !record_session(chaotic, true)) {
        return 1;
    }
    if (!replay_session(clean, scratch + "/clean.dump")) {
        return 1;
    }
    check_misuse(scratch, clean);

    if (failures != 0) {
        std::printf("%d check(s) failed\n", failures);
        return 1;
    }
    std::printf("harness ok: recorded %llu ticks per session, defect injected at tick %llu, "
                "replayed the clean session into %s/clean.dump\n",
                static_cast<unsigned long long>(kTicks), static_cast<unsigned long long>(kDivergenceAt),
                scratch.c_str());
    return 0;
}
