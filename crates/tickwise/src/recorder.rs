//! The recording API, Pass 1 of the two-pass workflow.
//!
//! The recorder is called from inside the user's game loop, so its per-tick
//! path stays cheap: payloads are encoded into one reused scratch buffer
//! and no allocation happens in the steady state.

use crate::dump::StateDump;
use crate::format::wire::{push_u32, push_u64};
use crate::format::{Chunk, FormatError, Header, RecWriter, SessionMeta, SnapshotPolicy, kind};
use crate::probe::DeterminismProbe;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Light hashes per batch chunk.
const LIGHT_HASH_BATCH_LEN: usize = 64;

/// Errors produced while recording.
#[derive(Debug)]
pub enum RecordError {
    /// Writing or encoding the `.rec` data failed.
    Format(FormatError),
    /// Ticks must advance by exactly one between calls.
    NonSequentialTick {
        /// The tick the recorder expected.
        expected: u64,
        /// The tick it was given.
        got: u64,
    },
    /// Serializing typed inputs failed.
    #[cfg(feature = "serde")]
    InputEncode(postcard::Error),
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(err) => write!(f, "{err}"),
            Self::NonSequentialTick { expected, got } => write!(
                f,
                "non-sequential tick: expected {expected}, got {got}, \
                 record_tick must be called once per tick in order"
            ),
            #[cfg(feature = "serde")]
            Self::InputEncode(err) => write!(f, "cannot encode typed inputs: {err}"),
        }
    }
}

impl std::error::Error for RecordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format(err) => Some(err),
            Self::NonSequentialTick { .. } => None,
            #[cfg(feature = "serde")]
            Self::InputEncode(err) => Some(err),
        }
    }
}

impl From<FormatError> for RecordError {
    fn from(err: FormatError) -> Self {
        Self::Format(err)
    }
}

/// Configuration for a recording session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderConfig {
    /// Session metadata written into the header.
    pub session_meta: SessionMeta,
    /// Full hash interval in ticks. Zero disables full hashes.
    pub full_hash_interval: u32,
    /// Snapshot policy, echoed in the header and served by
    /// [`Recorder::wants_snapshot`].
    pub snapshot: SnapshotPolicy,
    /// Identifier of the hash algorithm the probe uses.
    pub hash_algo_id: u16,
    /// User-declared input encoding identifier, decision #11.
    pub input_format_id: u64,
    /// State dump interval in ticks. Zero, the default, records no dumps
    /// during Pass 1.
    ///
    /// A dump taken at recording time is the Pass 2 shortcut: two
    /// machines' dumps can be diffed with `tickwise diff a.rec b.rec`
    /// and no replay at all, which is the only way to reach field level
    /// for a desync that does not reproduce. The cost is a full
    /// [`state_dump`](DeterminismProbe::state_dump) every N ticks inside
    /// the game loop, so choose N with the same care as the full hash
    /// interval and measure it.
    pub dump_interval: u32,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            session_meta: SessionMeta::default(),
            full_hash_interval: 300,
            snapshot: SnapshotPolicy::Off,
            hash_algo_id: 0,
            input_format_id: 0,
            dump_interval: 0,
        }
    }
}

/// Records inputs, per-tick light hashes, periodic full hashes, and
/// optional snapshots into a `.rec` stream.
///
/// # Examples
///
/// ```
/// use tickwise::recorder::{RecordError, Recorder, RecorderConfig};
/// use tickwise::{DeterminismProbe, StateDump};
///
/// struct Sim {
///     frame: u64,
/// }
///
/// impl DeterminismProbe for Sim {
///     fn light_hash(&self) -> u64 {
///         self.frame
///     }
///     fn full_hash(&self) -> u64 {
///         self.frame.wrapping_mul(31)
///     }
///     fn state_dump(&self) -> StateDump {
///         StateDump::empty()
///     }
/// }
///
/// fn main() -> Result<(), RecordError> {
///     let mut sim = Sim { frame: 0 };
///     let mut rec = Recorder::new(Vec::new(), RecorderConfig::default())?;
///     for tick in 0..600 {
///         sim.frame += 1;
///         let inputs = [0u8, 0u8]; // the user's own encoding
///         rec.record_tick(tick, &inputs, &sim)?;
///     }
///     let bytes = rec.finish()?;
///     assert!(!bytes.is_empty());
///     Ok(())
/// }
/// ```
pub struct Recorder<W: Write> {
    writer: RecWriter<W>,
    full_hash_interval: u32,
    dump_interval: u32,
    snapshot: SnapshotPolicy,
    next_tick: Option<u64>,
    ticks_recorded: u64,
    batch_first_tick: u64,
    light_hashes: Vec<u64>,
    last_inputs: Vec<u8>,
    scratch: Vec<u8>,
}

impl Recorder<BufWriter<std::fs::File>> {
    /// Creates a recorder writing to a new file at the given path.
    pub fn create<P: AsRef<Path>>(path: P, config: RecorderConfig) -> Result<Self, RecordError> {
        let file = std::fs::File::create(path).map_err(FormatError::from)?;
        Self::new(BufWriter::new(file), config)
    }
}

impl<W: Write> Recorder<W> {
    /// Creates a recorder writing the header to the given sink.
    pub fn new(sink: W, config: RecorderConfig) -> Result<Self, RecordError> {
        let header = Header {
            meta: config.session_meta,
            config: crate::format::ConfigEcho {
                full_hash_interval: config.full_hash_interval,
                snapshot_policy: config.snapshot,
                hash_algo_id: config.hash_algo_id,
                input_format_id: config.input_format_id,
                dump_interval: config.dump_interval,
            },
        };
        Ok(Self {
            writer: RecWriter::new(sink, &header)?,
            full_hash_interval: config.full_hash_interval,
            dump_interval: config.dump_interval,
            snapshot: config.snapshot,
            next_tick: None,
            ticks_recorded: 0,
            batch_first_tick: 0,
            light_hashes: Vec::with_capacity(LIGHT_HASH_BATCH_LEN),
            last_inputs: Vec::new(),
            scratch: Vec::with_capacity(1024),
        })
    }

    /// Records one tick: the input bytes, the light hash, and the full
    /// hash when the tick falls on the configured interval.
    ///
    /// Call exactly once per tick, in tick order. The first call may use
    /// any starting tick, every later call must advance by exactly one.
    ///
    /// Input frames are repeat suppressed: a frame is written only when
    /// the bytes differ from the previous tick, and it applies until the
    /// next frame. Inputs rarely change, so this is the RLE the format
    /// promises, at frame granularity.
    pub fn record_tick(
        &mut self,
        tick: u64,
        inputs: &[u8],
        probe: &dyn DeterminismProbe,
    ) -> Result<(), RecordError> {
        // The probe is asked only for what this tick keeps, so the full
        // hash and the dump, the expensive paths, run on their intervals
        // and never in between.
        let light = probe.light_hash();
        let full = if self.wants_full_hash(tick) {
            probe.full_hash()
        } else {
            0
        };
        self.record_tick_hashes(tick, inputs, light, full)?;
        if self.wants_dump(tick) {
            self.write_dump(tick, probe.state_dump())?;
        }
        Ok(())
    }

    /// Records one tick from hashes the caller computed, the push form of
    /// [`record_tick`](Recorder::record_tick).
    ///
    /// This is the primitive every engine bridge stands on: nothing calls
    /// back into the host language. `full_hash` is written only on ticks
    /// where [`wants_full_hash`](Recorder::wants_full_hash) is true and is
    /// ignored otherwise, so pass zero there rather than computing it.
    /// Dumps are not scheduled through this call; check
    /// [`wants_dump`](Recorder::wants_dump) and call
    /// [`record_state_dump`](Recorder::record_state_dump) yourself.
    pub fn record_tick_hashes(
        &mut self,
        tick: u64,
        inputs: &[u8],
        light_hash: u64,
        full_hash: u64,
    ) -> Result<(), RecordError> {
        let first_tick_of_session = self.next_tick.is_none();
        if let Some(expected) = self.next_tick
            && tick != expected
        {
            return Err(RecordError::NonSequentialTick {
                expected,
                got: tick,
            });
        }
        self.next_tick = Some(tick + 1);

        if first_tick_of_session || inputs != self.last_inputs.as_slice() {
            self.scratch.clear();
            push_u64(&mut self.scratch, tick);
            self.scratch.extend_from_slice(inputs);
            self.writer
                .write_raw_chunk(kind::INPUT_FRAME, tick, &self.scratch)?;
            self.last_inputs.clear();
            self.last_inputs.extend_from_slice(inputs);
        }

        if self.light_hashes.is_empty() {
            self.batch_first_tick = tick;
        }
        self.light_hashes.push(light_hash);
        if self.light_hashes.len() == LIGHT_HASH_BATCH_LEN {
            self.flush_light_hashes()?;
        }

        if self.wants_full_hash(tick) {
            self.scratch.clear();
            push_u64(&mut self.scratch, tick);
            push_u64(&mut self.scratch, full_hash);
            self.writer
                .write_raw_chunk(kind::FULL_HASH, tick, &self.scratch)?;
        }

        self.ticks_recorded += 1;
        Ok(())
    }

    /// Returns true when [`record_tick`](Recorder::record_tick) will ask
    /// the probe for a state dump at this tick, per the configured
    /// [`dump_interval`](RecorderConfig::dump_interval).
    pub fn wants_dump(&self, tick: u64) -> bool {
        self.dump_interval > 0 && tick.is_multiple_of(u64::from(self.dump_interval))
    }

    /// Records a state dump at the given tick, on demand.
    ///
    /// For a dump at a point the interval would miss, for example next to
    /// a marker at round start, or when a live check such as an exchanged
    /// hash has just disagreed. The dump lands in the recording and
    /// `tickwise diff` reads it from there.
    pub fn record_dump(
        &mut self,
        tick: u64,
        probe: &dyn DeterminismProbe,
    ) -> Result<(), RecordError> {
        self.write_dump(tick, probe.state_dump())
    }

    /// Records a state dump the caller built, the push form of
    /// [`record_dump`](Recorder::record_dump). Engine bridges assemble
    /// the dump field by field on their side of the boundary and hand it
    /// over whole.
    pub fn record_state_dump(&mut self, tick: u64, dump: StateDump) -> Result<(), RecordError> {
        self.write_dump(tick, dump)
    }

    fn write_dump(&mut self, tick: u64, dump: StateDump) -> Result<(), RecordError> {
        self.writer.write_chunk(&Chunk::StateDump { tick, dump })?;
        Ok(())
    }

    /// Records one tick with typed inputs, serialized through postcard.
    ///
    /// The convenience twin of [`record_tick`](Recorder::record_tick).
    /// Set [`RecorderConfig::input_format_id`] from
    /// [`serde_probe::format_id`](crate::serde_probe::format_id) so the
    /// replayer can refuse recordings made with an older input type.
    #[cfg(feature = "serde")]
    pub fn record_tick_typed<I: serde::Serialize + ?Sized>(
        &mut self,
        tick: u64,
        inputs: &I,
        probe: &dyn DeterminismProbe,
    ) -> Result<(), RecordError> {
        let bytes = postcard::to_allocvec(inputs).map_err(RecordError::InputEncode)?;
        self.record_tick(tick, &bytes, probe)
    }

    /// Returns true when [`record_tick`](Recorder::record_tick) will ask
    /// the probe for a full hash at this tick.
    ///
    /// Callers that compute hashes themselves instead of handing over a
    /// probe, for example an engine bridge on the other side of a C
    /// boundary, use this to skip the expensive full hash on the ticks
    /// where the recorder would discard it anyway. Rust callers using a
    /// probe never need it.
    pub fn wants_full_hash(&self, tick: u64) -> bool {
        self.full_hash_interval > 0 && tick.is_multiple_of(u64::from(self.full_hash_interval))
    }

    /// Returns true when the snapshot policy asks for a snapshot at this
    /// tick. The recorder cannot serialize state itself, so the user
    /// checks this and calls [`record_snapshot`](Recorder::record_snapshot)
    /// with their own bytes.
    pub fn wants_snapshot(&self, tick: u64) -> bool {
        match self.snapshot {
            SnapshotPolicy::Off => false,
            SnapshotPolicy::Every(n) => n > 0 && tick.is_multiple_of(u64::from(n)),
        }
    }

    /// Records a serialized state snapshot at the given tick.
    pub fn record_snapshot(&mut self, tick: u64, state: &[u8]) -> Result<(), RecordError> {
        self.scratch.clear();
        push_u64(&mut self.scratch, tick);
        self.scratch.extend_from_slice(state);
        self.writer
            .write_raw_chunk(kind::SNAPSHOT, tick, &self.scratch)?;
        Ok(())
    }

    /// Records a user-placed marker, for example round start.
    pub fn record_marker(&mut self, tick: u64, label: &str) -> Result<(), RecordError> {
        self.scratch.clear();
        push_u64(&mut self.scratch, tick);
        crate::format::wire::push_str(&mut self.scratch, label)?;
        self.writer
            .write_raw_chunk(kind::MARKER, tick, &self.scratch)?;
        Ok(())
    }

    /// Flushes any partial light hash batch, writes the index and
    /// trailer, and returns the inner sink.
    pub fn finish(mut self) -> Result<W, RecordError> {
        self.flush_light_hashes()?;
        Ok(self.writer.finish(self.ticks_recorded)?)
    }

    fn flush_light_hashes(&mut self) -> Result<(), RecordError> {
        if self.light_hashes.is_empty() {
            return Ok(());
        }
        self.scratch.clear();
        push_u64(&mut self.scratch, self.batch_first_tick);
        push_u32(&mut self.scratch, self.light_hashes.len() as u32);
        for hash in &self.light_hashes {
            push_u64(&mut self.scratch, *hash);
        }
        self.writer.write_raw_chunk(
            kind::LIGHT_HASH_BATCH,
            self.batch_first_tick,
            &self.scratch,
        )?;
        self.light_hashes.clear();
        Ok(())
    }
}
