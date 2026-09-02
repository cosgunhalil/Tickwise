//! Structural diff of state dumps, the offline half of Pass 2.
//!
//! Two dumps are walked with a single merge over their sorted paths.
//! Every difference is classified, never judged, per decision #7:
//! [`DiffClass::Structural`] when a field exists on one side only, a
//! collection length differs, or a type changed; [`DiffClass::SubEpsilonFloat`]
//! when two floats differ by less than the configured epsilon; and
//! [`DiffClass::Exact`] for every other disagreement.

use crate::dump::{StateDump, Value};
use crate::format::{Chunk, FormatError, RecReader};
use std::collections::BTreeMap;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Errors produced while diffing dump files.
#[derive(Debug)]
pub enum DiffError {
    /// Reading a dump file failed.
    Format(FormatError),
    /// A file holds no state dumps at all.
    NoDumps {
        /// Which file, first or second.
        side: Side,
    },
    /// The files hold dumps, but at no common tick.
    NoCommonTicks,
}

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(err) => write!(f, "{err}"),
            Self::NoDumps { side } => write!(
                f,
                "the {side} file holds no state dumps, was it produced by a replay with \
                 dump_at_ticks set"
            ),
            Self::NoCommonTicks => write!(
                f,
                "the files hold dumps at different ticks and share none, \
                 replay both recordings with the same dump_at_ticks"
            ),
        }
    }
}

impl std::error::Error for DiffError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Format(err) => Some(err),
            _ => None,
        }
    }
}

impl From<FormatError> for DiffError {
    fn from(err: FormatError) -> Self {
        Self::Format(err)
    }
}

/// Which side of a comparison a value belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The first dump.
    First,
    /// The second dump.
    Second,
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::First => write!(f, "first"),
            Self::Second => write!(f, "second"),
        }
    }
}

/// How float differences are classified.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatPolicy {
    /// Absolute deviation at or below which two f32 values count as
    /// sub-epsilon drift rather than an exact difference.
    pub epsilon_f32: f32,
    /// Absolute deviation at or below which two f64 values count as
    /// sub-epsilon drift rather than an exact difference.
    pub epsilon_f64: f64,
}

impl Default for FloatPolicy {
    fn default() -> Self {
        Self {
            epsilon_f32: 1e-5,
            epsilon_f64: 1e-12,
        }
    }
}

impl FloatPolicy {
    /// A policy that treats every bit-level float difference as exact.
    pub fn strict() -> Self {
        Self {
            epsilon_f32: 0.0,
            epsilon_f64: 0.0,
        }
    }
}

/// The class of a difference, per decision #7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffClass {
    /// A field exists on one side only, a collection length differs, or
    /// the value type changed.
    Structural,
    /// A bit-level difference in a value.
    Exact,
    /// A float deviation at or below the configured epsilon.
    SubEpsilonFloat,
}

impl std::fmt::Display for DiffClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Structural => write!(f, "structural"),
            Self::Exact => write!(f, "exact"),
            Self::SubEpsilonFloat => write!(f, "sub-epsilon float drift"),
        }
    }
}

/// What exactly differs at a path.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    /// The path exists only on one side.
    OnlyOn {
        /// The side that has it.
        side: Side,
        /// Its value there.
        value: Value,
    },
    /// Both sides are collections of different length.
    LengthMismatch {
        /// Length on the first side.
        a: u64,
        /// Length on the second side.
        b: u64,
    },
    /// The value types differ.
    TypeMismatch {
        /// Type name on the first side.
        a: &'static str,
        /// Type name on the second side.
        b: &'static str,
    },
    /// Same type, different value.
    ValueMismatch {
        /// Value on the first side.
        a: Value,
        /// Value on the second side.
        b: Value,
    },
    /// Two floats that differ by a measured amount.
    FloatDelta {
        /// Value on the first side.
        a: Value,
        /// Value on the second side.
        b: Value,
        /// Absolute difference, computed in f64.
        delta: f64,
    },
}

/// One classified difference at one path.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    /// The field path.
    pub path: String,
    /// The classification.
    pub class: DiffClass,
    /// What differs.
    pub detail: Detail,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: ", self.path)?;
        match &self.detail {
            Detail::OnlyOn { side, value } => write!(f, "only in the {side} dump, value {value}")?,
            Detail::LengthMismatch { a, b } => write!(f, "length {a} versus {b}")?,
            Detail::TypeMismatch { a, b } => write!(f, "type {a} versus {b}")?,
            Detail::ValueMismatch { a, b } => write!(f, "{a} versus {b}")?,
            Detail::FloatDelta { a, b, delta } => write!(f, "{a} versus {b}, delta {delta:e}")?,
        }
        write!(f, ", {}", self.class)
    }
}

/// All differences between two dumps taken at one tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TickDiff {
    /// The tick both dumps were taken at.
    pub tick: u64,
    /// Differences in sorted path order.
    pub differences: Vec<Difference>,
    /// Number of paths present on both sides and compared.
    pub fields_compared: usize,
}

impl TickDiff {
    /// True when the dumps agree completely.
    pub fn is_identical(&self) -> bool {
        self.differences.is_empty()
    }

    /// Number of differences of one class.
    pub fn count(&self, class: DiffClass) -> usize {
        self.differences.iter().filter(|d| d.class == class).count()
    }
}

/// The result of diffing two dump files.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffReport {
    /// One entry per tick present in both files, ascending.
    pub ticks: Vec<TickDiff>,
    /// Dump ticks present only in the first file.
    pub only_in_a: Vec<u64>,
    /// Dump ticks present only in the second file.
    pub only_in_b: Vec<u64>,
    /// The float policy used.
    pub policy: FloatPolicy,
}

impl DiffReport {
    /// True when every common tick agrees completely.
    pub fn is_identical(&self) -> bool {
        self.ticks.iter().all(TickDiff::is_identical)
    }
}

fn float_delta(a: f64, b: f64) -> f64 {
    (a - b).abs()
}

fn classify(path: &str, a: &Value, b: &Value, policy: &FloatPolicy) -> Option<Difference> {
    let make = |class, detail| {
        Some(Difference {
            path: path.to_string(),
            class,
            detail,
        })
    };

    match (a, b) {
        (Value::Len(la), Value::Len(lb)) => {
            if la == lb {
                None
            } else {
                make(
                    DiffClass::Structural,
                    Detail::LengthMismatch { a: *la, b: *lb },
                )
            }
        }
        (Value::F32(fa), Value::F32(fb)) => {
            if fa.to_bits() == fb.to_bits() {
                return None;
            }
            let delta = float_delta(f64::from(*fa), f64::from(*fb));
            let class = if delta.is_finite() && delta <= f64::from(policy.epsilon_f32) {
                DiffClass::SubEpsilonFloat
            } else {
                DiffClass::Exact
            };
            make(
                class,
                Detail::FloatDelta {
                    a: a.clone(),
                    b: b.clone(),
                    delta,
                },
            )
        }
        (Value::F64(fa), Value::F64(fb)) => {
            if fa.to_bits() == fb.to_bits() {
                return None;
            }
            let delta = float_delta(*fa, *fb);
            let class = if delta.is_finite() && delta <= policy.epsilon_f64 {
                DiffClass::SubEpsilonFloat
            } else {
                DiffClass::Exact
            };
            make(
                class,
                Detail::FloatDelta {
                    a: a.clone(),
                    b: b.clone(),
                    delta,
                },
            )
        }
        _ if a.kind_name() != b.kind_name() => make(
            DiffClass::Structural,
            Detail::TypeMismatch {
                a: a.kind_name(),
                b: b.kind_name(),
            },
        ),
        _ if a == b => None,
        _ => make(
            DiffClass::Exact,
            Detail::ValueMismatch {
                a: a.clone(),
                b: b.clone(),
            },
        ),
    }
}

/// Diffs two dumps taken at the same tick.
pub fn diff_dumps(tick: u64, a: &StateDump, b: &StateDump, policy: &FloatPolicy) -> TickDiff {
    let mut differences = Vec::new();
    let mut fields_compared = 0;
    let mut ia = a.iter().peekable();
    let mut ib = b.iter().peekable();

    loop {
        match (ia.peek(), ib.peek()) {
            (None, None) => break,
            (Some((pa, va)), None) => {
                differences.push(only_on(pa, Side::First, va));
                ia.next();
            }
            (None, Some((pb, vb))) => {
                differences.push(only_on(pb, Side::Second, vb));
                ib.next();
            }
            (Some((pa, va)), Some((pb, vb))) => match pa.cmp(pb) {
                std::cmp::Ordering::Less => {
                    differences.push(only_on(pa, Side::First, va));
                    ia.next();
                }
                std::cmp::Ordering::Greater => {
                    differences.push(only_on(pb, Side::Second, vb));
                    ib.next();
                }
                std::cmp::Ordering::Equal => {
                    fields_compared += 1;
                    if let Some(diff) = classify(pa, va, vb, policy) {
                        differences.push(diff);
                    }
                    ia.next();
                    ib.next();
                }
            },
        }
    }

    TickDiff {
        tick,
        differences,
        fields_compared,
    }
}

fn only_on(path: &str, side: Side, value: &Value) -> Difference {
    Difference {
        path: path.to_string(),
        class: DiffClass::Structural,
        detail: Detail::OnlyOn {
            side,
            value: value.clone(),
        },
    }
}

fn load_dumps<R: Read + Seek>(
    reader: &mut RecReader<R>,
    side: Side,
) -> Result<BTreeMap<u64, StateDump>, DiffError> {
    let mut dumps = BTreeMap::new();
    for item in reader.chunks()? {
        if let Chunk::StateDump { tick, dump } = item? {
            dumps.insert(tick, dump);
        }
    }
    if dumps.is_empty() {
        return Err(DiffError::NoDumps { side });
    }
    Ok(dumps)
}

/// Diffs two `.dump` files, pairing dumps by tick.
pub fn structural<A: AsRef<Path>, B: AsRef<Path>>(
    a: A,
    b: B,
    policy: FloatPolicy,
) -> Result<DiffReport, DiffError> {
    let mut reader_a = RecReader::open(BufReader::new(
        std::fs::File::open(a).map_err(FormatError::from)?,
    ))?;
    let mut reader_b = RecReader::open(BufReader::new(
        std::fs::File::open(b).map_err(FormatError::from)?,
    ))?;
    structural_from(&mut reader_a, &mut reader_b, policy)
}

/// Diffs two already opened dump files, pairing dumps by tick.
pub fn structural_from<Ra: Read + Seek, Rb: Read + Seek>(
    a: &mut RecReader<Ra>,
    b: &mut RecReader<Rb>,
    policy: FloatPolicy,
) -> Result<DiffReport, DiffError> {
    let dumps_a = load_dumps(a, Side::First)?;
    let dumps_b = load_dumps(b, Side::Second)?;

    let mut ticks = Vec::new();
    let mut only_in_a = Vec::new();
    for (tick, dump_a) in &dumps_a {
        match dumps_b.get(tick) {
            Some(dump_b) => ticks.push(diff_dumps(*tick, dump_a, dump_b, &policy)),
            None => only_in_a.push(*tick),
        }
    }
    let only_in_b: Vec<u64> = dumps_b
        .keys()
        .filter(|tick| !dumps_a.contains_key(tick))
        .copied()
        .collect();

    if ticks.is_empty() {
        return Err(DiffError::NoCommonTicks);
    }

    Ok(DiffReport {
        ticks,
        only_in_a,
        only_in_b,
        policy,
    })
}
