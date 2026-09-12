//! Fuzz target for compare and diff over pairs of recordings.
//!
//! The contract under test: two arbitrary byte strings, mutated from real
//! recordings by the fuzzer, must never panic first-divergence search or
//! the structural diff, only return errors or verdicts. The reader alone
//! is covered by `rec_reader`; this target reaches the code that walks two
//! files side by side, where a length from one file can index into the
//! other.
//!
//! Input layout: a little-endian u32 split point, then the bytes of both
//! recordings back to back. The corpus is seeded with a clean and a
//! chaotic refsim recording that carry dumps.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;
use tickwise::compare::first_divergence_from;
use tickwise::diff::{FloatPolicy, structural_from};
use tickwise::format::RecReader;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let split = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let rest = &data[4..];
    let (a, b) = rest.split_at(split.min(rest.len()));

    // Both walks take opened readers, and each consumes the reader's
    // position, so every pass opens its own pair.
    if let (Ok(mut reader_a), Ok(mut reader_b)) =
        (RecReader::open(Cursor::new(a)), RecReader::open(Cursor::new(b)))
    {
        let _ = first_divergence_from(&mut reader_a, &mut reader_b);
    }
    if let (Ok(mut reader_a), Ok(mut reader_b)) =
        (RecReader::open(Cursor::new(a)), RecReader::open(Cursor::new(b)))
    {
        let _ = structural_from(&mut reader_a, &mut reader_b, FloatPolicy::default());
    }
});
