//! Streaming `.rec` writer.

use super::header::{Header, encode_header_body};
use super::wire::{HashingWriter, MAX_CHUNK_LEN, push_u16, push_u32, push_u64};
use super::{Chunk, FormatError, IndexEntry, MAGIC, TRAILER_MAGIC};
use std::io::Write;

/// Writes a `.rec` file front to back without seeking.
///
/// The writer tracks its own position and checksum, so any `Write` sink
/// works, including plain files and in-memory buffers. Call
/// [`finish`](RecWriter::finish) to emit the index and trailer; dropping
/// the writer without finishing leaves the output truncated and unreadable.
pub struct RecWriter<W: Write> {
    out: HashingWriter<W>,
    index: Vec<IndexEntry>,
}

impl<W: Write> RecWriter<W> {
    /// Writes the magic and header, returning a writer ready for chunks.
    pub fn new(sink: W, header: &Header) -> Result<Self, FormatError> {
        let body = encode_header_body(header)?;
        let body_len = u32::try_from(body.len()).map_err(|_| FormatError::TooLarge)?;

        let mut head = Vec::new();
        head.extend_from_slice(&MAGIC);
        push_u16(&mut head, super::FORMAT_VERSION);
        push_u16(&mut head, 0); // flags, reserved
        push_u32(&mut head, body_len);
        head.extend_from_slice(&body);

        let mut out = HashingWriter::new(sink);
        out.write_all(&head)?;
        Ok(Self {
            out,
            index: Vec::new(),
        })
    }

    /// Appends one chunk and records it in the seek index.
    pub fn write_chunk(&mut self, chunk: &Chunk) -> Result<(), FormatError> {
        let payload = chunk.encode_payload()?;
        self.write_raw_chunk(chunk.kind(), chunk.first_tick(), &payload)
    }

    /// Appends a raw chunk of any kind, including kinds this build does
    /// not know. This is the extension point that keeps unknown chunk
    /// kinds first class.
    pub fn write_raw_chunk(
        &mut self,
        kind: u16,
        first_tick: u64,
        payload: &[u8],
    ) -> Result<(), FormatError> {
        let len = u32::try_from(payload.len()).map_err(|_| FormatError::TooLarge)?;
        if len > MAX_CHUNK_LEN {
            return Err(FormatError::TooLarge);
        }

        let offset = self.out.position();
        let mut head = Vec::with_capacity(6);
        push_u16(&mut head, kind);
        push_u32(&mut head, len);
        self.out.write_all(&head)?;
        self.out.write_all(payload)?;

        self.index.push(IndexEntry {
            kind,
            first_tick,
            offset,
            len,
        });
        Ok(())
    }

    /// Writes the index and trailer, then returns the inner sink.
    pub fn finish(mut self, tick_count: u64) -> Result<W, FormatError> {
        let index_offset = self.out.position();

        let mut tail = Vec::new();
        let count = u32::try_from(self.index.len()).map_err(|_| FormatError::TooLarge)?;
        push_u32(&mut tail, count);
        for entry in &self.index {
            push_u16(&mut tail, entry.kind);
            push_u64(&mut tail, entry.first_tick);
            push_u64(&mut tail, entry.offset);
            push_u32(&mut tail, entry.len);
        }
        push_u64(&mut tail, tick_count);
        push_u64(&mut tail, index_offset);
        self.out.write_all(&tail)?;

        // The checksum covers every byte before the checksum field itself.
        let (mut sink, checksum) = self.out.into_parts();
        sink.write_all(&checksum.to_le_bytes())?;
        sink.write_all(&TRAILER_MAGIC)?;
        sink.flush()?;
        Ok(sink)
    }
}
