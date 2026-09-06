//! Status codes, the thread-local last error message, and the panic
//! guard every exported function runs inside.

use std::cell::RefCell;
use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use tickwise::RecordError;
use tickwise::format::FormatError;

/// Result of every fallible call in the C surface.
///
/// Zero is success. Any other value means the call did nothing, and
/// [`tickwise_last_error_message`] describes why.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickwiseStatus {
    /// The call succeeded.
    Ok = 0,
    /// A required pointer was null.
    NullPointer = 1,
    /// An argument was out of range, for example a marker label longer
    /// than the format allows.
    InvalidArgument = 2,
    /// A string argument was not valid UTF-8.
    InvalidUtf8 = 3,
    /// Creating, writing, or finishing the file failed.
    Io = 4,
    /// Ticks must advance by exactly one between record calls.
    NonSequentialTick = 5,
    /// The recorder was already finished. Only destroy is allowed now.
    AlreadyFinished = 6,
    /// The library panicked internally. This is a Tickwise bug; please
    /// report it with the last error message.
    Panic = 7,
}

impl TickwiseStatus {
    fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok\0",
            Self::NullPointer => "null pointer\0",
            Self::InvalidArgument => "invalid argument\0",
            Self::InvalidUtf8 => "invalid utf-8\0",
            Self::Io => "io error\0",
            Self::NonSequentialTick => "non-sequential tick\0",
            Self::AlreadyFinished => "already finished\0",
            Self::Panic => "internal panic\0",
        }
    }
}

/// An error on its way to becoming a status code plus a message.
pub(crate) struct FfiError {
    status: TickwiseStatus,
    message: String,
}

impl FfiError {
    pub(crate) fn new(status: TickwiseStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(crate) fn null(what: &str) -> Self {
        Self::new(TickwiseStatus::NullPointer, format!("{what} is null"))
    }
}

impl From<RecordError> for FfiError {
    fn from(err: RecordError) -> Self {
        let status = match &err {
            RecordError::NonSequentialTick { .. } => TickwiseStatus::NonSequentialTick,
            RecordError::Format(FormatError::TooLarge) => TickwiseStatus::InvalidArgument,
            _ => TickwiseStatus::Io,
        };
        Self::new(status, err.to_string())
    }
}

impl From<std::io::Error> for FfiError {
    fn from(err: std::io::Error) -> Self {
        Self::new(TickwiseStatus::Io, err.to_string())
    }
}

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_last_error(status: TickwiseStatus, message: &str) -> TickwiseStatus {
    // A message can never contain an interior NUL after this, so the
    // CString constructor below cannot fail; the fallback keeps the
    // function total anyway.
    let sanitized = message.replace('\0', "?");
    let cstring = CString::new(sanitized).unwrap_or_default();
    LAST_ERROR.with(|slot| *slot.borrow_mut() = cstring);
    status
}

/// Runs a fallible body, converting both errors and panics into a status
/// code and a last error message. Every exported function goes through
/// here, which is what makes the "never unwinds across the boundary"
/// promise true.
pub(crate) fn guard(body: impl FnOnce() -> Result<(), FfiError>) -> TickwiseStatus {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => TickwiseStatus::Ok,
        Ok(Err(err)) => set_last_error(err.status, &err.message),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic with a non-string payload".to_owned());
            set_last_error(
                TickwiseStatus::Panic,
                &format!("tickwise-ffi panicked: {message}"),
            )
        }
    }
}

/// Returns the message of the last failed call on the current thread as
/// a NUL-terminated UTF-8 string, or an empty string when no call has
/// failed yet.
///
/// The pointer stays valid until the next failing call on the same
/// thread. Copy the string if you need it longer. Never free it.
#[unsafe(no_mangle)]
pub extern "C" fn tickwise_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

/// Returns a short static name for a status code, for log lines.
///
/// The pointer refers to static storage and must not be freed.
#[unsafe(no_mangle)]
pub extern "C" fn tickwise_status_name(status: TickwiseStatus) -> *const c_char {
    status.name().as_ptr().cast()
}
