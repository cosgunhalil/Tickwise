// The cocos2d-x recorder node driven the way a scene would drive it, against
// the shim engine and the real native library. Records a clean and a
// sabotaged session and checks every lifecycle path.
//
// Usage: harness <scratch-dir>
//
// Writes clean.rec and chaotic.rec into the scratch directory, each with a
// state dump every 100 ticks and one at tick 421. Exit code 0 means every
// check passed; CI then runs `tickwise compare` and expects tick 421, and
// `tickwise diff --at 421` and expects the score to differ.

#include "tickwise/cocos2dx/TickwiseRecorder.h"

#include <cstdio>
#include <string>

namespace {

int failures = 0;

#define CHECK(cond)                                                            \
    do {                                                                       \
        if (!(cond)) {                                                         \
            ++failures;                                                        \
            std::printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);       \
        }                                                                      \
    } while (0)

const uint64_t kTicks = 600;
const uint64_t kDivergenceAt = 421;

// The same four-body integer simulation as the C and C++ harnesses.
struct Sim : tickwise::Probe {
    uint64_t pos[4] = {0, 0, 0, 0};
    uint64_t score = 0;
    uint32_t rng;
    mutable tickwise::Hasher hasher;

    explicit Sim(uint32_t seed) : rng(seed) {}

    void step(const uint8_t inputs[2], bool defect) {
        for (size_t i = 0; i < 4; ++i) {
            rng = rng * 1664525u + 1013904223u;
            pos[i] += inputs[i % 2] + (rng & 0xFu);
        }
        score += pos[0] % 7u;
        if (defect) {
            score += 1;
        }
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

    // Every field the full hash covers, by name, so the diff can say
    // which one moved.
    void state_dump(tickwise::Dump& dump) const override {
        dump.u64("score", score).u64("rng", rng).len("pos", 4);
        for (size_t i = 0; i < 4; ++i) {
            dump.u64("pos[" + std::to_string(i) + "]", pos[i]);
        }
    }
};

// What a scene does: create the node, configure it, add it, step, leave.
void runScene(const std::string& file, bool defect) {
    using tickwise::cocos2dx::TickwiseRecorder;

    TickwiseRecorder* recorder = TickwiseRecorder::create();
    CHECK(recorder != nullptr);
    if (!recorder) {
        return;
    }
    recorder->config.game_id = "cocos-harness";
    recorder->config.platform = "shim";
    recorder->config.tick_rate = 60;
    recorder->config.rng_seed = 12345;
    recorder->config.full_hash_interval = 50;
    recorder->config.dump_interval = 100;
    recorder->config.input_format_id = 42;

    Sim sim(12345);
    recorder->setProbe(&sim);
    recorder->onEnter();

    CHECK(!recorder->isRecording());
    CHECK(!recorder->recordTick());
    // A relative path resolves under the engine's writable path.
    CHECK(recorder->startRecording(file));
    CHECK(recorder->isRecording());
    CHECK(recorder->getRecordingPath() == cocos2d::FileUtils::getInstance()->getWritablePath() + file);

    for (uint64_t tick = 0; tick < kTicks; ++tick) {
        uint8_t inputs[2];
        inputs[0] = static_cast<uint8_t>(tick / 30u);
        inputs[1] = static_cast<uint8_t>((tick / 45u) & 1u);
        sim.step(inputs, defect && tick >= kDivergenceAt);
        recorder->setInputs(inputs, sizeof inputs);
        CHECK(recorder->recordTick());
        if (tick == 300) {
            recorder->recordMarker("round start");
        }
        if (tick == kDivergenceAt) {
            // An on-demand dump at the planted tick itself, so `diff --at 421`
            // has both sides at the very tick compare names.
            CHECK(recorder->recordDump());
        }
    }
    CHECK(recorder->getTick() == kTicks);
    CHECK(recorder->getLastError().empty());

    // Leaving the scene finishes the recording; nothing else is called.
    recorder->onExit();
    CHECK(!recorder->isRecording());
    recorder->release();
}

void checkLifecycle(const std::string& scratch) {
    using tickwise::cocos2dx::TickwiseRecorder;

    TickwiseRecorder* recorder = TickwiseRecorder::create();
    Sim sim(1);

    // No probe: recording opens, the first tick fails and stops it.
    CHECK(recorder->startRecording(scratch + "/no-probe.rec"));
    CHECK(!recorder->recordTick());
    CHECK(!recorder->isRecording());
    CHECK(recorder->getLastError().find("no probe") != std::string::npos);
    CHECK(!recorder->recordDump());

    // A bad path fails to open with a message.
    recorder->setProbe(&sim);
    CHECK(!recorder->startRecording("/definitely/not/a/dir/x.rec"));
    CHECK(!recorder->getLastError().empty());

    // Stop is idempotent; a second start after stop works.
    recorder->stopRecording();
    recorder->stopRecording();
    CHECK(recorder->startRecording(scratch + "/again.rec"));
    CHECK(recorder->recordTick());
    CHECK(recorder->getTick() == 1);
    // An on-demand dump lands at the last recorded tick.
    CHECK(recorder->recordDump());
    recorder->stopRecording();
    CHECK(!recorder->isRecording());

    // Deleting an open recorder still finishes the file.
    CHECK(recorder->startRecording(scratch + "/dropped.rec"));
    CHECK(recorder->recordTick());
    recorder->release();
}

} // namespace

int main(int argc, char** argv) {
    if (argc != 2) {
        std::fprintf(stderr, "usage: %s <scratch-dir>\n", argv[0]);
        return 2;
    }
    std::string scratch = argv[1];
    cocos2d::FileUtils::getInstance()->writablePath = scratch + "/";

    checkLifecycle(scratch);
    runScene("clean.rec", false);
    runScene("chaotic.rec", true);

    if (failures != 0) {
        std::printf("%d check(s) failed\n", failures);
        return 1;
    }
    std::printf("harness ok: recorded %llu ticks per session, defect injected at tick %llu\n",
                static_cast<unsigned long long>(kTicks), static_cast<unsigned long long>(kDivergenceAt));
    return 0;
}
