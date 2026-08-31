//! Fuzz target for the .rec reader.
//!
//! The contract under test: arbitrary bytes must never panic the reader,
//! only return errors. Every entry point a corrupted file can reach is
//! exercised: open, chunk iteration, index reading, and checksum
//! verification.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;
use tickwise::format::RecReader;

fuzz_target!(|data: &[u8]| {
    if let Ok(mut reader) = RecReader::open(Cursor::new(data)) {
        if let Ok(chunks) = reader.chunks() {
            for chunk in chunks {
                let _ = chunk;
            }
        }
        let _ = reader.read_index();
        let _ = reader.verify_checksum();
    }
});
