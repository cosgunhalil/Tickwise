//! Chunk types and their payload encodings.
//!
//! A chunk on the wire is a u16 kind, a u32 payload length, and the
//! payload bytes. Readers skip kinds they do not know, which is how the
//! format grows without breaking old tools.

use super::FormatError;
use super::wire::{SliceReader, push_str, push_u32, push_u64};

/// Well-known chunk kind ids.
pub mod kind {
    /// Input bytes for one tick.
    pub const INPUT_FRAME: u16 = 1;
    /// A batch of consecutive per-tick light hashes.
    pub const LIGHT_HASH_BATCH: u16 = 2;
    /// A full hash at one tick.
    pub const FULL_HASH: u16 = 3;
    /// A serialized state snapshot at one tick.
    pub const SNAPSHOT: u16 = 4;
    /// A user-placed label at one tick.
    pub const MARKER: u16 = 5;
}

/// One decoded chunk of a `.rec` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// Opaque input bytes, decision #11.
    ///
    /// Input frames are repeat suppressed: a frame applies from its tick
    /// until the tick of the next frame, and the recorder writes a frame
    /// only when the bytes change. Ticks before the first frame have
    /// empty inputs.
    InputFrame {
        /// First tick the inputs apply to.
        tick: u64,
        /// Opaque input bytes, owned by the user's encoding.
        data: Vec<u8>,
    },
    /// Light hashes for consecutive ticks starting at first_tick.
    LightHashBatch {
        /// Tick of the first hash in the batch.
        first_tick: u64,
        /// One hash per tick, in tick order.
        hashes: Vec<u64>,
    },
    /// A full hash at one tick.
    FullHash {
        /// Tick the hash was taken at.
        tick: u64,
        /// The full hash value.
        hash: u64,
    },
    /// A serialized state snapshot at one tick.
    Snapshot {
        /// Tick the snapshot was taken at.
        tick: u64,
        /// Serialized state bytes, opaque to the format.
        data: Vec<u8>,
    },
    /// A user-placed marker, for example round start.
    Marker {
        /// Tick the marker points at.
        tick: u64,
        /// Human-readable label.
        label: String,
    },
    /// A chunk kind this build does not know. Preserved, never an error.
    Unknown {
        /// The unrecognized kind id.
        kind: u16,
        /// The raw payload bytes.
        payload: Vec<u8>,
    },
}

impl Chunk {
    /// Returns the wire kind id of this chunk.
    pub fn kind(&self) -> u16 {
        match self {
            Self::InputFrame { .. } => kind::INPUT_FRAME,
            Self::LightHashBatch { .. } => kind::LIGHT_HASH_BATCH,
            Self::FullHash { .. } => kind::FULL_HASH,
            Self::Snapshot { .. } => kind::SNAPSHOT,
            Self::Marker { .. } => kind::MARKER,
            Self::Unknown { kind, .. } => *kind,
        }
    }

    /// Returns the first tick this chunk covers, zero for unknown kinds.
    pub fn first_tick(&self) -> u64 {
        match self {
            Self::InputFrame { tick, .. }
            | Self::FullHash { tick, .. }
            | Self::Snapshot { tick, .. }
            | Self::Marker { tick, .. } => *tick,
            Self::LightHashBatch { first_tick, .. } => *first_tick,
            Self::Unknown { .. } => 0,
        }
    }

    pub(super) fn encode_payload(&self) -> Result<Vec<u8>, FormatError> {
        let mut out = Vec::new();
        match self {
            Self::InputFrame { tick, data } => {
                push_u64(&mut out, *tick);
                out.extend_from_slice(data);
            }
            Self::LightHashBatch { first_tick, hashes } => {
                push_u64(&mut out, *first_tick);
                let count = u32::try_from(hashes.len()).map_err(|_| FormatError::TooLarge)?;
                push_u32(&mut out, count);
                for hash in hashes {
                    push_u64(&mut out, *hash);
                }
            }
            Self::FullHash { tick, hash } => {
                push_u64(&mut out, *tick);
                push_u64(&mut out, *hash);
            }
            Self::Snapshot { tick, data } => {
                push_u64(&mut out, *tick);
                out.extend_from_slice(data);
            }
            Self::Marker { tick, label } => {
                push_u64(&mut out, *tick);
                push_str(&mut out, label)?;
            }
            Self::Unknown { payload, .. } => {
                out.extend_from_slice(payload);
            }
        }
        Ok(out)
    }

    pub(super) fn decode(kind_id: u16, payload: Vec<u8>) -> Result<Self, FormatError> {
        let mut reader = SliceReader::new(&payload);
        match kind_id {
            kind::INPUT_FRAME => Ok(Self::InputFrame {
                tick: reader.u64()?,
                data: reader.rest().to_vec(),
            }),
            kind::LIGHT_HASH_BATCH => {
                let first_tick = reader.u64()?;
                let count = reader.u32()? as usize;
                let mut hashes = Vec::with_capacity(count.min(1 << 16));
                for _ in 0..count {
                    hashes.push(reader.u64()?);
                }
                if !reader.is_done() {
                    return Err(FormatError::Corrupt("trailing bytes in light hash batch"));
                }
                Ok(Self::LightHashBatch { first_tick, hashes })
            }
            kind::FULL_HASH => {
                let chunk = Self::FullHash {
                    tick: reader.u64()?,
                    hash: reader.u64()?,
                };
                if !reader.is_done() {
                    return Err(FormatError::Corrupt("trailing bytes in full hash"));
                }
                Ok(chunk)
            }
            kind::SNAPSHOT => Ok(Self::Snapshot {
                tick: reader.u64()?,
                data: reader.rest().to_vec(),
            }),
            kind::MARKER => {
                let chunk = Self::Marker {
                    tick: reader.u64()?,
                    label: reader.str()?,
                };
                if !reader.is_done() {
                    return Err(FormatError::Corrupt("trailing bytes in marker"));
                }
                Ok(chunk)
            }
            other => Ok(Self::Unknown {
                kind: other,
                payload,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(chunk: Chunk) {
        let payload = chunk.encode_payload().unwrap();
        let decoded = Chunk::decode(chunk.kind(), payload).unwrap();
        assert_eq!(decoded, chunk);
    }

    #[test]
    fn every_kind_round_trips() {
        round_trip(Chunk::InputFrame {
            tick: 7,
            data: vec![1, 2, 3],
        });
        round_trip(Chunk::InputFrame {
            tick: 8,
            data: Vec::new(),
        });
        round_trip(Chunk::LightHashBatch {
            first_tick: 0,
            hashes: (0..64).collect(),
        });
        round_trip(Chunk::FullHash {
            tick: 300,
            hash: 0xDEAD_BEEF,
        });
        round_trip(Chunk::Snapshot {
            tick: 1800,
            data: vec![9; 128],
        });
        round_trip(Chunk::Marker {
            tick: 4021,
            label: "round start".to_string(),
        });
        round_trip(Chunk::Unknown {
            kind: 999,
            payload: vec![0xAB; 10],
        });
    }

    #[test]
    fn unknown_kinds_never_error() {
        let decoded = Chunk::decode(0xFFFF, vec![1, 2, 3]).unwrap();
        assert_eq!(
            decoded,
            Chunk::Unknown {
                kind: 0xFFFF,
                payload: vec![1, 2, 3],
            }
        );
    }

    #[test]
    fn truncated_known_payloads_error_not_panic() {
        let full = Chunk::LightHashBatch {
            first_tick: 5,
            hashes: vec![1, 2, 3],
        }
        .encode_payload()
        .unwrap();
        for len in 0..full.len() {
            assert!(Chunk::decode(kind::LIGHT_HASH_BATCH, full[..len].to_vec()).is_err());
        }
    }

    #[test]
    fn oversized_batch_count_is_an_error() {
        let mut payload = Vec::new();
        push_u64(&mut payload, 0);
        push_u32(&mut payload, u32::MAX);
        assert!(Chunk::decode(kind::LIGHT_HASH_BATCH, payload).is_err());
    }
}
