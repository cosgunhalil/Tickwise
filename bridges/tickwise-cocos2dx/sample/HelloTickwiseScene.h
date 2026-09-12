// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.
//
// A drop-in scene for a real cocos2d-x 3.17 or 4.0 project: eight balls in
// a box, integer math, recorded every fixed step. Run it once clean and
// once with the chaos flag, then compare the two files.
//
// To use it, add this pair of files and the adapter's two source files to
// your project, add bridges/tickwise-cpp/include and
// bridges/tickwise-ffi/include to the include path, link tickwise_ffi, and
// replace your first scene with HelloTickwiseScene::createScene(false).

#ifndef HELLO_TICKWISE_SCENE_H
#define HELLO_TICKWISE_SCENE_H

#include "cocos2d.h"
#include "tickwise/cocos2dx/TickwiseRecorder.h"

class HelloTickwiseScene : public cocos2d::Scene, public tickwise::Probe {
public:
    static cocos2d::Scene* createScene(bool injectChaos);
    static HelloTickwiseScene* create(bool injectChaos);

    bool init() override;
    void update(float delta) override;

    uint64_t light_hash() const override;
    uint64_t full_hash() const override;

private:
    static const int kBalls = 8;
    static const int kArena = 16000;
    static const int kTicks = 600;
    static const int kChaosAtTick = 421;
    static const int kTicksPerSecond = 60;

    void stepSimulation(uint8_t input);
    uint8_t scriptedInput(uint64_t tick) const;

    bool injectChaos_ = false;
    tickwise::cocos2dx::TickwiseRecorder* recorder_ = nullptr;
    mutable tickwise::Hasher hasher_;

    int x_[kBalls];
    int y_[kBalls];
    int vx_[kBalls];
    int vy_[kBalls];
    uint32_t rng_ = 12345;
    uint64_t score_ = 0;
    uint64_t tick_ = 0;
    float accumulator_ = 0.0f;
    cocos2d::Label* label_ = nullptr;
};

#endif // HELLO_TICKWISE_SCENE_H
