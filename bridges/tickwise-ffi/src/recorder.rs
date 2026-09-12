//! Pass 1 over the C boundary: the recorder as push calls.
//!
//! The Rust recorder pulls hashes and dumps from a probe. Here the caller
//! computes them and passes them in, so nothing ever calls back into the
//! host language. `tickwise_recorder_wants_full_hash` and
//! `tickwise_recorder_wants_dump` tell the caller when the expensive
//! paths are due.

use crate::dump::TickwiseDump;
use crate::error::{FfiError, TickwiseStatus, guard};
use std::io::{BufWriter, Write};
use tickwise::format::SnapshotPolicy;
use tickwise::{Recorder, RecorderConfig, SessionMeta};

/// Recorder configuration as plain C data.
///
/// Strings are UTF-8 as pointer and length, without a terminating NUL.
/// A null pointer with length zero is an empty string. Fill it with
/// `tickwise_recorder_config_default` first and override what you need.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TickwiseRecorderConfig {
    /// Identifier of the game or application.
    pub game_id: *const u8,
    /// Byte length of `game_id`.
    pub game_id_len: usize,
    /// Build identifier, for example a git hash or version string.
    pub build_hash: *const u8,
    /// Byte length of `build_hash`.
    pub build_hash_len: usize,
    /// Platform the session runs on, for example windows-x86_64.
    pub platform: *const u8,
    /// Byte length of `platform`.
    pub platform_len: usize,
    /// Simulation rate in ticks per second.
    pub tick_rate: u32,
    /// Seed the simulation started from.
    pub rng_seed: u64,
    /// Creation time as unix seconds. Metadata only, never compared.
    pub created_at: u64,
    /// Full hash interval in ticks. Zero disables full hashes.
    pub full_hash_interval: u32,
    /// Snapshot interval in ticks. Zero disables snapshots.
    pub snapshot_every: u32,
    /// Identifier of the hash algorithm the caller uses, see the
    /// `TICKWISE_HASH_ALGO_*` constants.
    pub hash_algo_id: u16,
    /// Caller-declared identifier of the input encoding. Replay refuses
    /// a recording whose id differs from the build's.
    pub input_format_id: u64,
    /// State dump interval in ticks. Zero records none. With dumps in
    /// both recordings, `tickwise diff a.rec b.rec` reaches field level
    /// with no replay. Check `tickwise_recorder_wants_dump` each tick and
    /// supply the dump with `tickwise_recorder_record_dump`.
    pub dump_interval: u32,
}

/// An open recording session. Opaque to C; create with
/// `tickwise_recorder_create`, release with `tickwise_recorder_destroy`.
pub struct TickwiseRecorder {
    inner: Option<Recorder<BufWriter<std::fs::File>>>,
}

/// Borrows `len` bytes at `ptr` for the duration of the call.
///
/// # Safety
///
/// When `len` is nonzero, `ptr` must be non-null and valid for reading
/// `len` bytes that stay unmodified for the borrow's lifetime. A null
/// `ptr` with `len == 0` is accepted as an empty slice.
unsafe fn bytes<'a>(ptr: *const u8, len: usize, what: &str) -> Result<&'a [u8], FfiError> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(FfiError::null(what));
    }
    // SAFETY: the caller promises ptr is valid for len readable bytes,
    // and the null case was rejected above.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// Borrows a UTF-8 string given as pointer and length.
///
/// # Safety
///
/// Same contract as [`bytes`].
unsafe fn utf8<'a>(ptr: *const u8, len: usize, what: &str) -> Result<&'a str, FfiError> {
    // SAFETY: forwarded unchanged from this function's own contract.
    let raw = unsafe { bytes(ptr, len, what)? };
    std::str::from_utf8(raw)
        .map_err(|err| FfiError::new(TickwiseStatus::InvalidUtf8, format!("{what}: {err}")))
}

/// Resolves a handle to the live recorder behind it.
///
/// # Safety
///
/// `rec` must be null or a pointer returned by `tickwise_recorder_create`
/// that has not been passed to `tickwise_recorder_destroy`, and no other
/// reference to it may exist for the duration of the call.
unsafe fn live<'a>(
    rec: *mut TickwiseRecorder,
) -> Result<&'a mut Recorder<BufWriter<std::fs::File>>, FfiError> {
    if rec.is_null() {
        return Err(FfiError::null("recorder"));
    }
    // SAFETY: non-null, and the caller promises it came from create, was
    // not destroyed, and is not aliased during this call.
    let handle = unsafe { &mut *rec };
    handle.inner.as_mut().ok_or_else(|| {
        FfiError::new(
            TickwiseStatus::AlreadyFinished,
            "recorder is already finished, only tickwise_recorder_destroy is allowed now",
        )
    })
}

/// Fills a configuration with the defaults the Rust API uses: empty
/// metadata, a full hash every 300 ticks, no snapshots, no dumps, hash
/// algorithm 0, input format 0.
///
/// # Safety
///
/// `out` must be null or valid for writing one `TickwiseRecorderConfig`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_config_default(
    out: *mut TickwiseRecorderConfig,
) -> TickwiseStatus {
    guard(|| {
        if out.is_null() {
            return Err(FfiError::null("out"));
        }
        let defaults = RecorderConfig::default();
        let config = TickwiseRecorderConfig {
            game_id: std::ptr::null(),
            game_id_len: 0,
            build_hash: std::ptr::null(),
            build_hash_len: 0,
            platform: std::ptr::null(),
            platform_len: 0,
            tick_rate: defaults.session_meta.tick_rate,
            rng_seed: defaults.session_meta.rng_seed,
            created_at: defaults.session_meta.created_at,
            full_hash_interval: defaults.full_hash_interval,
            snapshot_every: match defaults.snapshot {
                SnapshotPolicy::Off => 0,
                SnapshotPolicy::Every(n) => n,
            },
            hash_algo_id: defaults.hash_algo_id,
            input_format_id: defaults.input_format_id,
            dump_interval: defaults.dump_interval,
        };
        // SAFETY: out is non-null and the caller promises it is valid for
        // one write of this type.
        unsafe { out.write(config) };
        Ok(())
    })
}

/// Creates a recorder writing to a new file at `path` and stores the
/// handle in `out`. On failure `out` is left untouched.
///
/// # Safety
///
/// `path` must be valid for `path_len` readable bytes. `config` must be
/// null or point to a valid configuration whose string pointers obey the
/// same rule. `out` must be valid for writing one pointer. All borrowed
/// memory is used only during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_create(
    path: *const u8,
    path_len: usize,
    config: *const TickwiseRecorderConfig,
    out: *mut *mut TickwiseRecorder,
) -> TickwiseStatus {
    guard(|| {
        if out.is_null() {
            return Err(FfiError::null("out"));
        }
        if config.is_null() {
            return Err(FfiError::null("config"));
        }
        // SAFETY: path obeys this function's contract.
        let path = unsafe { utf8(path, path_len, "path")? };
        if path.is_empty() {
            return Err(FfiError::new(
                TickwiseStatus::InvalidArgument,
                "path is empty",
            ));
        }
        // SAFETY: config is non-null and the caller promises it points to
        // a valid configuration for the duration of the call.
        let c = unsafe { *config };
        // SAFETY: each string pointer obeys the configuration contract.
        let session_meta = unsafe {
            SessionMeta {
                game_id: utf8(c.game_id, c.game_id_len, "config.game_id")?.to_owned(),
                build_hash: utf8(c.build_hash, c.build_hash_len, "config.build_hash")?.to_owned(),
                platform: utf8(c.platform, c.platform_len, "config.platform")?.to_owned(),
                tick_rate: c.tick_rate,
                rng_seed: c.rng_seed,
                created_at: c.created_at,
            }
        };
        let rust_config = RecorderConfig {
            session_meta,
            full_hash_interval: c.full_hash_interval,
            snapshot: match c.snapshot_every {
                0 => SnapshotPolicy::Off,
                n => SnapshotPolicy::Every(n),
            },
            hash_algo_id: c.hash_algo_id,
            input_format_id: c.input_format_id,
            dump_interval: c.dump_interval,
        };
        let recorder = Recorder::create(path, rust_config)?;
        let handle = Box::new(TickwiseRecorder {
            inner: Some(recorder),
        });
        // SAFETY: out is non-null and the caller promises it is valid for
        // one pointer write.
        unsafe { out.write(Box::into_raw(handle)) };
        Ok(())
    })
}

/// Records one tick: the input bytes, the light hash, and the full hash
/// when `tickwise_recorder_wants_full_hash` is true for this tick. On
/// other ticks `full_hash` is ignored and may be zero.
///
/// Call exactly once per tick, in tick order. The first call may use any
/// starting tick, every later call must advance by exactly one. Dumps are
/// separate: check `tickwise_recorder_wants_dump` and call
/// `tickwise_recorder_record_dump` after this.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`.
/// `inputs` must be valid for `inputs_len` readable bytes, or null with
/// `inputs_len == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_record_tick(
    rec: *mut TickwiseRecorder,
    tick: u64,
    inputs: *const u8,
    inputs_len: usize,
    light_hash: u64,
    full_hash: u64,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rec obeys the handle contract.
        let recorder = unsafe { live(rec)? };
        // SAFETY: inputs obeys this function's contract.
        let inputs = unsafe { bytes(inputs, inputs_len, "inputs")? };
        recorder.record_tick_hashes(tick, inputs, light_hash, full_hash)?;
        Ok(())
    })
}

/// Returns true when the recorder will keep a full hash at this tick, so
/// the caller computes the expensive hash only when it is needed.
///
/// Returns false for a null or finished recorder.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_wants_full_hash(
    rec: *const TickwiseRecorder,
    tick: u64,
) -> bool {
    // SAFETY: rec obeys the handle contract, and this function only reads.
    unsafe { live(rec.cast_mut()) }.is_ok_and(|recorder| recorder.wants_full_hash(tick))
}

/// Returns true when the configured dump interval asks for a state dump
/// at this tick. Build one with the `tickwise_dump_*` calls and hand it to
/// `tickwise_recorder_record_dump`.
///
/// Returns false for a null or finished recorder.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_wants_dump(
    rec: *const TickwiseRecorder,
    tick: u64,
) -> bool {
    // SAFETY: rec obeys the handle contract, and this function only reads.
    unsafe { live(rec.cast_mut()) }.is_ok_and(|recorder| recorder.wants_dump(tick))
}

/// Records a state dump at the given tick, on the interval or on demand.
/// The dump is copied; the caller keeps ownership and may clear and reuse
/// it.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`; `dump`
/// obeys the handle contract of `tickwise_dump_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_record_dump(
    rec: *mut TickwiseRecorder,
    tick: u64,
    dump: *const TickwiseDump,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rec obeys the handle contract.
        let recorder = unsafe { live(rec)? };
        if dump.is_null() {
            return Err(FfiError::null("dump"));
        }
        // SAFETY: non-null and the caller promises a live dump handle that
        // is not aliased mutably during this call.
        let dump = unsafe { &*dump };
        recorder.record_state_dump(tick, dump.inner.clone())?;
        Ok(())
    })
}

/// Returns true when the snapshot policy asks for a snapshot at this
/// tick. The recorder cannot serialize state itself, so the caller
/// checks this and calls `tickwise_recorder_record_snapshot` with its
/// own bytes.
///
/// Returns false for a null or finished recorder.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_wants_snapshot(
    rec: *const TickwiseRecorder,
    tick: u64,
) -> bool {
    // SAFETY: rec obeys the handle contract, and this function only reads.
    unsafe { live(rec.cast_mut()) }.is_ok_and(|recorder| recorder.wants_snapshot(tick))
}

/// Records a serialized state snapshot at the given tick.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`.
/// `data` must be valid for `data_len` readable bytes, or null with
/// `data_len == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_record_snapshot(
    rec: *mut TickwiseRecorder,
    tick: u64,
    data: *const u8,
    data_len: usize,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rec obeys the handle contract.
        let recorder = unsafe { live(rec)? };
        // SAFETY: data obeys this function's contract.
        let data = unsafe { bytes(data, data_len, "data")? };
        recorder.record_snapshot(tick, data)?;
        Ok(())
    })
}

/// Records a caller-placed marker, for example round start. The label is
/// UTF-8 as pointer and length, at most 65535 bytes.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`.
/// `label` must be valid for `label_len` readable bytes, or null with
/// `label_len == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_record_marker(
    rec: *mut TickwiseRecorder,
    tick: u64,
    label: *const u8,
    label_len: usize,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rec obeys the handle contract.
        let recorder = unsafe { live(rec)? };
        // SAFETY: label obeys this function's contract.
        let label = unsafe { utf8(label, label_len, "label")? };
        recorder.record_marker(tick, label)?;
        Ok(())
    })
}

/// Flushes the last hashes, writes the index and trailer, and closes the
/// file. The handle stays allocated but accepts nothing except
/// `tickwise_recorder_destroy` afterwards. A second finish returns the
/// `AlreadyFinished` status.
///
/// A recorder destroyed without finish leaves an unreadable file.
///
/// # Safety
///
/// `rec` obeys the handle contract of `tickwise_recorder_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_finish(rec: *mut TickwiseRecorder) -> TickwiseStatus {
    guard(|| {
        // Check liveness through the shared helper so the error messages
        // match the other calls, then take ownership for finish.
        // SAFETY: rec obeys the handle contract.
        unsafe { live(rec)? };
        // SAFETY: live succeeded, so rec is non-null, valid, and unaliased
        // for this call.
        let handle = unsafe { &mut *rec };
        let recorder = handle
            .inner
            .take()
            .ok_or_else(|| FfiError::new(TickwiseStatus::AlreadyFinished, "already finished"))?;
        let mut sink = recorder.finish()?;
        sink.flush()?;
        Ok(())
    })
}

/// Releases a recorder handle. Null is a no-op. Using the handle after
/// this call is undefined behavior.
///
/// # Safety
///
/// `rec` must be null or a pointer returned by `tickwise_recorder_create`
/// that has not already been destroyed. This is the handle contract every
/// other function in this module refers to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_recorder_destroy(rec: *mut TickwiseRecorder) {
    if rec.is_null() {
        return;
    }
    // SAFETY: non-null and, by the contract, a live Box from create that
    // nobody else references. Dropping an unfinished recorder only closes
    // the file, which cannot panic across the boundary.
    drop(unsafe { Box::from_raw(rec) });
}
