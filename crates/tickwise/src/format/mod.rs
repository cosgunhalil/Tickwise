//! The versioned `.rec` container format.
//!
//! Layout: a magic and versioned header, a stream of chunks where unknown
//! chunk types are skippable, an index for fast seeking, and a fixed-size
//! trailer holding the tick count, the index offset, and an integrity
//! checksum.
//!
//! Everything is hand encoded as little-endian values per decision #12,
//! so the core stays dependency-free. Malformed input must never panic:
//! every read is bounds checked and every failure surfaces as a
//! [`FormatError`].
//!
//! The format may break freely until 1.0.

mod chunk;
mod header;
mod reader;
pub(crate) mod wire;
mod writer;

pub use chunk::{Chunk, kind};
pub use header::{ConfigEcho, Header, SessionMeta, SnapshotPolicy};
pub use reader::{ChunkIter, RecReader};
pub use writer::RecWriter;

/// File magic, the first four bytes of every `.rec` file.
pub const MAGIC: [u8; 4] = *b"TKWS";

/// Trailer magic, the last four bytes of every `.rec` file.
pub const TRAILER_MAGIC: [u8; 4] = *b"TKWE";

/// The format version this build writes and the newest it can read.
pub const FORMAT_VERSION: u16 = 1;

/// Size of the fixed trailer in bytes: tick count, index offset,
/// checksum, and trailer magic.
pub(crate) const TRAILER_LEN: u64 = 8 + 8 + 8 + 4;

/// One entry of the seek index stored before the trailer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    /// Chunk type id.
    pub kind: u16,
    /// First tick covered by the chunk, zero when not applicable.
    pub first_tick: u64,
    /// Absolute file offset of the chunk header.
    pub offset: u64,
    /// Payload length in bytes.
    pub len: u32,
}

/// Errors produced while reading or writing `.rec` data.
///
/// Malformed input is reported through these variants and never panics.
#[derive(Debug)]
pub enum FormatError {
    /// An underlying I/O operation failed.
    Io(std::io::Error),
    /// The file does not start with the `TKWS` magic.
    BadMagic([u8; 4]),
    /// The file does not end with the `TKWE` trailer magic.
    BadTrailerMagic([u8; 4]),
    /// The file was written by a newer format version than this build reads.
    UnsupportedVersion(u16),
    /// The data ends before a complete structure could be read.
    Truncated,
    /// A declared length exceeds the format's safety limits.
    TooLarge,
    /// A string field holds bytes that are not valid UTF-8.
    InvalidUtf8,
    /// The stored checksum does not match the recomputed one.
    ChecksumMismatch {
        /// Checksum stored in the trailer.
        stored: u64,
        /// Checksum recomputed from the file bytes.
        computed: u64,
    },
    /// A structural invariant does not hold, with a short reason.
    Corrupt(&'static str),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::BadMagic(found) => {
                write!(
                    f,
                    "not a .rec file, expected TKWS magic, found {found:02x?}"
                )
            }
            Self::BadTrailerMagic(found) => {
                write!(f, "missing TKWE trailer magic, found {found:02x?}")
            }
            Self::UnsupportedVersion(version) => write!(
                f,
                "format version {version} is newer than this build supports, \
                 upgrade tickwise to read this file"
            ),
            Self::Truncated => write!(f, "data ends unexpectedly, the file is truncated"),
            Self::TooLarge => write!(f, "a declared length exceeds the format safety limits"),
            Self::InvalidUtf8 => write!(f, "a string field is not valid utf-8"),
            Self::ChecksumMismatch { stored, computed } => write!(
                f,
                "checksum mismatch, stored {stored:016x} but computed {computed:016x}, \
                 the file is corrupt"
            ),
            Self::Corrupt(reason) => write!(f, "corrupt file: {reason}"),
        }
    }
}

impl std::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FormatError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Truncated
        } else {
            Self::Io(err)
        }
    }
}
