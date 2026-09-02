//! Structural state dumps, decision #13.
//!
//! A [`StateDump`] is a flat, sorted list of field paths mapped to typed
//! values, for example `players[2].velocity.x` mapped to `F32(3.5)`.
//! Collections carry an explicit [`Value::Len`] entry at their own path,
//! so a length difference shows up as a structural difference. The diff
//! engine walks two dumps with a single merge over their sorted paths.
//!
//! Building a dump is a sequence of insert calls, which is exactly the
//! shape an FFI bridge can drive without any tree-building API.

use crate::format::FormatError;
use crate::format::wire::{SliceReader, push_str, push_u32, push_u64};
use std::collections::BTreeMap;

/// One typed value in a dump.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An absent value, for example `Option::None`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    I64(i64),
    /// An unsigned integer.
    U64(u64),
    /// A single precision float, compared bit for bit or by epsilon.
    F32(f32),
    /// A double precision float, compared bit for bit or by epsilon.
    F64(f64),
    /// A string.
    Str(String),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// The length of a collection, stored at the collection's own path.
    Len(u64),
}

impl Value {
    /// Returns a short name for the variant, for reports.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::I64(_) => "i64",
            Self::U64(_) => "u64",
            Self::F32(_) => "f32",
            Self::F64(_) => "f64",
            Self::Str(_) => "str",
            Self::Bytes(_) => "bytes",
            Self::Len(_) => "len",
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
            Self::U64(v) => write!(f, "{v}"),
            Self::F32(v) => write!(f, "{v:?}"),
            Self::F64(v) => write!(f, "{v:?}"),
            Self::Str(v) => write!(f, "{v:?}"),
            Self::Bytes(v) => write!(f, "{} bytes", v.len()),
            Self::Len(v) => write!(f, "len {v}"),
        }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::I64(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::I64(i64::from(v))
    }
}
impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Self::U64(v)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Self::U64(u64::from(v))
    }
}
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Self::F32(v)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::F64(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}
impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}

/// A structural, diffable representation of simulation state.
///
/// # Examples
///
/// ```
/// use tickwise::{StateDump, Value};
///
/// let mut dump = StateDump::empty();
/// dump.insert("tick", 4021u64);
/// dump.insert("players", Value::Len(2));
/// dump.insert("players[0].velocity.x", 3.5f32);
/// dump.insert("players[1].velocity.x", -1.25f32);
///
/// assert_eq!(dump.len(), 4);
/// assert_eq!(dump.get("players"), Some(&Value::Len(2)));
/// // Iteration is always in sorted path order.
/// let first = dump.iter().next().map(|(path, _)| path);
/// assert_eq!(first, Some("players"));
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateDump {
    entries: BTreeMap<String, Value>,
}

impl StateDump {
    /// Creates an empty dump.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Inserts a value at a path, replacing and returning any previous
    /// value at that path.
    pub fn insert<P: Into<String>, V: Into<Value>>(&mut self, path: P, value: V) -> Option<Value> {
        self.entries.insert(path.into(), value.into())
    }

    /// Returns the value at a path.
    pub fn get(&self, path: &str) -> Option<&Value> {
        self.entries.get(path)
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true when the dump holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates entries in sorted path order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
}

const TAG_NULL: u8 = 0;
const TAG_BOOL: u8 = 1;
const TAG_I64: u8 = 2;
const TAG_U64: u8 = 3;
const TAG_F32: u8 = 4;
const TAG_F64: u8 = 5;
const TAG_STR: u8 = 6;
const TAG_BYTES: u8 = 7;
const TAG_LEN: u8 = 8;

fn push_blob(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), FormatError> {
    let len = u32::try_from(bytes.len()).map_err(|_| FormatError::TooLarge)?;
    push_u32(out, len);
    out.extend_from_slice(bytes);
    Ok(())
}

pub(crate) fn encode_dump(dump: &StateDump, out: &mut Vec<u8>) -> Result<(), FormatError> {
    let count = u32::try_from(dump.entries.len()).map_err(|_| FormatError::TooLarge)?;
    push_u32(out, count);
    for (path, value) in &dump.entries {
        push_str(out, path)?;
        match value {
            Value::Null => out.push(TAG_NULL),
            Value::Bool(v) => {
                out.push(TAG_BOOL);
                out.push(u8::from(*v));
            }
            Value::I64(v) => {
                out.push(TAG_I64);
                push_u64(out, *v as u64);
            }
            Value::U64(v) => {
                out.push(TAG_U64);
                push_u64(out, *v);
            }
            Value::F32(v) => {
                out.push(TAG_F32);
                push_u32(out, v.to_bits());
            }
            Value::F64(v) => {
                out.push(TAG_F64);
                push_u64(out, v.to_bits());
            }
            Value::Str(v) => {
                out.push(TAG_STR);
                push_blob(out, v.as_bytes())?;
            }
            Value::Bytes(v) => {
                out.push(TAG_BYTES);
                push_blob(out, v)?;
            }
            Value::Len(v) => {
                out.push(TAG_LEN);
                push_u64(out, *v);
            }
        }
    }
    Ok(())
}

pub(crate) fn decode_dump(reader: &mut SliceReader<'_>) -> Result<StateDump, FormatError> {
    let count = reader.u32()? as usize;
    let mut entries = BTreeMap::new();
    let mut previous: Option<String> = None;

    for _ in 0..count {
        let path = reader.str()?;
        // The encoding is canonical: strictly ascending paths, no
        // duplicates. Anything else is corruption, not a valid dump.
        if let Some(prev) = &previous
            && prev.as_str() >= path.as_str()
        {
            return Err(FormatError::Corrupt("dump entries are not in sorted order"));
        }

        let tag = reader.take(1)?[0];
        let value = match tag {
            TAG_NULL => Value::Null,
            TAG_BOOL => match reader.take(1)?[0] {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                _ => return Err(FormatError::Corrupt("bool value is not 0 or 1")),
            },
            TAG_I64 => Value::I64(reader.u64()? as i64),
            TAG_U64 => Value::U64(reader.u64()?),
            TAG_F32 => Value::F32(f32::from_bits(reader.u32()?)),
            TAG_F64 => Value::F64(f64::from_bits(reader.u64()?)),
            TAG_STR => {
                let len = reader.u32()? as usize;
                let bytes = reader.take(len)?;
                Value::Str(String::from_utf8(bytes.to_vec()).map_err(|_| FormatError::InvalidUtf8)?)
            }
            TAG_BYTES => {
                let len = reader.u32()? as usize;
                Value::Bytes(reader.take(len)?.to_vec())
            }
            TAG_LEN => Value::Len(reader.u64()?),
            _ => return Err(FormatError::Corrupt("unknown dump value tag")),
        };

        entries.insert(path.clone(), value);
        previous = Some(path);
    }

    Ok(StateDump { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StateDump {
        let mut dump = StateDump::empty();
        dump.insert("tick", 4021u64);
        dump.insert("score", 77i64);
        dump.insert("alive", true);
        dump.insert("name", "refsim");
        dump.insert("blob", vec![1u8, 2, 3]);
        dump.insert("maybe", Value::Null);
        dump.insert("balls", Value::Len(2));
        dump.insert("balls[0].x", 1.5f32);
        dump.insert("balls[1].x", -2.25f32);
        dump.insert("precise", 0.1f64);
        dump
    }

    #[test]
    fn round_trips_every_value_kind() {
        let dump = sample();
        let mut bytes = Vec::new();
        encode_dump(&dump, &mut bytes).unwrap();
        let mut reader = SliceReader::new(&bytes);
        let decoded = decode_dump(&mut reader).unwrap();
        assert_eq!(decoded, dump);
        assert!(reader.is_done());
    }

    #[test]
    fn encoding_is_deterministic_regardless_of_insert_order() {
        let mut a = StateDump::empty();
        a.insert("b", 1u64);
        a.insert("a", 2u64);
        let mut b = StateDump::empty();
        b.insert("a", 2u64);
        b.insert("b", 1u64);
        let mut ea = Vec::new();
        let mut eb = Vec::new();
        encode_dump(&a, &mut ea).unwrap();
        encode_dump(&b, &mut eb).unwrap();
        assert_eq!(ea, eb);
    }

    #[test]
    fn unsorted_or_duplicate_entries_are_rejected() {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, 2);
        push_str(&mut bytes, "b").unwrap();
        bytes.push(TAG_NULL);
        push_str(&mut bytes, "a").unwrap();
        bytes.push(TAG_NULL);
        assert!(decode_dump(&mut SliceReader::new(&bytes)).is_err());

        let mut dup = Vec::new();
        push_u32(&mut dup, 2);
        push_str(&mut dup, "a").unwrap();
        dup.push(TAG_NULL);
        push_str(&mut dup, "a").unwrap();
        dup.push(TAG_NULL);
        assert!(decode_dump(&mut SliceReader::new(&dup)).is_err());
    }

    #[test]
    fn truncations_and_bad_tags_never_panic() {
        let mut bytes = Vec::new();
        encode_dump(&sample(), &mut bytes).unwrap();
        for len in 0..bytes.len() {
            assert!(decode_dump(&mut SliceReader::new(&bytes[..len])).is_err());
        }

        let mut bad_tag = Vec::new();
        push_u32(&mut bad_tag, 1);
        push_str(&mut bad_tag, "x").unwrap();
        bad_tag.push(0xFF);
        assert!(decode_dump(&mut SliceReader::new(&bad_tag)).is_err());
    }

    #[test]
    fn insert_replaces_and_returns_previous() {
        let mut dump = StateDump::empty();
        assert_eq!(dump.insert("x", 1u64), None);
        assert_eq!(dump.insert("x", 2u64), Some(Value::U64(1)));
        assert_eq!(dump.get("x"), Some(&Value::U64(2)));
    }
}
