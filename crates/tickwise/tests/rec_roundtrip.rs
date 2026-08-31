//! Round-trip and robustness tests for the `.rec` container.
//!
//! The coding standards require a round-trip test for every format
//! change, and that malformed input never panics. Both live here.

use std::io::Cursor;
use tickwise::format::{
    Chunk, ConfigEcho, FormatError, Header, RecReader, RecWriter, SessionMeta, SnapshotPolicy,
};

fn sample_header() -> Header {
    Header {
        meta: SessionMeta {
            game_id: "refsim".to_string(),
            build_hash: "deadbeef".to_string(),
            platform: "test".to_string(),
            tick_rate: 60,
            rng_seed: 0x0DD_BA11,
            created_at: 1_756_400_000,
        },
        config: ConfigEcho {
            full_hash_interval: 300,
            snapshot_policy: SnapshotPolicy::Every(1800),
            hash_algo_id: 1,
            input_format_id: 42,
        },
    }
}

fn sample_chunks() -> Vec<Chunk> {
    vec![
        Chunk::InputFrame {
            tick: 0,
            data: vec![1, 2, 3, 4],
        },
        Chunk::LightHashBatch {
            first_tick: 0,
            hashes: (0..64).map(|i| i * 31).collect(),
        },
        Chunk::FullHash {
            tick: 300,
            hash: 0xFEED_F00D,
        },
        Chunk::Marker {
            tick: 150,
            label: "round start".to_string(),
        },
        Chunk::Snapshot {
            tick: 1800,
            data: vec![7; 256],
        },
    ]
}

fn write_sample_file() -> Vec<u8> {
    let mut writer = RecWriter::new(Vec::new(), &sample_header()).unwrap();
    for chunk in sample_chunks() {
        writer.write_chunk(&chunk).unwrap();
    }
    // A chunk kind from the future, written raw.
    writer.write_raw_chunk(0x7777, 0, &[0xAA, 0xBB]).unwrap();
    writer.finish(2000).unwrap()
}

#[test]
fn full_file_round_trips() {
    let bytes = write_sample_file();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();

    assert_eq!(reader.version(), tickwise::format::FORMAT_VERSION);
    assert_eq!(reader.header(), &sample_header());
    assert_eq!(reader.tick_count(), 2000);

    let chunks: Vec<Chunk> = reader
        .chunks()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut expected = sample_chunks();
    expected.push(Chunk::Unknown {
        kind: 0x7777,
        payload: vec![0xAA, 0xBB],
    });
    assert_eq!(chunks, expected);

    let index = reader.read_index().unwrap();
    assert_eq!(index.len(), expected.len());
    assert_eq!(index[2].first_tick, 300);

    reader.verify_checksum().unwrap();
}

#[test]
fn unknown_chunks_are_skippable_data_not_errors() {
    let bytes = write_sample_file();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    let known = reader
        .chunks()
        .unwrap()
        .filter_map(Result::ok)
        .filter(|c| !matches!(c, Chunk::Unknown { .. }))
        .count();
    assert_eq!(known, sample_chunks().len());
}

#[test]
fn every_truncation_errors_and_never_panics() {
    let bytes = write_sample_file();
    for len in 0..bytes.len() {
        if let Ok(mut reader) = RecReader::open(Cursor::new(&bytes[..len])) {
            // A truncation that still opens must still never panic while
            // iterating, indexing, or checksumming.
            if let Ok(chunks) = reader.chunks() {
                for chunk in chunks {
                    let _ = chunk;
                }
            }
            let _ = reader.read_index();
            assert!(reader.verify_checksum().is_err());
        }
    }
}

#[test]
fn every_single_byte_corruption_is_caught_by_the_checksum() {
    let bytes = write_sample_file();
    for pos in 0..bytes.len() - 12 {
        let mut corrupt = bytes.clone();
        corrupt[pos] ^= 0xFF;
        if let Ok(mut reader) = RecReader::open(Cursor::new(&corrupt)) {
            assert!(
                reader.verify_checksum().is_err(),
                "flip at byte {pos} slipped past the checksum"
            );
        }
    }
}

#[test]
fn bad_magic_is_reported() {
    let mut bytes = write_sample_file();
    bytes[0] = b'X';
    assert!(matches!(
        RecReader::open(Cursor::new(&bytes)),
        Err(FormatError::BadMagic(_))
    ));
}

#[test]
fn future_versions_are_rejected_loudly() {
    let mut bytes = write_sample_file();
    bytes[4] = 0xFF;
    bytes[5] = 0xFF;
    assert!(matches!(
        RecReader::open(Cursor::new(&bytes)),
        Err(FormatError::UnsupportedVersion(0xFFFF))
    ));
}

#[test]
fn empty_recording_round_trips() {
    let writer = RecWriter::new(Vec::new(), &Header::default()).unwrap();
    let bytes = writer.finish(0).unwrap();
    let mut reader = RecReader::open(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.tick_count(), 0);
    assert_eq!(reader.chunks().unwrap().count(), 0);
    assert_eq!(reader.read_index().unwrap().len(), 0);
    reader.verify_checksum().unwrap();
}
