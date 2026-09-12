//! The dump builder: a state dump assembled field by field from the
//! engine's side of the boundary, decision #13 in C.
//!
//! A state dump is a flat, sorted list of path and value pairs. Paths are
//! dotted, with brackets for indices, the same shape every probe in the
//! repository produces: `players[2].velocity.x`, `projectiles`. Inserting
//! a path twice replaces the value.

use crate::error::{FfiError, TickwiseStatus, guard};
use tickwise::{StateDump, Value};

/// A state dump under construction. Opaque to C; create with
/// `tickwise_dump_new`, hand to a recorder or replayer, release with
/// `tickwise_dump_destroy`. The recorder and replayer copy what they
/// need, so one dump can be cleared and reused across ticks.
pub struct TickwiseDump {
    pub(crate) inner: StateDump,
}

/// Borrows `len` bytes at `ptr`.
///
/// # Safety
///
/// When `len` is nonzero, `ptr` must be non-null and valid for reading
/// `len` bytes for the duration of the call. Null with `len == 0` is an
/// empty slice.
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

/// Resolves a dump handle.
///
/// # Safety
///
/// `dump` must be null or a pointer returned by `tickwise_dump_new` that
/// has not been passed to `tickwise_dump_destroy`, and no other reference
/// to it may exist for the duration of the call.
unsafe fn live<'a>(dump: *mut TickwiseDump) -> Result<&'a mut TickwiseDump, FfiError> {
    if dump.is_null() {
        return Err(FfiError::null("dump"));
    }
    // SAFETY: non-null, and the caller promises it came from new, was not
    // destroyed, and is not aliased during this call.
    Ok(unsafe { &mut *dump })
}

/// Inserts one value under a path, the shared body of every set call.
///
/// # Safety
///
/// `dump` obeys the handle contract of `tickwise_dump_destroy`; `path`
/// must be valid for `path_len` readable bytes.
unsafe fn insert(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: Value,
) -> Result<(), FfiError> {
    // SAFETY: dump obeys the handle contract.
    let dump = unsafe { live(dump)? };
    // SAFETY: path obeys this function's contract.
    let path = unsafe { utf8(path, path_len, "path")? };
    if path.is_empty() {
        return Err(FfiError::new(
            TickwiseStatus::InvalidArgument,
            "a dump path cannot be empty",
        ));
    }
    dump.inner.insert(path, value);
    Ok(())
}

/// Creates an empty dump.
#[unsafe(no_mangle)]
pub extern "C" fn tickwise_dump_new() -> *mut TickwiseDump {
    Box::into_raw(Box::new(TickwiseDump {
        inner: StateDump::empty(),
    }))
}

/// Releases a dump. Null is a no-op. Using the handle afterwards is
/// undefined behavior.
///
/// # Safety
///
/// `dump` must be null or a pointer returned by `tickwise_dump_new` that
/// has not already been destroyed. This is the handle contract every
/// other function in this module refers to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_destroy(dump: *mut TickwiseDump) {
    if dump.is_null() {
        return;
    }
    // SAFETY: non-null and, by the contract, a live Box from new that
    // nobody else references.
    drop(unsafe { Box::from_raw(dump) });
}

/// Removes every field, keeping the handle for reuse across ticks.
///
/// # Safety
///
/// `dump` obeys the handle contract of `tickwise_dump_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_clear(dump: *mut TickwiseDump) -> TickwiseStatus {
    guard(|| {
        // SAFETY: dump obeys the handle contract.
        let dump = unsafe { live(dump)? };
        dump.inner = StateDump::empty();
        Ok(())
    })
}

/// Number of fields in the dump. Zero for a null handle.
///
/// # Safety
///
/// `dump` obeys the handle contract of `tickwise_dump_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_len(dump: *const TickwiseDump) -> usize {
    // SAFETY: dump obeys the handle contract, and this function only reads.
    unsafe { live(dump.cast_mut()) }.map_or(0, |dump| dump.inner.len())
}

/// Inserts a null, for an optional value that is absent.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` is valid for `path_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_null(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
) -> TickwiseStatus {
    // SAFETY: forwarded unchanged from this function's contract.
    guard(|| unsafe { insert(dump, path, path_len, Value::Null) })
}

/// Inserts a boolean.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` is valid for `path_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_bool(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: bool,
) -> TickwiseStatus {
    // SAFETY: forwarded unchanged from this function's contract.
    guard(|| unsafe { insert(dump, path, path_len, Value::Bool(value)) })
}

/// Inserts a signed integer.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` is valid for `path_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_i64(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: i64,
) -> TickwiseStatus {
    // SAFETY: forwarded unchanged from this function's contract.
    guard(|| unsafe { insert(dump, path, path_len, Value::I64(value)) })
}

/// Inserts an unsigned integer.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` is valid for `path_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_u64(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: u64,
) -> TickwiseStatus {
    // SAFETY: forwarded unchanged from this function's contract.
    guard(|| unsafe { insert(dump, path, path_len, Value::U64(value)) })
}

/// Inserts a 32 bit float. The diff classifies float differences by
/// magnitude against a per-width epsilon, so use this for values your
/// simulation holds as single precision.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` is valid for `path_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_f32(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: f32,
) -> TickwiseStatus {
    // SAFETY: forwarded unchanged from this function's contract.
    guard(|| unsafe { insert(dump, path, path_len, Value::F32(value)) })
}

/// Inserts a 64 bit float.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` is valid for `path_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_f64(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: f64,
) -> TickwiseStatus {
    // SAFETY: forwarded unchanged from this function's contract.
    guard(|| unsafe { insert(dump, path, path_len, Value::F64(value)) })
}

/// Inserts a UTF-8 string given as pointer and length.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` and `value` are each valid
/// for their lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_str(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: *const u8,
    value_len: usize,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: value obeys this function's contract.
        let text = unsafe { utf8(value, value_len, "value")? }.to_owned();
        // SAFETY: dump and path obey this function's contract.
        unsafe { insert(dump, path, path_len, Value::Str(text)) }
    })
}

/// Inserts raw bytes.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` and `value` are each valid
/// for their lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_bytes(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    value: *const u8,
    value_len: usize,
) -> TickwiseStatus {
    guard(|| {
        // SAFETY: value obeys this function's contract.
        let data = unsafe { bytes(value, value_len, "value")? }.to_vec();
        // SAFETY: dump and path obey this function's contract.
        unsafe { insert(dump, path, path_len, Value::Bytes(data)) }
    })
}

/// Inserts a collection length. Emit one for every list or map in your
/// state, under the collection's own path, so the diff can tell a
/// shorter list from one whose tail happens to match.
///
/// # Safety
///
/// `dump` obeys the handle contract; `path` is valid for `path_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tickwise_dump_set_len(
    dump: *mut TickwiseDump,
    path: *const u8,
    path_len: usize,
    count: u64,
) -> TickwiseStatus {
    // SAFETY: forwarded unchanged from this function's contract.
    guard(|| unsafe { insert(dump, path, path_len, Value::Len(count)) })
}
