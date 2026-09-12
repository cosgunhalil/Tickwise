// Tickwise for C++: a thin, header-only wrapper over the tickwise-ffi C
// ABI, shared by the Unreal plugin and the cocos2d-x bridge.
//
// Design rules, because engine code is where they matter most:
//   - No exceptions. Unreal builds without them, so every failure comes
//     back as a Status, with the message available from last_error().
//   - No engine types. Only the standard library, C++14.
//   - No hidden allocation on the tick path. record_tick borrows the
//     caller's bytes; the only allocation is the Hasher's buffer, which
//     the caller owns and reuses.
//
// Licensed under MIT OR Apache-2.0, at your option.

#ifndef TICKWISE_HPP
#define TICKWISE_HPP

#include "tickwise.h"

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

namespace tickwise {

/// Result of every fallible call. Zero is success; the rest mirror the
/// C header one to one, so a status can be logged with status_name().
enum class Status : int {
    Ok = TICKWISE_STATUS_OK,
    NullPointer = TICKWISE_STATUS_NULL_POINTER,
    InvalidArgument = TICKWISE_STATUS_INVALID_ARGUMENT,
    InvalidUtf8 = TICKWISE_STATUS_INVALID_UTF8,
    Io = TICKWISE_STATUS_IO,
    NonSequentialTick = TICKWISE_STATUS_NON_SEQUENTIAL_TICK,
    AlreadyFinished = TICKWISE_STATUS_ALREADY_FINISHED,
    Panic = TICKWISE_STATUS_PANIC,
};

/// A short static name for a status, for log lines.
inline const char* status_name(Status status) {
    return tickwise_status_name(static_cast<enum TickwiseStatus>(status));
}

/// The native library's message for the last failed call on this
/// thread. Valid until the next failure on the same thread.
inline const char* last_error_message() {
    return tickwise_last_error_message();
}

/// Version of the C surface this header was written against. The
/// loaded library must report the same value, see abi_matches().
const uint32_t kExpectedAbiVersion = TICKWISE_ABI_VERSION;

/// True when the loaded library speaks the ABI this header expects.
/// Check it once at startup and refuse to record otherwise.
inline bool abi_matches() {
    return tickwise_ffi_abi_version() == kExpectedAbiVersion;
}

/// The native library's own version, for example "0.1.0".
inline const char* native_version() {
    return tickwise_ffi_version();
}

/// xxh3 64 bit over a buffer, the same hash the Rust side computes for
/// the same bytes, under hash_algo_id 1.
inline uint64_t xxh3_64(const void* data, size_t len) {
    return tickwise_xxh3_64(static_cast<const uint8_t*>(data), len);
}

/// hash_algo_id values for Config::hash_algo_id.
namespace hash_algo {
const uint16_t kUserDefined = TICKWISE_HASH_ALGO_USER_DEFINED;
const uint16_t kXxh3 = TICKWISE_HASH_ALGO_XXH3;
const uint16_t kBlake3 = TICKWISE_HASH_ALGO_BLAKE3;
} // namespace hash_algo

/// Recording configuration. Defaults match the Rust API: a full hash
/// every 300 ticks, no snapshots, a caller-defined hash.
struct Config {
    std::string game_id;
    std::string build_hash;
    std::string platform;
    uint32_t tick_rate = 0;
    uint64_t rng_seed = 0;
    uint64_t created_at = 0;
    uint32_t full_hash_interval = 300;
    uint32_t snapshot_every = 0;
    uint16_t hash_algo_id = hash_algo::kUserDefined;
    uint64_t input_format_id = 0;
};

/// The contract between a simulation and Tickwise: two hashes of
/// gameplay state. light_hash runs every tick and must stay well under
/// one percent of the tick; full_hash runs on the ticks the recorder
/// keeps, every 300 by default, and covers everything.
class Probe {
public:
    virtual ~Probe() {}
    virtual uint64_t light_hash() const = 0;
    virtual uint64_t full_hash() const = 0;
};

/// Builds a byte buffer field by field and hashes it with xxh3, so a
/// probe never has to think about byte order or padding. Reuse one
/// instance across ticks; reset() keeps the allocation.
///
///   hasher.reset().u64(score).u32(rng_state).i32(pos_x).i32(pos_y);
///   return hasher.finish();
class Hasher {
public:
    Hasher& reset() {
        bytes_.clear();
        return *this;
    }

    Hasher& u8(uint8_t v) { return raw(&v, 1); }
    Hasher& u16(uint16_t v) { return le(v, 2); }
    Hasher& u32(uint32_t v) { return le(v, 4); }
    Hasher& u64(uint64_t v) { return le(v, 8); }
    Hasher& i8(int8_t v) { return u8(static_cast<uint8_t>(v)); }
    Hasher& i16(int16_t v) { return u16(static_cast<uint16_t>(v)); }
    Hasher& i32(int32_t v) { return u32(static_cast<uint32_t>(v)); }
    Hasher& i64(int64_t v) { return u64(static_cast<uint64_t>(v)); }
    Hasher& boolean(bool v) { return u8(v ? 1 : 0); }

    /// Hashes the bit pattern, so two floats that print the same but
    /// differ in the last bit hash differently, which is the point.
    Hasher& f32(float v) {
        uint32_t bits;
        std::memcpy(&bits, &v, 4);
        return u32(bits);
    }

    Hasher& f64(double v) {
        uint64_t bits;
        std::memcpy(&bits, &v, 8);
        return u64(bits);
    }

    /// Length-prefixed, so "ab" then "c" never equals "a" then "bc".
    Hasher& str(const std::string& v) {
        u32(static_cast<uint32_t>(v.size()));
        return raw(v.data(), v.size());
    }

    Hasher& raw(const void* data, size_t len) {
        const uint8_t* p = static_cast<const uint8_t*>(data);
        bytes_.insert(bytes_.end(), p, p + len);
        return *this;
    }

    /// The xxh3 of everything appended since the last reset.
    uint64_t finish() const {
        return xxh3_64(bytes_.empty() ? nullptr : bytes_.data(), bytes_.size());
    }

    /// The bytes themselves, for snapshots.
    const std::vector<uint8_t>& bytes() const { return bytes_; }

private:
    Hasher& le(uint64_t v, int width) {
        for (int i = 0; i < width; ++i) {
            bytes_.push_back(static_cast<uint8_t>(v >> (8 * i)));
        }
        return *this;
    }

    std::vector<uint8_t> bytes_;
};

/// One recording session. Move-only; the destructor finishes an open
/// recording, so a session that just goes out of scope still leaves a
/// readable file.
class Recorder {
public:
    Recorder() {}
    ~Recorder() { destroy(); }

    Recorder(Recorder&& other) noexcept { steal(other); }
    Recorder& operator=(Recorder&& other) noexcept {
        if (this != &other) {
            destroy();
            steal(other);
        }
        return *this;
    }

    Recorder(const Recorder&) = delete;
    Recorder& operator=(const Recorder&) = delete;

    /// Opens a recording at a UTF-8 path. Fails when the ABI does not
    /// match, so a stale library is caught before it writes anything.
    Status open(const std::string& path, const Config& config) {
        destroy();
        if (!abi_matches()) {
            return fail(Status::InvalidArgument, "tickwise_ffi ABI version mismatch");
        }
        TickwiseRecorderConfig native;
        Status status = wrap(tickwise_recorder_config_default(&native));
        if (status != Status::Ok) {
            return status;
        }
        native.game_id = bytes_of(config.game_id);
        native.game_id_len = config.game_id.size();
        native.build_hash = bytes_of(config.build_hash);
        native.build_hash_len = config.build_hash.size();
        native.platform = bytes_of(config.platform);
        native.platform_len = config.platform.size();
        native.tick_rate = config.tick_rate;
        native.rng_seed = config.rng_seed;
        native.created_at = config.created_at;
        native.full_hash_interval = config.full_hash_interval;
        native.snapshot_every = config.snapshot_every;
        native.hash_algo_id = config.hash_algo_id;
        native.input_format_id = config.input_format_id;

        TickwiseRecorder* handle = nullptr;
        status = wrap(tickwise_recorder_create(bytes_of(path), path.size(), &native, &handle));
        if (status == Status::Ok) {
            handle_ = handle;
            finished_ = false;
        }
        return status;
    }

    /// Records one tick, asking the probe for the full hash only when
    /// the recorder will keep it.
    Status record_tick(uint64_t tick, const uint8_t* inputs, size_t inputs_len, const Probe& probe) {
        if (!handle_) {
            return fail(Status::NullPointer, "recorder is not open");
        }
        uint64_t light = probe.light_hash();
        uint64_t full = wants_full_hash(tick) ? probe.full_hash() : 0;
        return record_tick(tick, inputs, inputs_len, light, full);
    }

    /// Records one tick with hashes computed elsewhere. The full hash
    /// is ignored on ticks where wants_full_hash() is false.
    Status record_tick(uint64_t tick, const uint8_t* inputs, size_t inputs_len, uint64_t light_hash,
                       uint64_t full_hash) {
        if (!handle_) {
            return fail(Status::NullPointer, "recorder is not open");
        }
        return wrap(tickwise_recorder_record_tick(handle_, tick, inputs, inputs_len, light_hash, full_hash));
    }

    bool wants_full_hash(uint64_t tick) const {
        return handle_ && tickwise_recorder_wants_full_hash(handle_, tick);
    }

    bool wants_snapshot(uint64_t tick) const {
        return handle_ && tickwise_recorder_wants_snapshot(handle_, tick);
    }

    Status record_snapshot(uint64_t tick, const uint8_t* data, size_t len) {
        if (!handle_) {
            return fail(Status::NullPointer, "recorder is not open");
        }
        return wrap(tickwise_recorder_record_snapshot(handle_, tick, data, len));
    }

    Status record_marker(uint64_t tick, const std::string& label) {
        if (!handle_) {
            return fail(Status::NullPointer, "recorder is not open");
        }
        return wrap(tickwise_recorder_record_marker(handle_, tick, bytes_of(label), label.size()));
    }

    /// Flushes and closes the file. Later record calls fail with
    /// AlreadyFinished. Calling finish twice is itself AlreadyFinished.
    Status finish() {
        if (!handle_) {
            return fail(Status::NullPointer, "recorder is not open");
        }
        Status status = wrap(tickwise_recorder_finish(handle_));
        if (status == Status::Ok) {
            finished_ = true;
        }
        return status;
    }

    /// Finishes if still open, then releases the handle.
    void destroy() {
        if (handle_) {
            if (!finished_) {
                tickwise_recorder_finish(handle_);
            }
            tickwise_recorder_destroy(handle_);
            handle_ = nullptr;
        }
        finished_ = false;
    }

    /// True between a successful open and finish.
    bool is_recording() const { return handle_ && !finished_; }

    /// The status of the last call, and its message when it failed.
    Status last_status() const { return last_status_; }
    const std::string& last_error() const { return last_error_; }

private:
    static const uint8_t* bytes_of(const std::string& s) {
        return s.empty() ? nullptr : reinterpret_cast<const uint8_t*>(s.data());
    }

    Status wrap(enum TickwiseStatus raw) {
        Status status = static_cast<Status>(raw);
        last_status_ = status;
        if (status == Status::Ok) {
            last_error_.clear();
        } else {
            last_error_ = last_error_message();
        }
        return status;
    }

    Status fail(Status status, const char* message) {
        last_status_ = status;
        last_error_ = message;
        return status;
    }

    void steal(Recorder& other) {
        handle_ = other.handle_;
        finished_ = other.finished_;
        last_status_ = other.last_status_;
        last_error_ = other.last_error_;
        other.handle_ = nullptr;
        other.finished_ = false;
    }

    TickwiseRecorder* handle_ = nullptr;
    bool finished_ = false;
    Status last_status_ = Status::Ok;
    std::string last_error_;
};

} // namespace tickwise

#endif // TICKWISE_HPP
