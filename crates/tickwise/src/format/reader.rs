//! Seekable `.rec` reader.

use super::header::{Header, decode_header_body};
use super::wire::{Fnv1a, MAX_CHUNK_LEN, MAX_HEADER_LEN, SliceReader};
use super::{Chunk, FormatError, IndexEntry, MAGIC, TRAILER_LEN, TRAILER_MAGIC};
use std::io::{Read, Seek, SeekFrom};

const INDEX_ENTRY_LEN: u64 = 2 + 8 + 8 + 4;

fn read_exact_into<R: Read>(src: &mut R, buf: &mut [u8]) -> Result<(), FormatError> {
    src.read_exact(buf).map_err(FormatError::from)
}

/// Reads a `.rec` file: header and trailer eagerly, chunks on demand.
///
/// Opening validates the magic, the version, and the trailer. It does not
/// verify the checksum, since that requires reading the whole file; call
/// [`verify_checksum`](RecReader::verify_checksum) when integrity matters
/// more than speed.
pub struct RecReader<R: Read + Seek> {
    src: R,
    version: u16,
    header: Header,
    tick_count: u64,
    index_offset: u64,
    stored_checksum: u64,
    chunks_start: u64,
    file_len: u64,
}

impl<R: Read + Seek> RecReader<R> {
    /// Opens and validates a `.rec` source.
    pub fn open(mut src: R) -> Result<Self, FormatError> {
        let file_len = src.seek(SeekFrom::End(0))?;
        src.seek(SeekFrom::Start(0))?;

        let mut magic = [0u8; 4];
        read_exact_into(&mut src, &mut magic)?;
        if magic != MAGIC {
            return Err(FormatError::BadMagic(magic));
        }

        let mut fixed = [0u8; 8];
        read_exact_into(&mut src, &mut fixed)?;
        let mut fixed_reader = SliceReader::new(&fixed);
        let version = fixed_reader.u16()?;
        let _flags = fixed_reader.u16()?;
        let header_len = fixed_reader.u32()?;

        if version > super::FORMAT_VERSION {
            return Err(FormatError::UnsupportedVersion(version));
        }
        if header_len > MAX_HEADER_LEN {
            return Err(FormatError::TooLarge);
        }

        let mut body = vec![0u8; header_len as usize];
        read_exact_into(&mut src, &mut body)?;
        let header = decode_header_body(&body)?;
        let chunks_start = 4 + 8 + u64::from(header_len);

        if file_len < chunks_start + TRAILER_LEN {
            return Err(FormatError::Truncated);
        }
        src.seek(SeekFrom::Start(file_len - TRAILER_LEN))?;
        let mut trailer = [0u8; TRAILER_LEN as usize];
        read_exact_into(&mut src, &mut trailer)?;
        let mut trailer_reader = SliceReader::new(&trailer);
        let tick_count = trailer_reader.u64()?;
        let index_offset = trailer_reader.u64()?;
        let stored_checksum = trailer_reader.u64()?;
        let mut trailer_magic = [0u8; 4];
        trailer_magic.copy_from_slice(trailer_reader.take(4)?);
        if trailer_magic != TRAILER_MAGIC {
            return Err(FormatError::BadTrailerMagic(trailer_magic));
        }

        if index_offset < chunks_start || index_offset > file_len - TRAILER_LEN {
            return Err(FormatError::Corrupt("index offset out of bounds"));
        }

        Ok(Self {
            src,
            version,
            header,
            tick_count,
            index_offset,
            stored_checksum,
            chunks_start,
            file_len,
        })
    }

    /// Returns the format version the file was written with.
    pub fn version(&self) -> u16 {
        self.version
    }

    /// Returns the decoded header.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Returns the total tick count recorded in the trailer.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Reads the seek index stored before the trailer.
    pub fn read_index(&mut self) -> Result<Vec<IndexEntry>, FormatError> {
        self.src.seek(SeekFrom::Start(self.index_offset))?;
        let mut count_bytes = [0u8; 4];
        read_exact_into(&mut self.src, &mut count_bytes)?;
        let count = u64::from(u32::from_le_bytes(count_bytes));

        let index_area = (self.file_len - TRAILER_LEN)
            .checked_sub(self.index_offset + 4)
            .ok_or(FormatError::Corrupt("index area too small for its count"))?;
        if count * INDEX_ENTRY_LEN != index_area {
            return Err(FormatError::Corrupt("index size does not match its area"));
        }

        let mut raw = vec![0u8; index_area as usize];
        read_exact_into(&mut self.src, &mut raw)?;
        let mut reader = SliceReader::new(&raw);
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            entries.push(IndexEntry {
                kind: reader.u16()?,
                first_tick: reader.u64()?,
                offset: reader.u64()?,
                len: reader.u32()?,
            });
        }
        Ok(entries)
    }

    /// Returns an iterator over the chunk stream in file order.
    pub fn chunks(&mut self) -> Result<ChunkIter<'_, R>, FormatError> {
        self.src.seek(SeekFrom::Start(self.chunks_start))?;
        Ok(ChunkIter {
            src: &mut self.src,
            pos: self.chunks_start,
            end: self.index_offset,
        })
    }

    /// Recomputes the checksum over the whole file and compares it to the
    /// stored one. Reads every byte, so it costs a full file pass.
    pub fn verify_checksum(&mut self) -> Result<(), FormatError> {
        self.src.seek(SeekFrom::Start(0))?;
        let covered = self.file_len - 12;
        let mut digest = Fnv1a::new();
        let mut remaining = covered;
        let mut buf = [0u8; 64 * 1024];
        while remaining > 0 {
            let take = remaining.min(buf.len() as u64) as usize;
            read_exact_into(&mut self.src, &mut buf[..take])?;
            digest.update(&buf[..take]);
            remaining -= take as u64;
        }
        let computed = digest.value();
        if computed != self.stored_checksum {
            return Err(FormatError::ChecksumMismatch {
                stored: self.stored_checksum,
                computed,
            });
        }
        Ok(())
    }
}

/// Iterator over decoded chunks, yielding errors instead of panicking on
/// malformed data.
pub struct ChunkIter<'a, R: Read + Seek> {
    src: &'a mut R,
    pos: u64,
    end: u64,
}

impl<R: Read + Seek> ChunkIter<'_, R> {
    fn read_next(&mut self) -> Result<Chunk, FormatError> {
        let mut head = [0u8; 6];
        read_exact_into(self.src, &mut head)?;
        let mut head_reader = SliceReader::new(&head);
        let kind = head_reader.u16()?;
        let len = head_reader.u32()?;

        if len > MAX_CHUNK_LEN {
            return Err(FormatError::TooLarge);
        }
        let after = self.pos + 6 + u64::from(len);
        if after > self.end {
            return Err(FormatError::Corrupt("chunk overruns the index offset"));
        }

        let mut payload = vec![0u8; len as usize];
        read_exact_into(self.src, &mut payload)?;
        self.pos = after;
        Chunk::decode(kind, payload)
    }
}

impl<R: Read + Seek> Iterator for ChunkIter<'_, R> {
    type Item = Result<Chunk, FormatError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }
        match self.read_next() {
            Ok(chunk) => Some(Ok(chunk)),
            Err(err) => {
                // Stop after the first error instead of spinning on a
                // corrupt stream.
                self.pos = self.end;
                Some(Err(err))
            }
        }
    }
}
