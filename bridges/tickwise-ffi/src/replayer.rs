//! Pass 2 over the C boundary: the replayer as push calls.
//!
//! The Rust replayer hands each tick's inputs to the caller, who steps
//! the simulation and reports back. Nothing calls into the host language.

use crate::dump::TickwiseDump;
use crate::error::{FfiError, TickwiseStatus, guard};
use tickwise::replayer::{ReplayConfig, Replayer};

/// An open replay session. Opaque to C; create with
/// `tickwise_replayer_open`, release with `tickwise_replayer_destroy`.
pub struct TickwiseReplayer {
    inner: Option<Replayer>,
    /// The inputs of the current step, copied out so the pointer handed
    /// to C stays valid until the next step regardless of what the
    /// replayer does internally.
    current_inputs: Vec<u8>,
}

/// Borrows `len` bytes at `ptr`.
///
/// # Safety
///
/// When `len` is nonzero, `ptr` must be non-null and valid for reading
/// `len` bytes for the duration of the call.
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

/// Resolves a handle to the live replayer behind it.
///
/// # Safety
///
/// `rep` must be null or a pointer returned by `tickwise_replayer_open`
/// that has not been passed to `tickwise_replayer_destroy`, and no other
/// reference to it may exist for the duration of the call.
unsafe fn live<'a>(rep: *mut TickwiseReplayer) -> Result<&'a mut TickwiseReplayer, FfiError> {
    if rep.is_null() {
        return Err(FfiError::null("replayer"));
    }
    // SAFETY: non-null, and the caller promises it came from open, was not
    // destroyed, and is not aliased during this call.
    let handle = unsafe { &mut *rep };
    if handle.inner.is_none() {
        return Err(FfiError::new(
            TickwiseStatus::AlreadyFinished,
            "replayer is already finished, only tickwise_replayer_destroy is allowed now",
        ));
    }
    Ok(handle)
}

fn inner(handle: &mut TickwiseReplayer) -> &mut Replayer {
    handle
        .inner
        .as_mut()
        .expect("live() rejects a finished replayer")
}

/// Opens a recording for replay and stores the handle in `out`.
///
/// `dump_at_ticks` names the ticks whose state dumps the replay will
/// collect; each must lie inside the recording. When `check_input_format`
/// is true, opening fails unless the recording declares
/// `expected_input_format_id`, decision #11. When `verify_hashes` is true,
/// every `after_tick` compares the live hashes against the recording.
///
/// # Safety
///
/// `path` must be valid for `path_len` readable bytes. `dump_at_ticks`
/// must be valid for `dump_count` values, or null with `dump_count == 0`.
/// `out` must be valid for writing one pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_open(
    path: *const u8,
    path_len: usize,
    dump_at_ticks: *const u64,
    dump_count: usize,
    verify_hashes: bool,
    check_input_format: bool,
    expected_input_format_id: u64,
    out: *mut *mut TickwiseReplayer,
) -> TickwiseStatus {
    guard(|| {
        if out.is_null() {
            return Err(FfiError::null("out"));
        }
        // SAFETY: path obeys this function's contract.
        let raw_path = unsafe { bytes(path, path_len, "path")? };
        let path = std::str::from_utf8(raw_path)
            .map_err(|err| FfiError::new(TickwiseStatus::InvalidUtf8, format!("path: {err}")))?;
        if path.is_empty() {
            return Err(FfiError::new(
                TickwiseStatus::InvalidArgument,
                "path is empty",
            ));
        }
        let dumps = if dump_count == 0 {
            Vec::new()
        } else {
            if dump_at_ticks.is_null() {
                return Err(FfiError::null("dump_at_ticks"));
            }
            // SAFETY: non-null and the caller promises dump_count readable
            // u64 values at dump_at_ticks for this call.
            unsafe { std::slice::from_raw_parts(dump_at_ticks, dump_count) }.to_vec()
        };
        let config = ReplayConfig {
            dump_at_ticks: dumps,
            verify_hashes,
            expected_input_format_id: check_input_format.then_some(expected_input_format_id),
        };
        let replayer = Replayer::open(path, config)?;
        let handle = Box::new(TickwiseReplayer {
            inner: Some(replayer),
            current_inputs: Vec::new(),
        });
        // SAFETY: out is non-null and the caller promises it is valid for
        // one pointer write.
        unsafe { out.write(Box::into_raw(handle)) };
        Ok(())
    })
}

/// Writes the first and last tick the recording covers.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`;
/// `first` and `last` must each be valid for one write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_tick_range(
    rep: *mut TickwiseReplayer,
    first: *mut u64,
    last: *mut u64,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rep obeys the handle contract.
        let handle = unsafe { live(rep)? };
        if first.is_null() || last.is_null() {
            return Err(FfiError::null("first or last"));
        }
        let (lo, hi) = inner(handle).tick_range();
        // SAFETY: both pointers are non-null and the caller promises each
        // is valid for one write.
        unsafe {
            first.write(lo);
            last.write(hi);
        }
        Ok(())
    })
}

/// Yields the next tick to simulate and its recorded inputs. Returns
/// false, leaving the out parameters untouched, when the recording is
/// exhausted or the handle is invalid.
///
/// The input pointer refers to memory owned by the replayer and stays
/// valid until the next call to this function or to destroy. Call
/// `tickwise_replayer_after_tick` exactly once after simulating the step.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`; `tick`,
/// `inputs`, and `inputs_len` must each be valid for one write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_next_step(
    rep: *mut TickwiseReplayer,
    tick: *mut u64,
    inputs: *mut *const u8,
    inputs_len: *mut usize,
) -> bool {
    if tick.is_null() || inputs.is_null() || inputs_len.is_null() {
        return false;
    }
    // SAFETY: rep obeys the handle contract.
    let Ok(handle) = (unsafe { live(rep) }) else {
        return false;
    };
    let Some(step) = inner(handle).next_step() else {
        return false;
    };
    let step_tick = step.tick();
    let step_inputs = step.inputs().to_vec();
    handle.current_inputs = step_inputs;
    // SAFETY: the three pointers are non-null and the caller promises each
    // is valid for one write. The data pointer refers to the handle's own
    // buffer, which lives until the next step or destroy as documented.
    unsafe {
        tick.write(step_tick);
        inputs.write(handle.current_inputs.as_ptr());
        inputs_len.write(handle.current_inputs.len());
    }
    true
}

/// Returns true when a dump was requested at this tick, so the caller
/// builds one before `tickwise_replayer_after_tick`. False for an invalid
/// handle.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_wants_dump(
    rep: *const TickwiseReplayer,
    tick: u64,
) -> bool {
    // SAFETY: rep obeys the handle contract, and this function only reads.
    unsafe { live(rep.cast_mut()) }.is_ok_and(|handle| inner(handle).wants_dump(tick))
}

/// Returns true when the recording holds a full hash at this tick and
/// verification is on, so the caller computes the expensive hash only
/// where it will be checked. False for an invalid handle.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_wants_full_hash(
    rep: *const TickwiseReplayer,
    tick: u64,
) -> bool {
    // SAFETY: rep obeys the handle contract, and this function only reads.
    unsafe { live(rep.cast_mut()) }.is_ok_and(|handle| inner(handle).wants_full_hash(tick))
}

/// Completes the pending step: stores the dump when this tick asked for
/// one, then verifies the hashes against the recording when verification
/// is on.
///
/// `full_hash` is compared only when `tickwise_replayer_wants_full_hash`
/// was true for the tick. `dump` may be null on ticks where
/// `tickwise_replayer_wants_dump` is false; passing null where a dump was
/// requested returns `MissingDump` and leaves the step pending, so the
/// call can be repeated with the dump. The dump is copied; the caller
/// keeps ownership.
///
/// A `HashMismatch` means the replay is not reproducing the recording.
/// The step is complete either way and the dump, if any, was kept, so
/// the session can still be finished and its dumps inspected.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`; `dump`
/// is null or obeys the handle contract of `tickwise_dump_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_after_tick(
    rep: *mut TickwiseReplayer,
    light_hash: u64,
    full_hash: u64,
    dump: *const TickwiseDump,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rep obeys the handle contract.
        let handle = unsafe { live(rep)? };
        let dump = if dump.is_null() {
            None
        } else {
            // SAFETY: non-null and the caller promises a live dump handle
            // that is not aliased mutably during this call.
            Some(unsafe { &*dump }.inner.clone())
        };
        inner(handle).after_tick_hashes(light_hash, full_hash, dump)?;
        Ok(())
    })
}

/// Finds the latest snapshot at or before `tick`. Returns false, leaving
/// the out parameters untouched, when there is none or the handle is
/// invalid.
///
/// A snapshot at tick T holds the state after tick T completed. Restore
/// your simulation from the bytes, then call `tickwise_replayer_seek_to`
/// with T + 1. The data pointer refers to memory owned by the replayer
/// and stays valid until destroy.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`;
/// `snapshot_tick`, `data`, and `data_len` must each be valid for one
/// write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_nearest_snapshot_before(
    rep: *const TickwiseReplayer,
    tick: u64,
    snapshot_tick: *mut u64,
    data: *mut *const u8,
    data_len: *mut usize,
) -> bool {
    if snapshot_tick.is_null() || data.is_null() || data_len.is_null() {
        return false;
    }
    // SAFETY: rep obeys the handle contract, and this function only reads.
    let Ok(handle) = (unsafe { live(rep.cast_mut()) }) else {
        return false;
    };
    let Some((found_tick, bytes)) = inner(handle).nearest_snapshot_before(tick) else {
        return false;
    };
    // SAFETY: the three pointers are non-null and the caller promises each
    // is valid for one write. The data pointer refers to the replayer's
    // own storage, which lives until destroy as documented.
    unsafe {
        snapshot_tick.write(found_tick);
        data.write(bytes.as_ptr());
        data_len.write(bytes.len());
    }
    true
}

/// Positions the replay so the next step is `tick`, after restoring state
/// from a snapshot.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_seek_to(
    rep: *mut TickwiseReplayer,
    tick: u64,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rep obeys the handle contract.
        let handle = unsafe { live(rep)? };
        inner(handle).seek_to(tick)?;
        Ok(())
    })
}

/// Writes the collected dumps as a `.dump` file at `path` and ends the
/// session. The handle stays allocated but accepts nothing except
/// destroy afterwards. Fails with `ProtocolMisuse` when a step was left
/// without its `after_tick`.
///
/// # Safety
///
/// `rep` obeys the handle contract of `tickwise_replayer_destroy`; `path`
/// must be valid for `path_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_finish(
    rep: *mut TickwiseReplayer,
    path: *const u8,
    path_len: usize,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: rep obeys the handle contract.
        let handle = unsafe { live(rep)? };
        // SAFETY: path obeys this function's contract.
        let raw_path = unsafe { bytes(path, path_len, "path")? };
        let path = std::str::from_utf8(raw_path)
            .map_err(|err| FfiError::new(TickwiseStatus::InvalidUtf8, format!("path: {err}")))?;
        if path.is_empty() {
            return Err(FfiError::new(
                TickwiseStatus::InvalidArgument,
                "path is empty",
            ));
        }
        // A protocol error must leave the session usable, so the replayer
        // is taken only once the check that would fail finish has passed.
        handle
            .inner
            .as_ref()
            .expect("live() rejects a finished replayer")
            .check_protocol()?;
        let replayer = handle
            .inner
            .take()
            .expect("live() rejects a finished replayer");
        match replayer.finish(path) {
            Ok(()) => Ok(()),
            Err(err) => Err(FfiError::from(err)),
        }
    })
}

/// Releases a replayer handle. Null is a no-op. Using the handle after
/// this call is undefined behavior.
///
/// # Safety
///
/// `rep` must be null or a pointer returned by `tickwise_replayer_open`
/// that has not already been destroyed. This is the handle contract every
/// other function in this module refers to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_replayer_destroy(rep: *mut TickwiseReplayer) {
    if rep.is_null() {
        return;
    }
    // SAFETY: non-null and, by the contract, a live Box from open that
    // nobody else references.
    drop(unsafe { Box::from_raw(rep) });
}
