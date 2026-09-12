// Licensed under MIT OR Apache-2.0, at your option. See LICENSE-MIT and LICENSE-APACHE.

#include "tickwise/cocos2dx/TickwiseRecorder.h"

#include <ctime>
#include <new>

namespace tickwise {
namespace cocos2dx {

namespace {

bool isAbsolutePath(const std::string& path) {
    if (path.empty()) {
        return false;
    }
    if (path[0] == '/' || path[0] == '\\') {
        return true;
    }
    // A drive letter, the Windows shape of an absolute path.
    return path.size() > 2 && path[1] == ':' && (path[2] == '/' || path[2] == '\\');
}

uint64_t unixSeconds() {
    return static_cast<uint64_t>(std::time(nullptr));
}

} // namespace

TickwiseRecorder* TickwiseRecorder::create() {
    TickwiseRecorder* node = new (std::nothrow) TickwiseRecorder();
    if (node && node->init()) {
        node->autorelease();
        return node;
    }
    delete node;
    return nullptr;
}

TickwiseRecorder::TickwiseRecorder() {
    config.full_hash_interval = 300;
}

TickwiseRecorder::~TickwiseRecorder() {
    // The wrapper's destructor finishes an open recording, so a node that
    // is released without leaving a scene still leaves a readable file.
}

bool TickwiseRecorder::init() {
    return cocos2d::Node::init();
}

void TickwiseRecorder::setProbe(const tickwise::Probe* probe) {
    probe_ = probe;
}

bool TickwiseRecorder::startRecording(const std::string& path) {
    stopRecording();
    lastError_.clear();

    if (!tickwise::abi_matches()) {
        fail("the Tickwise native library speaks a different ABI than this build expects");
        return false;
    }

    recordingPath_ = isAbsolutePath(path)
        ? path
        : cocos2d::FileUtils::getInstance()->getWritablePath() + path;

    tickwise::Config session = config;
    if (session.created_at == 0) {
        session.created_at = unixSeconds();
    }
    // The Hasher produces xxh3, and a probe built on it must say so.
    session.hash_algo_id = tickwise::hash_algo::kXxh3;

    if (recorder_.open(recordingPath_, session) != tickwise::Status::Ok) {
        fail("cannot record to " + recordingPath_ + ": " + recorder_.last_error());
        return false;
    }
    nextTick_ = 0;
    cocos2d::log("tickwise: recording to %s", recordingPath_.c_str());
    return true;
}

void TickwiseRecorder::stopRecording() {
    if (!recorder_.is_recording()) {
        return;
    }
    if (recorder_.finish() != tickwise::Status::Ok) {
        lastError_ = recorder_.last_error();
        cocos2d::log("tickwise: %s", lastError_.c_str());
    }
    recorder_.destroy();
    cocos2d::log("tickwise: finished %llu ticks, saved %s",
                 static_cast<unsigned long long>(nextTick_), recordingPath_.c_str());
}

void TickwiseRecorder::setInputs(const uint8_t* data, size_t len) {
    inputs_.assign(data, data + len);
}

void TickwiseRecorder::setInputs(const std::vector<uint8_t>& inputs) {
    inputs_ = inputs;
}

bool TickwiseRecorder::recordTick() {
    if (!recorder_.is_recording()) {
        return false;
    }
    if (!probe_) {
        fail("no probe: call setProbe before recording");
        return false;
    }
    const uint8_t* data = inputs_.empty() ? nullptr : inputs_.data();
    if (recorder_.record_tick(nextTick_, data, inputs_.size(), *probe_) != tickwise::Status::Ok) {
        fail(recorder_.last_error());
        return false;
    }
    ++nextTick_;
    return true;
}

void TickwiseRecorder::recordMarker(const std::string& label) {
    if (!recorder_.is_recording()) {
        return;
    }
    uint64_t tick = nextTick_ == 0 ? 0 : nextTick_ - 1;
    if (recorder_.record_marker(tick, label) != tickwise::Status::Ok) {
        fail(recorder_.last_error());
    }
}

bool TickwiseRecorder::isRecording() const {
    return recorder_.is_recording();
}

uint64_t TickwiseRecorder::getTick() const {
    return nextTick_;
}

const std::string& TickwiseRecorder::getLastError() const {
    return lastError_;
}

const std::string& TickwiseRecorder::getRecordingPath() const {
    return recordingPath_;
}

void TickwiseRecorder::onExit() {
    // Leaving the scene ends the session, so a scene transition or a
    // quit still leaves a readable recording.
    stopRecording();
    cocos2d::Node::onExit();
}

void TickwiseRecorder::fail(const std::string& message) {
    lastError_ = message;
    cocos2d::log("tickwise: %s", message.c_str());
    // The file is already unusable, and continuing would only produce the
    // same error on every tick.
    recorder_.destroy();
}

} // namespace cocos2dx
} // namespace tickwise
