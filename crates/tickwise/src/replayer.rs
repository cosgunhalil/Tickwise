//! The replay API, Pass 2 of the two-pass workflow.
//!
//! The replayer feeds recorded inputs back to the user's own simulation
//! loop, verifies live hashes against the recording as it goes, and
//! captures structural state dumps at the requested ticks. It never runs
//! the simulation itself.
//!
//! Snapshot resume follows decision #14: the replayer only locates
//! snapshots, the user restores their own state from the bytes, then
//! seeks the replayer to the tick after the snapshot.

use crate::compare::HashKind;
use crate::dump::StateDump;
use crate::format::{Chunk, FormatError, Header, RecReader, RecWriter};
use crate::probe::DeterminismProbe;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::Path;

/// Errors produced while replaying.
#[derive(Debug)]
pub enum ReplayError {
    /// Reading the recording or writing the dump failed.
    Format(FormatError),
    /// The recording declares a different input encoding than the build
    /// expects, decision #11. Feeding these inputs to the simulation would
    /// silently produce garbage.
    InputFormatMismatch {
        /// Input format id stored in the recording.
        recorded: u64,
        /// Input format id the caller expects.
        expected: u64,
    },
    /// A live hash disagrees with the recorded one: the replay itself is
    /// not reproducing the recorded session.
    HashMismatch {
        /// Tick of the disagreement.
        tick: u64,
        /// Which hash stream disagreed.
        kind: HashKind,
        /// Hash stored in the recording.
        recorded: u64,
        /// Hash the probe produced during replay.
        actual: u64,
    },
    /// A requested tick lies outside the recording.
    TickOutOfRange {
        /// The offending tick.
        tick: u64,
        /// First tick in the recording.
        first: u64,
        /// Last tick in the recording.
        last: u64,
    },
    /// The recording holds no ticks at all.
    EmptyRecording,
    /// `after_tick` was called without a preceding `next_step`.
    NoPendingStep,
    /// `next_step` was called again before `after_tick` for this tick.
    StepSkipped {
        /// The tick whose `after_tick` never happened.
        tick: u64,
    },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(err) => write!(f, "{err}"),
            Self::InputFormatMismatch { recorded, expected } => write!(
                f,
                "input format mismatch: the recording declares id {recorded} but this build \
                 expects id {expected}, replaying would feed misinterpreted inputs"
            ),
            Self::HashMismatch {
                tick,
                kind,
                recorded,
                actual,
            } => write!(
                f,
                "replay diverged from the recording at tick {tick}: {} hash recorded \
                 {recorded:016x}, replay produced {actual:016x}. Your simulation is not \
                 reproducing the session, run the self-check before hunting cross-client desyncs",
                match kind {
                    HashKind::Light => "light",
                    HashKind::Full => "full",
                }
            ),
            Self::TickOutOfRange { tick, first, last } => write!(
                f,
                "tick {tick} is outside the recording, which covers ticks {first} to {last}"
            ),
            Self::EmptyRecording => write!(f, "the recording holds no ticks"),
            Self::NoPendingStep => write!(
                f,
                "after_tick called without a pending step, call next_step first"
            ),
            Self::StepSkipped { tick } => write!(
                f,
                "next_step was called again before after_tick for tick {tick}, \
                 every step needs exactly one after_tick"
            ),
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format(err) => Some(err),
            _ => None,
        }
    }
}

impl From<FormatError> for ReplayError {
    fn from(err: FormatError) -> Self {
        Self::Format(err)
    }
}

/// Configuration for a replay session.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplayConfig {
    /// Ticks at which to capture a state dump.
    pub dump_at_ticks: Vec<u64>,
    /// Compare live hashes against the recording after every tick.
    pub verify_hashes: bool,
    /// When set, opening fails unless the recording declares this input
    /// format id, decision #11.
    pub expected_input_format_id: Option<u64>,
}

/// One tick's worth of replay: the tick number and its recorded inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step<'a> {
    tick: u64,
    inputs: &'a [u8],
}

impl Step<'_> {
    /// The tick to simulate.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// The opaque input bytes recorded for this tick.
    pub fn inputs(&self) -> &[u8] {
        self.inputs
    }
}

/// Replays a recording through the user's simulation loop.
///
/// # Examples
///
/// ```
/// use tickwise::{DeterminismProbe, Recorder, RecorderConfig, StateDump};
/// use tickwise::replayer::{ReplayConfig, ReplayError, Replayer};
/// use std::io::Cursor;
///
/// struct Sim { frame: u64 }
///
/// impl DeterminismProbe for Sim {
///     fn light_hash(&self) -> u64 { self.frame }
///     fn full_hash(&self) -> u64 { self.frame * 31 }
///     fn state_dump(&self) -> StateDump {
///         let mut dump = StateDump::empty();
///         dump.insert("frame", self.frame);
///         dump
///     }
/// }
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Pass 1: record a session.
///     let mut sim = Sim { frame: 0 };
///     let mut rec = Recorder::new(Vec::new(), RecorderConfig::default())?;
///     for tick in 0..100 {
///         sim.frame += 1;
///         rec.record_tick(tick, &[], &sim)?;
///     }
///     let recording = rec.finish()?;
///
///     // Pass 2: replay it and dump the state at tick 42.
///     let mut reader = tickwise::format::RecReader::open(Cursor::new(&recording))?;
///     let mut rep = Replayer::from_reader(&mut reader, ReplayConfig {
///         dump_at_ticks: vec![42],
///         verify_hashes: true,
///         ..ReplayConfig::default()
///     })?;
///     let mut sim = Sim { frame: 0 };
///     while let Some(step) = rep.next_step() {
///         let _inputs = step.inputs(); // apply to your simulation
///         sim.frame += 1;
///         rep.after_tick(&sim)?;
///     }
///     let dumps = rep.into_dumps()?;
///     assert_eq!(dumps.len(), 1);
///     assert_eq!(dumps[0].0, 42);
///     Ok(())
/// }
/// ```
pub struct Replayer {
    header: Header,
    tick_count: u64,
    frames: Vec<(u64, Vec<u8>)>,
    light: BTreeMap<u64, u64>,
    full: BTreeMap<u64, u64>,
    snapshots: Vec<(u64, Vec<u8>)>,
    first_tick: u64,
    last_tick: u64,
    next_tick: u64,
    frame_index: usize,
    pending: Option<u64>,
    skipped: Option<u64>,
    dump_ticks: BTreeSet<u64>,
    verify_hashes: bool,
    dumps: Vec<(u64, StateDump)>,
}

impl Replayer {
    /// Opens a `.rec` file for replay.
    pub fn open<P: AsRef<Path>>(path: P, config: ReplayConfig) -> Result<Self, ReplayError> {
        let file = std::fs::File::open(path).map_err(FormatError::from)?;
        let mut reader = RecReader::open(BufReader::new(file))?;
        Self::from_reader(&mut reader, config)
    }

    /// Builds a replayer from an already opened recording.
    pub fn from_reader<R: Read + Seek>(
        reader: &mut RecReader<R>,
        config: ReplayConfig,
    ) -> Result<Self, ReplayError> {
        let header = reader.header().clone();
        if let Some(expected) = config.expected_input_format_id
            && header.config.input_format_id != expected
        {
            return Err(ReplayError::InputFormatMismatch {
                recorded: header.config.input_format_id,
                expected,
            });
        }

        let mut frames = Vec::new();
        let mut light = BTreeMap::new();
        let mut full = BTreeMap::new();
        let mut snapshots = Vec::new();
        for item in reader.chunks()? {
            match item? {
                Chunk::InputFrame { tick, data } => frames.push((tick, data)),
                Chunk::LightHashBatch { first_tick, hashes } => {
                    for (offset, hash) in hashes.iter().enumerate() {
                        light.insert(first_tick + offset as u64, *hash);
                    }
                }
                Chunk::FullHash { tick, hash } => {
                    full.insert(tick, hash);
                }
                Chunk::Snapshot { tick, data } => snapshots.push((tick, data)),
                _ => {}
            }
        }
        frames.sort_by_key(|(tick, _)| *tick);
        snapshots.sort_by_key(|(tick, _)| *tick);

        let (first_tick, last_tick) = match (light.keys().next(), light.keys().next_back()) {
            (Some(first), Some(last)) => (*first, *last),
            _ => return Err(ReplayError::EmptyRecording),
        };

        let dump_ticks: BTreeSet<u64> = config.dump_at_ticks.iter().copied().collect();
        for tick in &dump_ticks {
            if *tick < first_tick || *tick > last_tick {
                return Err(ReplayError::TickOutOfRange {
                    tick: *tick,
                    first: first_tick,
                    last: last_tick,
                });
            }
        }

        Ok(Self {
            header,
            tick_count: reader.tick_count(),
            frames,
            light,
            full,
            snapshots,
            first_tick,
            last_tick,
            next_tick: first_tick,
            frame_index: 0,
            pending: None,
            skipped: None,
            dump_ticks,
            verify_hashes: config.verify_hashes,
            dumps: Vec::new(),
        })
    }

    /// Returns the recording's header.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Returns the first and last tick the recording covers.
    pub fn tick_range(&self) -> (u64, u64) {
        (self.first_tick, self.last_tick)
    }

    /// Returns the tick the next call to `next_step` will yield, or
    /// `None` when the replay is complete.
    pub fn upcoming_tick(&self) -> Option<u64> {
        (self.next_tick <= self.last_tick).then_some(self.next_tick)
    }

    /// Returns the ticks that carry snapshots, in ascending order.
    pub fn snapshot_ticks(&self) -> Vec<u64> {
        self.snapshots.iter().map(|(tick, _)| *tick).collect()
    }

    /// Returns the latest snapshot at or before the given tick.
    ///
    /// A snapshot at tick T holds the state after tick T completed.
    /// Restore your simulation from the bytes, then call
    /// [`seek_to`](Replayer::seek_to) with T + 1.
    pub fn nearest_snapshot_before(&self, tick: u64) -> Option<(u64, &[u8])> {
        self.snapshots
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .map(|(t, data)| (*t, data.as_slice()))
    }

    /// Positions the replay so the next step is the given tick. Used
    /// after restoring state from a snapshot.
    pub fn seek_to(&mut self, tick: u64) -> Result<(), ReplayError> {
        if tick < self.first_tick || tick > self.last_tick + 1 {
            return Err(ReplayError::TickOutOfRange {
                tick,
                first: self.first_tick,
                last: self.last_tick,
            });
        }
        self.next_tick = tick;
        self.frame_index = 0;
        self.pending = None;
        Ok(())
    }

    /// Yields the next tick to simulate and its inputs, or `None` when
    /// the recording is exhausted. Call [`after_tick`](Replayer::after_tick)
    /// exactly once after simulating each step.
    pub fn next_step(&mut self) -> Option<Step<'_>> {
        if self.next_tick > self.last_tick {
            return None;
        }
        if let Some(pending) = self.pending
            && self.skipped.is_none()
        {
            self.skipped = Some(pending);
        }

        let tick = self.next_tick;
        while self.frame_index + 1 < self.frames.len()
            && self.frames[self.frame_index + 1].0 <= tick
        {
            self.frame_index += 1;
        }
        let inputs = match self.frames.get(self.frame_index) {
            Some((frame_tick, data)) if *frame_tick <= tick => data.as_slice(),
            _ => &[],
        };

        self.pending = Some(tick);
        self.next_tick = tick + 1;
        Some(Step { tick, inputs })
    }

    /// Captures a dump if this tick was requested, then verifies the live
    /// hashes against the recording when verification is enabled.
    pub fn after_tick(&mut self, probe: &dyn DeterminismProbe) -> Result<(), ReplayError> {
        let tick = self.pending.take().ok_or(ReplayError::NoPendingStep)?;

        // Dump before verifying, so a divergence at the target tick still
        // leaves the dump behind for inspection.
        if self.dump_ticks.contains(&tick) {
            self.dumps.push((tick, probe.state_dump()));
        }

        if self.verify_hashes {
            if let Some(recorded) = self.light.get(&tick) {
                let actual = probe.light_hash();
                if actual != *recorded {
                    return Err(ReplayError::HashMismatch {
                        tick,
                        kind: HashKind::Light,
                        recorded: *recorded,
                        actual,
                    });
                }
            }
            if let Some(recorded) = self.full.get(&tick) {
                let actual = probe.full_hash();
                if actual != *recorded {
                    return Err(ReplayError::HashMismatch {
                        tick,
                        kind: HashKind::Full,
                        recorded: *recorded,
                        actual,
                    });
                }
            }
        }
        Ok(())
    }

    fn check_protocol(&self) -> Result<(), ReplayError> {
        if let Some(tick) = self.skipped.or(self.pending) {
            return Err(ReplayError::StepSkipped { tick });
        }
        Ok(())
    }

    /// Returns the captured dumps in tick order without writing a file.
    pub fn into_dumps(self) -> Result<Vec<(u64, StateDump)>, ReplayError> {
        self.check_protocol()?;
        Ok(self.dumps)
    }

    /// Writes the captured dumps as a `.dump` file at the given path.
    pub fn finish<P: AsRef<Path>>(self, path: P) -> Result<(), ReplayError> {
        let file = std::fs::File::create(path).map_err(FormatError::from)?;
        self.finish_into(BufWriter::new(file))?;
        Ok(())
    }

    /// Writes the captured dumps as `.dump` content into any sink and
    /// returns the sink.
    pub fn finish_into<W: Write>(self, sink: W) -> Result<W, ReplayError> {
        self.check_protocol()?;
        let mut writer = RecWriter::new(sink, &self.header)?;
        for (tick, dump) in &self.dumps {
            writer.write_chunk(&Chunk::StateDump {
                tick: *tick,
                dump: dump.clone(),
            })?;
        }
        Ok(writer.finish(self.tick_count)?)
    }
}
