//! The reflection walk: turns any `Reflect` value into a flat list of
//! path and value pairs, the shape Tickwise hashes and diffs.

use bevy_reflect::{PartialReflect, ReflectRef};
use tickwise::Value;

/// Receives each leaf the walk produces.
pub(crate) trait Sink {
    fn leaf(&mut self, path: &str, value: Value);
}

/// FNV-1a over path bytes and value bits, so the hash depends both on
/// what the values are and on where they live. A field moving from one
/// path to another changes the hash, which is the point.
pub(crate) struct Hasher(u64);

impl Hasher {
    pub(crate) fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }
}

impl Sink for Hasher {
    fn leaf(&mut self, path: &str, value: Value) {
        self.write(path.as_bytes());
        match value {
            Value::Null => self.write(&[0]),
            Value::Bool(v) => self.write(&[1, u8::from(v)]),
            Value::I64(v) => self.write(&v.to_le_bytes()),
            Value::U64(v) | Value::Len(v) => self.write(&v.to_le_bytes()),
            Value::F32(v) => self.write(&v.to_bits().to_le_bytes()),
            Value::F64(v) => self.write(&v.to_bits().to_le_bytes()),
            Value::Str(v) => self.write(v.as_bytes()),
            Value::Bytes(v) => self.write(&v),
        }
    }
}

impl Sink for tickwise::StateDump {
    fn leaf(&mut self, path: &str, value: Value) {
        self.insert(path, value);
    }
}

/// Appends a child name to a path, adding the dot only when needed.
pub(crate) fn push_child(path: &mut String, name: &str) {
    if !path.is_empty() {
        path.push('.');
    }
    path.push_str(name);
}

/// Appends an index or key in brackets.
fn push_index(path: &mut String, index: &str) {
    path.push('[');
    path.push_str(index);
    path.push(']');
}

/// Reads a leaf value out of an opaque type, meaning one reflection does
/// not decompose further, by trying every primitive Tickwise can store.
///
/// A type that matches nothing is recorded as its type path rather than
/// silently skipped, so a gap in coverage is visible in the dump instead
/// of hiding a desync.
fn opaque_value(value: &dyn PartialReflect) -> Value {
    macro_rules! try_int {
        ($($ty:ty => $variant:ident),* $(,)?) => {
            $(if let Some(v) = value.try_downcast_ref::<$ty>() {
                return Value::$variant((*v).into());
            })*
        };
    }
    if let Some(v) = value.try_downcast_ref::<bool>() {
        return Value::Bool(*v);
    }
    try_int!(u8 => U64, u16 => U64, u32 => U64, u64 => U64);
    try_int!(i8 => I64, i16 => I64, i32 => I64, i64 => I64);
    if let Some(v) = value.try_downcast_ref::<f32>() {
        return Value::F32(*v);
    }
    if let Some(v) = value.try_downcast_ref::<f64>() {
        return Value::F64(*v);
    }
    if let Some(v) = value.try_downcast_ref::<char>() {
        return Value::Str(v.to_string());
    }
    if let Some(v) = value.try_downcast_ref::<String>() {
        return Value::Str(v.clone());
    }
    if let Some(v) = value.try_downcast_ref::<usize>() {
        return Value::U64(*v as u64);
    }
    if let Some(v) = value.try_downcast_ref::<isize>() {
        return Value::I64(*v as i64);
    }
    if let Some(v) = value.try_downcast_ref::<u128>() {
        return Value::Str(v.to_string());
    }
    if let Some(v) = value.try_downcast_ref::<i128>() {
        return Value::Str(v.to_string());
    }
    let type_path = value
        .get_represented_type_info()
        .map_or("unknown", |info| info.type_path());
    Value::Str(format!("opaque {type_path}"))
}

/// Renders a value as a path segment, for map keys and set members.
fn key_segment(value: &dyn PartialReflect) -> String {
    match opaque_value(value) {
        Value::Bool(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::U64(v) | Value::Len(v) => v.to_string(),
        Value::F32(v) => format!("{v:?}"),
        Value::F64(v) => format!("{v:?}"),
        Value::Str(v) => v,
        _ => {
            // A composite key. Hash its own walk so the segment is stable
            // without needing a rendering rule for every shape.
            let mut hasher = Hasher::new();
            let mut path = String::new();
            walk(value, &mut path, &mut hasher);
            format!("#{:016x}", hasher.finish())
        }
    }
}

/// Walks a reflected value, emitting a leaf for every primitive and an
/// explicit length for every collection.
///
/// Collection lengths matter: without them a diff cannot tell a shorter
/// list from one whose tail happens to match.
pub(crate) fn walk(value: &dyn PartialReflect, path: &mut String, sink: &mut dyn Sink) {
    match value.reflect_ref() {
        ReflectRef::Struct(structure) => {
            for index in 0..structure.field_len() {
                let Some(field) = structure.field_at(index) else {
                    continue;
                };
                let base = path.len();
                match structure.name_at(index) {
                    Some(name) => push_child(path, name),
                    None => push_child(path, &index.to_string()),
                }
                walk(field, path, sink);
                path.truncate(base);
            }
        }
        ReflectRef::TupleStruct(tuple) => {
            for index in 0..tuple.field_len() {
                let Some(field) = tuple.field(index) else {
                    continue;
                };
                let base = path.len();
                push_child(path, &index.to_string());
                walk(field, path, sink);
                path.truncate(base);
            }
        }
        ReflectRef::Tuple(tuple) => {
            for index in 0..tuple.field_len() {
                let Some(field) = tuple.field(index) else {
                    continue;
                };
                let base = path.len();
                push_child(path, &index.to_string());
                walk(field, path, sink);
                path.truncate(base);
            }
        }
        ReflectRef::List(list) => {
            sink.leaf(path, Value::Len(list.len() as u64));
            for (index, element) in list.iter().enumerate() {
                let base = path.len();
                push_index(path, &index.to_string());
                walk(element, path, sink);
                path.truncate(base);
            }
        }
        ReflectRef::Array(array) => {
            sink.leaf(path, Value::Len(array.len() as u64));
            for (index, element) in array.iter().enumerate() {
                let base = path.len();
                push_index(path, &index.to_string());
                walk(element, path, sink);
                path.truncate(base);
            }
        }
        ReflectRef::Map(map) => {
            sink.leaf(path, Value::Len(map.len() as u64));
            // Sorted by rendered key, because a hash map's iteration order
            // is not part of the simulation and must never reach a hash.
            let mut entries: Vec<(String, &dyn PartialReflect)> = map
                .iter()
                .map(|(key, value)| (key_segment(key), value))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, value) in entries {
                let base = path.len();
                push_index(path, &key);
                walk(value, path, sink);
                path.truncate(base);
            }
        }
        ReflectRef::Set(set) => {
            sink.leaf(path, Value::Len(set.len() as u64));
            let mut members: Vec<String> = set.iter().map(key_segment).collect();
            members.sort();
            for (index, member) in members.into_iter().enumerate() {
                let base = path.len();
                push_index(path, &index.to_string());
                sink.leaf(path, Value::Str(member));
                path.truncate(base);
            }
        }
        ReflectRef::Enum(enumeration) => {
            let base = path.len();
            let variant = enumeration.variant_name().to_string();
            if enumeration.field_len() == 0 {
                sink.leaf(path, Value::Str(variant));
            } else {
                push_child(path, &variant);
                for index in 0..enumeration.field_len() {
                    let Some(field) = enumeration.field_at(index) else {
                        continue;
                    };
                    let field_base = path.len();
                    match enumeration.name_at(index) {
                        Some(name) => push_child(path, name),
                        None => push_child(path, &index.to_string()),
                    }
                    walk(field, path, sink);
                    path.truncate(field_base);
                }
            }
            path.truncate(base);
        }
        ReflectRef::Opaque(opaque) => sink.leaf(path, opaque_value(opaque)),
        // Unreachable with the features this crate enables, but kept for
        // two cases it costs nothing to survive: another crate in the
        // graph turning on bevy_reflect's `functions` feature, which adds
        // a variant, and a future version adding more. An unknown kind is
        // recorded as its type path rather than dropped, so the gap is
        // visible in the dump instead of hiding a desync.
        #[allow(unreachable_patterns)]
        _ => {
            let type_path = value
                .get_represented_type_info()
                .map_or("unknown", |info| info.type_path());
            sink.leaf(path, Value::Str(format!("unsupported kind {type_path}")));
        }
    }
}
