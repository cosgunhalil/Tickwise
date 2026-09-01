//! First-divergence search over two recordings, the offline half of
//! Pass 1.
//!
//! Both recordings are reduced to their hash timelines, then compared
//! over the tick range they share. The first tick where hashes disagree
//! is the answer the whole workflow exists to produce.

use crate::format::{Chunk, FormatError, RecReader};
use std::collections::BTreeMap;
use std::io::{Read, Seek};
use std::path::Path;

/// Errors produced while comparing two recordings.
#[derive(Debug)]
pub enum CompareError {
    /// Reading one of the recordings failed.
    Format(FormatError),
    /// The recordings hash with different algorithms, so their hashes
    /// cannot be compared at all.
    HashAlgoMismatch {
        /// Algorithm id of the first recording.
        a: u16,
        /// Algorithm id of the second recording.
        b: u16,
    },
    /// The recordings share no ticks with hashes on both sides.
    NoCommonTicks,
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(err) => write!(f, "{err}"),
            Self::HashAlgoMismatch { a, b } => write!(
                f,
                "hash algorithm mismatch, id {a} versus id {b}, \
                 these recordings cannot be compared"
            ),
            Self::NoCommonTicks => write!(
                f,
                "the recordings share no ticks with hashes on both sides, \
                 nothing to compare"
            ),
        }
    }
}

impl std::error::Error for CompareError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format(err) => Some(err),
            _ => None,
        }
    }
}

impl From<FormatError> for CompareError {
    fn from(err: FormatError) -> Self {
        Self::Format(err)
    }
}

/// Which hash stream detected a divergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashKind {
    /// The per-tick light hash.
    Light,
    /// The periodic full hash.
    Full,
}

/// A found divergence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// First compared tick whose hashes disagree.
    pub tick: u64,
    /// Which hash stream caught it.
    ///
    /// [`HashKind::Full`] here means the light hash agreed while the full
    /// hash did not: the light hash has a blind spot, and the real
    /// divergence happened at or before this tick.
    pub detected_by: HashKind,
    /// The last compared tick where both sides still agreed.
    pub last_agreeing_tick: Option<u64>,
    /// First full hash mismatch at or after the divergent tick, when one
    /// exists. Confirms the light hash finding with full state coverage.
    pub confirming_full_hash_tick: Option<u64>,
}

/// The comparison verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every compared tick agreed.
    Identical {
        /// Number of ticks compared on both sides.
        ticks_compared: u64,
        /// Ticks only the first recording covers.
        extra_ticks_a: u64,
        /// Ticks only the second recording covers.
        extra_ticks_b: u64,
    },
    /// The recordings diverge.
    Diverged(Divergence),
}

/// Metadata differences worth telling the user about. None of these stop
/// the comparison, but each one can explain a divergence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareWarning {
    /// The recordings started from different RNG seeds.
    SeedMismatch(u64, u64),
    /// The recordings ran at different tick rates.
    TickRateMismatch(u32, u32),
    /// The recordings declare different input encodings.
    InputFormatMismatch(u64, u64),
    /// The recordings come from different builds.
    BuildMismatch(String, String),
}

impl std::fmt::Display for CompareWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SeedMismatch(a, b) => write!(
                f,
                "rng seeds differ, {a:#x} versus {b:#x}, \
                 different seeds usually mean genuinely different sessions"
            ),
            Self::TickRateMismatch(a, b) => {
                write!(f, "tick rates differ, {a} versus {b} ticks per second")
            }
            Self::InputFormatMismatch(a, b) => write!(
                f,
                "input format ids differ, {a} versus {b}, \
                 the inputs may not mean the same thing"
            ),
            Self::BuildMismatch(a, b) => write!(f, "builds differ, {a} versus {b}"),
        }
    }
}

/// The full comparison result: a verdict plus metadata warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareReport {
    /// The verdict.
    pub outcome: Outcome,
    /// Metadata differences found along the way.
    pub warnings: Vec<CompareWarning>,
}

impl std::fmt::Display for CompareReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.outcome {
            Outcome::Identical {
                ticks_compared,
                extra_ticks_a,
                extra_ticks_b,
            } => {
                write!(f, "identical over {ticks_compared} compared ticks")?;
                if *extra_ticks_a > 0 || *extra_ticks_b > 0 {
                    write!(
                        f,
                        ", though coverage differs: {extra_ticks_a} ticks only in the first \
                         recording, {extra_ticks_b} only in the second"
                    )?;
                }
                Ok(())
            }
            Outcome::Diverged(d) => {
                match d.detected_by {
                    HashKind::Light => {
                        write!(
                            f,
                            "first divergence at tick {}, caught by the light hash",
                            d.tick
                        )?;
                        match d.confirming_full_hash_tick {
                            Some(t) => write!(f, ", confirmed by the full hash at tick {t}")?,
                            None => write!(f, ", no full hash available to confirm it")?,
                        }
                    }
                    HashKind::Full => {
                        write!(
                            f,
                            "divergence caught by the full hash at tick {}, while the light \
                             hash saw nothing: the light hash has a blind spot, and the real \
                             divergence happened at or before this tick",
                            d.tick
                        )?;
                    }
                }
                if let Some(t) = d.last_agreeing_tick {
                    write!(f, ", last agreement at tick {t}")?;
                }
                Ok(())
            }
        }
    }
}

/// One recording reduced to its comparable content.
struct HashTimeline {
    light: BTreeMap<u64, u64>,
    full: BTreeMap<u64, u64>,
    hash_algo_id: u16,
    rng_seed: u64,
    tick_rate: u32,
    input_format_id: u64,
    build_hash: String,
}

fn load_timeline<R: Read + Seek>(reader: &mut RecReader<R>) -> Result<HashTimeline, CompareError> {
    let header = reader.header().clone();
    let mut light = BTreeMap::new();
    let mut full = BTreeMap::new();

    for item in reader.chunks()? {
        match item? {
            Chunk::LightHashBatch { first_tick, hashes } => {
                for (offset, hash) in hashes.iter().enumerate() {
                    light.insert(first_tick + offset as u64, *hash);
                }
            }
            Chunk::FullHash { tick, hash } => {
                full.insert(tick, hash);
            }
            _ => {}
        }
    }

    Ok(HashTimeline {
        light,
        full,
        hash_algo_id: header.config.hash_algo_id,
        rng_seed: header.meta.rng_seed,
        tick_rate: header.meta.tick_rate,
        input_format_id: header.config.input_format_id,
        build_hash: header.meta.build_hash,
    })
}

/// Finds the first divergent tick between two `.rec` files.
pub fn first_divergence<A: AsRef<Path>, B: AsRef<Path>>(
    a: A,
    b: B,
) -> Result<CompareReport, CompareError> {
    let mut reader_a = RecReader::open(std::io::BufReader::new(
        std::fs::File::open(a).map_err(FormatError::from)?,
    ))?;
    let mut reader_b = RecReader::open(std::io::BufReader::new(
        std::fs::File::open(b).map_err(FormatError::from)?,
    ))?;
    first_divergence_from(&mut reader_a, &mut reader_b)
}

/// Finds the first divergent tick between two already opened recordings.
pub fn first_divergence_from<Ra: Read + Seek, Rb: Read + Seek>(
    a: &mut RecReader<Ra>,
    b: &mut RecReader<Rb>,
) -> Result<CompareReport, CompareError> {
    let ta = load_timeline(a)?;
    let tb = load_timeline(b)?;

    if ta.hash_algo_id != tb.hash_algo_id {
        return Err(CompareError::HashAlgoMismatch {
            a: ta.hash_algo_id,
            b: tb.hash_algo_id,
        });
    }

    let mut warnings = Vec::new();
    if ta.rng_seed != tb.rng_seed {
        warnings.push(CompareWarning::SeedMismatch(ta.rng_seed, tb.rng_seed));
    }
    if ta.tick_rate != tb.tick_rate {
        warnings.push(CompareWarning::TickRateMismatch(ta.tick_rate, tb.tick_rate));
    }
    if ta.input_format_id != tb.input_format_id {
        warnings.push(CompareWarning::InputFormatMismatch(
            ta.input_format_id,
            tb.input_format_id,
        ));
    }
    if ta.build_hash != tb.build_hash {
        warnings.push(CompareWarning::BuildMismatch(
            ta.build_hash.clone(),
            tb.build_hash.clone(),
        ));
    }

    // First light hash disagreement over the common ticks, walking in
    // ascending tick order.
    let mut ticks_compared: u64 = 0;
    let mut last_agreeing: Option<u64> = None;
    let mut light_divergence: Option<u64> = None;
    for (tick, hash_a) in &ta.light {
        if let Some(hash_b) = tb.light.get(tick) {
            ticks_compared += 1;
            if hash_a == hash_b {
                last_agreeing = Some(*tick);
            } else {
                light_divergence = Some(*tick);
                break;
            }
        }
    }

    // First full hash disagreement, independently: it can fire earlier
    // than the light stream when the light hash has a blind spot. For a
    // full-detected divergence, the honest last agreement is the last
    // agreeing full hash, since light agreement is exactly what cannot
    // be trusted in that case.
    let mut full_divergence: Option<u64> = None;
    let mut last_full_agree: Option<u64> = None;
    for (tick, hash_a) in &ta.full {
        if let Some(hash_b) = tb.full.get(tick) {
            if hash_a != hash_b {
                full_divergence = Some(*tick);
                break;
            }
            last_full_agree = Some(*tick);
        }
    }

    if ticks_compared == 0 && full_divergence.is_none() {
        return Err(CompareError::NoCommonTicks);
    }

    let outcome = match (light_divergence, full_divergence) {
        (None, None) => {
            let common = ticks_compared;
            Outcome::Identical {
                ticks_compared: common,
                extra_ticks_a: ta.light.len() as u64 - common,
                extra_ticks_b: tb.light.len() as u64 - common,
            }
        }
        (Some(lt), full) => {
            let confirming = full.filter(|ft| *ft >= lt);
            if let Some(ft) = full
                && ft < lt
            {
                // The full hash fired before the light stream noticed:
                // report the earlier, stronger signal.
                Outcome::Diverged(Divergence {
                    tick: ft,
                    detected_by: HashKind::Full,
                    last_agreeing_tick: last_full_agree,
                    confirming_full_hash_tick: Some(ft),
                })
            } else {
                Outcome::Diverged(Divergence {
                    tick: lt,
                    detected_by: HashKind::Light,
                    last_agreeing_tick: last_agreeing,
                    confirming_full_hash_tick: confirming,
                })
            }
        }
        (None, Some(ft)) => Outcome::Diverged(Divergence {
            tick: ft,
            detected_by: HashKind::Full,
            last_agreeing_tick: last_full_agree,
            confirming_full_hash_tick: Some(ft),
        }),
    };

    Ok(CompareReport { outcome, warnings })
}
