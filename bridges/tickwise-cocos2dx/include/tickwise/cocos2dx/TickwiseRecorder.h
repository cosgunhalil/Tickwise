// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#ifndef TICKWISE_COCOS2DX_RECORDER_H
#define TICKWISE_COCOS2DX_RECORDER_H

#include "cocos2d.h"
#include <tickwise/tickwise.hpp>

#include <cstdint>
#include <string>
#include <vector>

namespace tickwise {
namespace cocos2dx {

/**
 * Records one .rec file per session, one tick per call to recordTick.
 *
 * Add the node to your scene, point it at a probe, call startRecording,
 * then call recordTick from your fixed simulation step after the step has
 * run. The node never decides what a tick is: cocos2d-x's update is a
 * variable timestep, and a recording is only meaningful when every tick
 * is a simulation tick. Leaving the scene finishes the recording.
 *
 *   auto* recorder = tickwise::cocos2dx::TickwiseRecorder::create();
 *   recorder->config.game_id = "my-game";
 *   recorder->config.rng_seed = seed;
 *   recorder->setProbe(&match);
 *   addChild(recorder);
 *   recorder->startRecording("clean.rec");
 *
 *   // in your fixed step, after the simulation advanced:
 *   recorder->setInputs(inputBytes, inputLen);
 *   recorder->recordTick();
 *
 * Two recordings of the same match then go to the command line tool:
 *
 *   tickwise compare clean.rec chaotic.rec
 *
 * With config.dump_interval set and a probe that overrides state_dump,
 * the recordings also carry state dumps, and `tickwise diff clean.rec
 * chaotic.rec` names the fields that differ with no replay.
 */
class TickwiseRecorder : public cocos2d::Node {
public:
    /** Creates an autoreleased node, the cocos2d-x way. */
    static TickwiseRecorder* create();

    /**
     * Session settings, set before startRecording. Defaults match the Rust
     * API: a full hash every 300 ticks, no snapshots, no dumps. The hash
     * algorithm is forced to xxh3, which is what the Hasher produces.
     */
    tickwise::Config config;

    /** The object that hashes your state. Must outlive the recording. */
    void setProbe(const tickwise::Probe* probe);

    /**
     * Opens a recording. A relative path is resolved under the engine's
     * writable path. Returns false on failure; getLastError says why.
     */
    bool startRecording(const std::string& path);

    /** Flushes and closes the recording. Leaving the scene does this too. */
    void stopRecording();

    /** The input bytes for the current tick. Tickwise never interprets them. */
    void setInputs(const uint8_t* data, size_t len);
    void setInputs(const std::vector<uint8_t>& inputs);

    /**
     * Records one tick with the current inputs and the probe's hashes, and
     * the probe's state dump on the ticks config.dump_interval names.
     * Call it once per simulation tick, in order, after the step has run.
     */
    bool recordTick();

    /** Records a named point in the recording, for example a round start. */
    void recordMarker(const std::string& label);

    /**
     * Records the probe's state dump at the last recorded tick, on demand,
     * for example next to a round start marker. Returns false on failure.
     */
    bool recordDump();

    bool isRecording() const;

    /** Ticks recorded so far, which is also the tick the next recordTick uses. */
    uint64_t getTick() const;

    /** The last failure, or an empty string. Recording stops at the first failure and the game keeps running. */
    const std::string& getLastError() const;

    /** The resolved absolute path of the current or last recording. */
    const std::string& getRecordingPath() const;

    bool init() override;
    void onExit() override;

protected:
    TickwiseRecorder();
    ~TickwiseRecorder() override;

private:
    void fail(const std::string& message);

    tickwise::Recorder recorder_;
    const tickwise::Probe* probe_ = nullptr;
    std::vector<uint8_t> inputs_;
    tickwise::Dump dump_;
    uint64_t nextTick_ = 0;
    std::string lastError_;
    std::string recordingPath_;
};

} // namespace cocos2dx
} // namespace tickwise

#endif // TICKWISE_COCOS2DX_RECORDER_H
