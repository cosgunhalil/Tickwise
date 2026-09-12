// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#include "HelloTickwiseScene.h"

#include <ctime>

USING_NS_CC;

Scene* HelloTickwiseScene::createScene(bool injectChaos) {
    return HelloTickwiseScene::create(injectChaos);
}

HelloTickwiseScene* HelloTickwiseScene::create(bool injectChaos) {
    HelloTickwiseScene* scene = new (std::nothrow) HelloTickwiseScene();
    if (scene) {
        scene->injectChaos_ = injectChaos;
        if (scene->init()) {
            scene->autorelease();
            return scene;
        }
    }
    delete scene;
    return nullptr;
}

bool HelloTickwiseScene::init() {
    if (!Scene::init()) {
        return false;
    }

    for (int i = 0; i < kBalls; ++i) {
        x_[i] = 1000 + i * 700;
        y_[i] = 2000 + i * 300;
        vx_[i] = 40 + i * 11;
        vy_[i] = 55 - i * 7;
    }

    label_ = Label::createWithSystemFont("Tickwise", "Arial", 18);
    label_->setAnchorPoint(Vec2(0, 1));
    label_->setPosition(Vec2(10, Director::getInstance()->getVisibleSize().height - 10));
    addChild(label_);

    recorder_ = tickwise::cocos2dx::TickwiseRecorder::create();
    recorder_->config.game_id = "tickwise-cocos-sample";
    recorder_->config.tick_rate = kTicksPerSecond;
    recorder_->config.rng_seed = rng_;
    recorder_->config.full_hash_interval = 50;
    recorder_->config.input_format_id = 1;
    recorder_->setProbe(this);
    addChild(recorder_);
    recorder_->startRecording(injectChaos_ ? "chaotic.rec" : "clean.rec");

    scheduleUpdate();
    return true;
}

void HelloTickwiseScene::update(float delta) {
    // A fixed step driven from the variable frame: as many simulation
    // ticks as the elapsed time owes, each one recorded. Tickwise sees
    // ticks, never frames.
    const float step = 1.0f / kTicksPerSecond;
    accumulator_ += delta;
    while (accumulator_ >= step && tick_ < kTicks) {
        accumulator_ -= step;
        uint8_t input = scriptedInput(tick_);
        stepSimulation(input);
        recorder_->setInputs(&input, 1);
        recorder_->recordTick();
    }

    if (tick_ >= kTicks && recorder_->isRecording()) {
        recorder_->stopRecording();
        label_->setString(StringUtils::format(
            "recorded %d ticks, score %llu\nsaved %s\nrun again with the other flag, then:\n  tickwise compare clean.rec chaotic.rec",
            kTicks, static_cast<unsigned long long>(score_), recorder_->getRecordingPath().c_str()));
        return;
    }
    label_->setString(StringUtils::format("tick %llu  score %llu  %s",
        static_cast<unsigned long long>(tick_), static_cast<unsigned long long>(score_),
        injectChaos_ ? "chaos" : "clean"));
}

uint8_t HelloTickwiseScene::scriptedInput(uint64_t tick) const {
    // Scripted rather than read from touch, so two runs differ only by the
    // planted bug.
    switch ((tick / 45) % 8) {
        case 0: return 1;
        case 2: return 4;
        case 4: return 2;
        case 6: return 8;
        default: return 0;
    }
}

void HelloTickwiseScene::stepSimulation(uint8_t input) {
    int nudgeX = ((input & 1) ? 8 : 0) - ((input & 2) ? 8 : 0);
    int nudgeY = ((input & 4) ? 8 : 0) - ((input & 8) ? 8 : 0);

    for (int i = 0; i < kBalls; ++i) {
        vx_[i] = std::max(-300, std::min(300, vx_[i] + nudgeX));
        vy_[i] = std::max(-300, std::min(300, vy_[i] + nudgeY));
        x_[i] += vx_[i];
        y_[i] += vy_[i];
        if (x_[i] < 0 || x_[i] > kArena) { vx_[i] = -vx_[i]; x_[i] = std::max(0, std::min(kArena, x_[i])); ++score_; }
        if (y_[i] < 0 || y_[i] > kArena) { vy_[i] = -vy_[i]; y_[i] = std::max(0, std::min(kArena, y_[i])); ++score_; }
    }

    // The planted bug: wall clock time leaking into the first ball's
    // position from the chaos tick on. Applied last so nothing can undo
    // it before the tick is recorded.
    if (injectChaos_ && tick_ >= static_cast<uint64_t>(kChaosAtTick)) {
        x_[0] += 1 + static_cast<int>(std::time(nullptr) % 7);
    }

    rng_ = rng_ * 1664525u + 1013904223u;
    ++tick_;
}

uint64_t HelloTickwiseScene::light_hash() const {
    // Score, generator, tick, and one sum over positions: cheap, and it
    // notices a ball being moved. Drop the sum and compare still finds the
    // bug, but later, when a shifted bounce changes the score.
    long long positionSum = 0;
    for (int i = 0; i < kBalls; ++i) {
        positionSum += x_[i] + y_[i];
    }
    return hasher_.reset().u64(score_).u32(rng_).u64(tick_).i64(positionSum).finish();
}

uint64_t HelloTickwiseScene::full_hash() const {
    hasher_.reset().u64(score_).u32(rng_).u64(tick_);
    for (int i = 0; i < kBalls; ++i) {
        hasher_.i32(x_[i]).i32(y_[i]).i32(vx_[i]).i32(vy_[i]);
    }
    return hasher_.finish();
}
