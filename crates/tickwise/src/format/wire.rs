//! Low-level wire primitives shared by the reader and the writer.
//!
//! All integers are little endian. Strings are a u16 length followed by
//! UTF-8 bytes. Reads are bounds checked and never panic.

use super::FormatError;
use std::io::Write;

/// Maximum accepted chunk payload length in bytes.
pub const MAX_CHUNK_LEN: u32 = 256 * 1024 * 1024;

/// Maximum accepted header body length in bytes.
pub const MAX_HEADER_LEN: u32 = 1024 * 1024;

/// Streaming FNV-1a 64 digest used for the file integrity checksum.
#[derive(Debug, Clone)]
pub struct Fnv1a(u64);

impl Fnv1a {
    /// Creates a digest at the FNV-1a offset basis.
    pub fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    /// Feeds bytes into the digest.
    pub fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    /// Returns the current digest value.
    pub fn value(&self) -> u64 {
        self.0
    }
}

/// A writer wrapper that counts bytes written and feeds the checksum.
pub struct HashingWriter<W: Write> {
    inner: W,
    digest: Fnv1a,
    written: u64,
}

impl<W: Write> HashingWriter<W> {
    /// Wraps a writer with position tracking and checksumming.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            digest: Fnv1a::new(),
            written: 0,
        }
    }

    /// Returns the number of bytes written so far.
    pub fn position(&self) -> u64 {
        self.written
    }

    /// Splits into the inner writer and the digest over everything written.
    pub fn into_parts(self) -> (W, u64) {
        (self.inner, self.digest.value())
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.digest.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Appends a u16 in little endian.
pub fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a u32 in little endian.
pub fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a u64 in little endian.
pub fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends a string as a u16 length followed by UTF-8 bytes.
pub fn push_str(out: &mut Vec<u8>, value: &str) -> Result<(), FormatError> {
    let len = u16::try_from(value.len()).map_err(|_| FormatError::TooLarge)?;
    push_u16(out, len);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

/// A bounds-checked reader over a byte slice.
pub struct SliceReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> SliceReader<'a> {
    /// Wraps a slice for reading from the start.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Takes the next n bytes or fails with Truncated.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let end = self.pos.checked_add(n).ok_or(FormatError::TooLarge)?;
        if end > self.data.len() {
            return Err(FormatError::Truncated);
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    /// Reads a little endian u16.
    pub fn u16(&mut self) -> Result<u16, FormatError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Reads a little endian u32.
    pub fn u32(&mut self) -> Result<u32, FormatError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads a little endian u64.
    pub fn u64(&mut self) -> Result<u64, FormatError> {
        let bytes = self.take(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(buf))
    }

    /// Reads a u16 length followed by that many UTF-8 bytes.
    pub fn str(&mut self) -> Result<String, FormatError> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| FormatError::InvalidUtf8)
    }

    /// Returns all remaining bytes and advances to the end.
    pub fn rest(&mut self) -> &'a [u8] {
        let slice = &self.data[self.pos..];
        self.pos = self.data.len();
        slice
    }

    /// Returns true when every byte has been consumed.
    pub fn is_done(&self) -> bool {
        self.pos == self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_bounds_checked() {
        let mut reader = SliceReader::new(&[1, 2, 3]);
        assert!(reader.u16().is_ok());
        assert!(matches!(reader.u32(), Err(FormatError::Truncated)));
    }

    #[test]
    fn strings_round_trip() {
        let mut buf = Vec::new();
        push_str(&mut buf, "tickwise çalışır").unwrap();
        let mut reader = SliceReader::new(&buf);
        assert_eq!(reader.str().unwrap(), "tickwise çalışır");
        assert!(reader.is_done());
    }

    #[test]
    fn invalid_utf8_is_an_error_not_a_panic() {
        let buf = [2u8, 0, 0xff, 0xfe];
        let mut reader = SliceReader::new(&buf);
        assert!(matches!(reader.str(), Err(FormatError::InvalidUtf8)));
    }
}
