//! The `.rec` header: session metadata and the recorder config echo.

use super::FormatError;
use super::wire::{SliceReader, push_str, push_u16, push_u32, push_u64};

/// Metadata describing the recorded session.
///
/// Purely informational. Nothing here feeds hashes or comparisons, so
/// wall-clock data is allowed, and only here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionMeta {
    /// Identifier of the game or application.
    pub game_id: String,
    /// Build identifier, for example a git hash or version string.
    pub build_hash: String,
    /// Platform the session ran on, for example windows-x86_64.
    pub platform: String,
    /// Simulation rate in ticks per second.
    pub tick_rate: u32,
    /// Seed the simulation started from.
    pub rng_seed: u64,
    /// Creation time as unix seconds. Metadata only, never compared.
    pub created_at: u64,
}

/// Snapshot recording policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapshotPolicy {
    /// No snapshots are recorded.
    #[default]
    Off,
    /// A snapshot is recorded every n ticks. Zero is not a valid interval.
    Every(u32),
}

impl SnapshotPolicy {
    fn to_wire(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Every(n) => n,
        }
    }

    fn from_wire(value: u32) -> Self {
        if value == 0 {
            Self::Off
        } else {
            Self::Every(value)
        }
    }
}

/// Echo of the recorder configuration, stored so tools can interpret the
/// chunk stream without guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEcho {
    /// Interval between full hashes in ticks.
    pub full_hash_interval: u32,
    /// Snapshot recording policy.
    pub snapshot_policy: SnapshotPolicy,
    /// Identifier of the hash algorithm used for light and full hashes.
    pub hash_algo_id: u16,
    /// User-declared identifier of the input encoding, decision #11.
    /// Replay fails loudly when the recording and the build disagree.
    pub input_format_id: u64,
    /// Interval between state dumps taken at recording time, in ticks.
    /// Zero means none were scheduled, which is what every recording made
    /// before this field existed reports.
    pub dump_interval: u32,
}

impl Default for ConfigEcho {
    fn default() -> Self {
        Self {
            full_hash_interval: 300,
            snapshot_policy: SnapshotPolicy::Off,
            hash_algo_id: 0,
            input_format_id: 0,
            dump_interval: 0,
        }
    }
}

/// The complete `.rec` header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Header {
    /// Session metadata.
    pub meta: SessionMeta,
    /// Recorder configuration echo.
    pub config: ConfigEcho,
}

pub(super) fn encode_header_body(header: &Header) -> Result<Vec<u8>, FormatError> {
    let mut out = Vec::new();
    push_str(&mut out, &header.meta.game_id)?;
    push_str(&mut out, &header.meta.build_hash)?;
    push_str(&mut out, &header.meta.platform)?;
    push_u32(&mut out, header.meta.tick_rate);
    push_u64(&mut out, header.meta.rng_seed);
    push_u64(&mut out, header.meta.created_at);
    push_u32(&mut out, header.config.full_hash_interval);
    push_u32(&mut out, header.config.snapshot_policy.to_wire());
    push_u16(&mut out, header.config.hash_algo_id);
    push_u64(&mut out, header.config.input_format_id);
    // Appended after the original fields, so a reader from before this
    // field existed skips it as trailing bytes.
    push_u32(&mut out, header.config.dump_interval);
    Ok(out)
}

pub(super) fn decode_header_body(body: &[u8]) -> Result<Header, FormatError> {
    let mut reader = SliceReader::new(body);
    let header = Header {
        meta: SessionMeta {
            game_id: reader.str()?,
            build_hash: reader.str()?,
            platform: reader.str()?,
            tick_rate: reader.u32()?,
            rng_seed: reader.u64()?,
            created_at: reader.u64()?,
        },
        config: ConfigEcho {
            full_hash_interval: reader.u32()?,
            snapshot_policy: SnapshotPolicy::from_wire(reader.u32()?),
            hash_algo_id: reader.u16()?,
            input_format_id: reader.u64()?,
            // Absent in recordings made before dumps at recording time
            // existed, where no dumps were ever scheduled. Present but
            // cut short is corruption, and the read says so.
            dump_interval: if reader.is_done() { 0 } else { reader.u32()? },
        },
    };
    // Trailing bytes are tolerated: a future minor version may append
    // fields, and this reader skips what it does not know.
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> Header {
        Header {
            meta: SessionMeta {
                game_id: "refsim".to_string(),
                build_hash: "abc123".to_string(),
                platform: "windows-x86_64".to_string(),
                tick_rate: 60,
                rng_seed: 0x0DD_BA11,
                created_at: 1_756_400_000,
            },
            config: ConfigEcho {
                full_hash_interval: 300,
                snapshot_policy: SnapshotPolicy::Every(1800),
                hash_algo_id: 1,
                input_format_id: 42,
                dump_interval: 600,
            },
        }
    }

    /// The body as written before `dump_interval` existed: everything up
    /// to and including the input format id.
    fn legacy_body(header: &Header) -> Vec<u8> {
        let mut body = encode_header_body(header).unwrap();
        body.truncate(body.len() - 4);
        body
    }

    #[test]
    fn a_recording_from_before_dump_intervals_reads_as_no_dumps() {
        let header = sample_header();
        let decoded = decode_header_body(&legacy_body(&header)).unwrap();
        assert_eq!(decoded.config.dump_interval, 0);
        assert_eq!(decoded.meta, header.meta);
        assert_eq!(decoded.config.input_format_id, 42);
    }

    #[test]
    fn a_partially_written_dump_interval_is_corruption() {
        let full = encode_header_body(&sample_header()).unwrap();
        let mandatory = full.len() - 4;
        for len in mandatory + 1..full.len() {
            assert!(decode_header_body(&full[..len]).is_err(), "{len} bytes");
        }
    }

    #[test]
    fn header_round_trips() {
        let header = sample_header();
        let body = encode_header_body(&header).unwrap();
        let decoded = decode_header_body(&body).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn future_trailing_fields_are_tolerated() {
        let header = sample_header();
        let mut body = encode_header_body(&header).unwrap();
        body.extend_from_slice(&[0xAA; 16]);
        let decoded = decode_header_body(&body).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn truncated_header_is_an_error_not_a_panic() {
        // Every cut inside the mandatory fields is an error. The one
        // legal short body, the legacy one, is covered by its own test.
        let body = legacy_body(&sample_header());
        for len in 0..body.len() {
            assert!(decode_header_body(&body[..len]).is_err(), "{len} bytes");
        }
    }

    #[test]
    fn snapshot_policy_zero_means_off() {
        assert_eq!(SnapshotPolicy::from_wire(0), SnapshotPolicy::Off);
        assert_eq!(SnapshotPolicy::from_wire(9), SnapshotPolicy::Every(9));
        assert_eq!(SnapshotPolicy::Every(9).to_wire(), 9);
        assert_eq!(SnapshotPolicy::Off.to_wire(), 0);
    }
}
