//! The variant walk: turns any Godot value into a flat list of path and
//! value pairs, the shape Tickwise hashes and diffs.

use godot::builtin::{
    Aabb, Basis, Color, GString, Plane, Projection, Quaternion, Rect2, Rect2i, StringName,
    Transform2D, Transform3D, VarArray, VarDictionary, Variant, VariantType, Vector2, Vector2i,
    Vector3, Vector3i, Vector4, Vector4i,
};
use godot::builtin::{
    PackedByteArray, PackedColorArray, PackedFloat32Array, PackedFloat64Array, PackedInt32Array,
    PackedInt64Array, PackedStringArray, PackedVector2Array, PackedVector3Array,
};
use godot::meta::ToGodot;
use godot::obj::EngineBitfield;
use godot::obj::Gd;
use tickwise::Value;

/// Receives each leaf the walk produces.
pub(crate) trait Sink {
    fn leaf(&mut self, path: &str, value: Value);
}

/// FNV-1a over path bytes and value bits, so the hash depends both on
/// what the values are and on where they live. A field moving from one
/// node to another changes the hash, which is the point.
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
fn push_child(path: &mut String, name: &str) {
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

/// Emits one named child leaf, restoring the path afterwards.
fn child(path: &mut String, name: &str, value: Value, sink: &mut dyn Sink) {
    let base = path.len();
    push_child(path, name);
    sink.leaf(path, value);
    path.truncate(base);
}

/// Emits the components of a real-valued vector as `.x`, `.y` and so on.
fn reals(path: &mut String, names: &[&str], values: &[f32], sink: &mut dyn Sink) {
    for (name, value) in names.iter().zip(values) {
        child(path, name, Value::F32(*value), sink);
    }
}

/// Emits the components of an integer vector.
fn ints(path: &mut String, names: &[&str], values: &[i32], sink: &mut dyn Sink) {
    for (name, value) in names.iter().zip(values) {
        child(path, name, Value::I64(i64::from(*value)), sink);
    }
}

/// Emits a nested value under a named child path.
fn nested(path: &mut String, name: &str, value: &Variant, sink: &mut dyn Sink) {
    let base = path.len();
    push_child(path, name);
    walk(value, path, sink);
    path.truncate(base);
}

/// Renders a value as a path segment, for dictionary keys.
fn key_segment(value: &Variant) -> String {
    let kind = value.get_type();
    if kind == VariantType::STRING || kind == VariantType::STRING_NAME {
        value.to_string()
    } else if kind == VariantType::INT {
        value.to::<i64>().to_string()
    } else if kind == VariantType::FLOAT {
        format!("{:?}", value.to::<f64>())
    } else if kind == VariantType::BOOL {
        value.to::<bool>().to_string()
    } else if kind == VariantType::NIL {
        "null".to_string()
    } else {
        // A composite key. Hash its own walk so the segment is stable
        // without needing a rendering rule for every shape.
        let mut hasher = Hasher::new();
        let mut path = String::new();
        walk(value, &mut path, &mut hasher);
        format!("#{:016x}", hasher.finish())
    }
}

/// Walks one packed array, emitting its length and every element.
macro_rules! packed {
    ($path:expr, $sink:expr, $array:expr, $convert:expr) => {{
        $sink.leaf($path, Value::Len($array.len() as u64));
        for (index, element) in $array.as_slice().iter().enumerate() {
            let base = $path.len();
            push_index($path, &index.to_string());
            let convert: fn(_) -> Value = $convert;
            $sink.leaf($path, convert(*element));
            $path.truncate(base);
        }
    }};
}

/// Walks a Godot value, emitting a leaf for every scalar and an explicit
/// length for every collection.
///
/// Collection lengths matter: without them a diff cannot tell a shorter
/// array from one whose tail happens to match.
pub(crate) fn walk(value: &Variant, path: &mut String, sink: &mut dyn Sink) {
    let kind = value.get_type();

    if kind == VariantType::NIL {
        sink.leaf(path, Value::Null);
    } else if kind == VariantType::BOOL {
        sink.leaf(path, Value::Bool(value.to::<bool>()));
    } else if kind == VariantType::INT {
        sink.leaf(path, Value::I64(value.to::<i64>()));
    } else if kind == VariantType::FLOAT {
        sink.leaf(path, Value::F64(value.to::<f64>()));
    } else if kind == VariantType::STRING
        || kind == VariantType::STRING_NAME
        || kind == VariantType::NODE_PATH
    {
        // All three render to text, and text is what the diff compares.
        sink.leaf(path, Value::Str(value.to_string()));
    } else if kind == VariantType::VECTOR2 {
        let v = value.to::<Vector2>();
        reals(path, &["x", "y"], &[v.x, v.y], sink);
    } else if kind == VariantType::VECTOR2I {
        let v = value.to::<Vector2i>();
        ints(path, &["x", "y"], &[v.x, v.y], sink);
    } else if kind == VariantType::VECTOR3 {
        let v = value.to::<Vector3>();
        reals(path, &["x", "y", "z"], &[v.x, v.y, v.z], sink);
    } else if kind == VariantType::VECTOR3I {
        let v = value.to::<Vector3i>();
        ints(path, &["x", "y", "z"], &[v.x, v.y, v.z], sink);
    } else if kind == VariantType::VECTOR4 {
        let v = value.to::<Vector4>();
        reals(path, &["x", "y", "z", "w"], &[v.x, v.y, v.z, v.w], sink);
    } else if kind == VariantType::VECTOR4I {
        let v = value.to::<Vector4i>();
        ints(path, &["x", "y", "z", "w"], &[v.x, v.y, v.z, v.w], sink);
    } else if kind == VariantType::QUATERNION {
        let q = value.to::<Quaternion>();
        reals(path, &["x", "y", "z", "w"], &[q.x, q.y, q.z, q.w], sink);
    } else if kind == VariantType::COLOR {
        let c = value.to::<Color>();
        reals(path, &["r", "g", "b", "a"], &[c.r, c.g, c.b, c.a], sink);
    } else if kind == VariantType::RECT2 {
        let r = value.to::<Rect2>();
        nested(path, "position", &r.position.to_variant(), sink);
        nested(path, "size", &r.size.to_variant(), sink);
    } else if kind == VariantType::RECT2I {
        let r = value.to::<Rect2i>();
        nested(path, "position", &r.position.to_variant(), sink);
        nested(path, "size", &r.size.to_variant(), sink);
    } else if kind == VariantType::PLANE {
        let p = value.to::<Plane>();
        nested(path, "normal", &p.normal.to_variant(), sink);
        child(path, "d", Value::F32(p.d), sink);
    } else if kind == VariantType::AABB {
        let a = value.to::<Aabb>();
        nested(path, "position", &a.position.to_variant(), sink);
        nested(path, "size", &a.size.to_variant(), sink);
    } else if kind == VariantType::BASIS {
        let b = value.to::<Basis>();
        for (index, row) in b.rows.iter().enumerate() {
            nested(path, &index.to_string(), &row.to_variant(), sink);
        }
    } else if kind == VariantType::TRANSFORM2D {
        let t = value.to::<Transform2D>();
        nested(path, "a", &t.a.to_variant(), sink);
        nested(path, "b", &t.b.to_variant(), sink);
        nested(path, "origin", &t.origin.to_variant(), sink);
    } else if kind == VariantType::TRANSFORM3D {
        let t = value.to::<Transform3D>();
        nested(path, "basis", &t.basis.to_variant(), sink);
        nested(path, "origin", &t.origin.to_variant(), sink);
    } else if kind == VariantType::PROJECTION {
        let p = value.to::<Projection>();
        for (index, column) in p.cols.iter().enumerate() {
            nested(path, &index.to_string(), &column.to_variant(), sink);
        }
    } else if kind == VariantType::ARRAY {
        let array = value.to::<VarArray>();
        sink.leaf(path, Value::Len(array.len() as u64));
        for index in 0..array.len() {
            let base = path.len();
            push_index(path, &index.to_string());
            walk(&array.at(index), path, sink);
            path.truncate(base);
        }
    } else if kind == VariantType::DICTIONARY {
        let dictionary = value.to::<VarDictionary>();
        sink.leaf(path, Value::Len(dictionary.len() as u64));
        // Sorted by rendered key, because a dictionary's insertion order
        // is not part of the simulation and must never reach a hash.
        let mut entries: Vec<(String, Variant)> = dictionary
            .iter_shared()
            .map(|(key, value)| (key_segment(&key), value))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, entry) in entries {
            let base = path.len();
            push_index(path, &key);
            walk(&entry, path, sink);
            path.truncate(base);
        }
    } else if kind == VariantType::PACKED_BYTE_ARRAY {
        let array = value.to::<PackedByteArray>();
        sink.leaf(path, Value::Len(array.len() as u64));
        let base = path.len();
        push_child(path, "bytes");
        sink.leaf(path, Value::Bytes(array.as_slice().to_vec()));
        path.truncate(base);
    } else if kind == VariantType::PACKED_INT32_ARRAY {
        let array = value.to::<PackedInt32Array>();
        packed!(path, sink, array, |v: i32| Value::I64(i64::from(v)));
    } else if kind == VariantType::PACKED_INT64_ARRAY {
        let array = value.to::<PackedInt64Array>();
        packed!(path, sink, array, Value::I64);
    } else if kind == VariantType::PACKED_FLOAT32_ARRAY {
        let array = value.to::<PackedFloat32Array>();
        packed!(path, sink, array, Value::F32);
    } else if kind == VariantType::PACKED_FLOAT64_ARRAY {
        let array = value.to::<PackedFloat64Array>();
        packed!(path, sink, array, Value::F64);
    } else if kind == VariantType::PACKED_STRING_ARRAY {
        let array = value.to::<PackedStringArray>();
        sink.leaf(path, Value::Len(array.len() as u64));
        for (index, element) in array.as_slice().iter().enumerate() {
            let base = path.len();
            push_index(path, &index.to_string());
            sink.leaf(path, Value::Str(element.to_string()));
            path.truncate(base);
        }
    } else if kind == VariantType::PACKED_VECTOR2_ARRAY {
        let array = value.to::<PackedVector2Array>();
        walk_packed_composite(path, sink, array.as_slice(), |element| element.to_variant());
    } else if kind == VariantType::PACKED_VECTOR3_ARRAY {
        let array = value.to::<PackedVector3Array>();
        walk_packed_composite(path, sink, array.as_slice(), |element| element.to_variant());
    } else if kind == VariantType::PACKED_COLOR_ARRAY {
        let array = value.to::<PackedColorArray>();
        walk_packed_composite(path, sink, array.as_slice(), |element| element.to_variant());
    } else if kind == VariantType::OBJECT {
        // An object reference is an address, different on every machine
        // and every run. Its class name is recorded so the gap is visible
        // in the dump, and a node that holds simulation state belongs in
        // the tickwise group in its own right.
        let class = value
            .try_to::<Gd<godot::classes::Object>>()
            .map(|object| object.get_class().to_string())
            .unwrap_or_else(|_| "Object".to_string());
        sink.leaf(path, Value::Str(format!("object {class}, not hashed")));
    } else {
        // RID, Callable, Signal, and anything a later Godot adds. None of
        // these are simulation state, and recording the type keeps the
        // omission visible rather than silent.
        sink.leaf(path, Value::Str(format!("unhashed {kind:?}")));
    }
}

/// Walks a packed array whose elements decompose further.
fn walk_packed_composite<T: Copy>(
    path: &mut String,
    sink: &mut dyn Sink,
    elements: &[T],
    to_variant: impl Fn(&T) -> Variant,
) {
    sink.leaf(path, Value::Len(elements.len() as u64));
    for (index, element) in elements.iter().enumerate() {
        let base = path.len();
        push_index(path, &index.to_string());
        walk(&to_variant(element), path, sink);
        path.truncate(base);
    }
}

/// The name of every script variable on an object, in declaration order.
///
/// Only script variables are considered gameplay state. A node's
/// `position`, `visible`, and `name` are engine properties and stay out,
/// which is what keeps rendering and editor state out of the hash.
pub(crate) fn script_variable_names(object: &Gd<godot::classes::Object>) -> Vec<StringName> {
    use godot::register::info::PropertyUsageFlags;

    let mut names = Vec::new();
    for property in object.get_property_list().iter_shared() {
        let Some(usage) = property.get("usage") else {
            continue;
        };
        // The script variable flag alone. Godot sets the storage flag on
        // exported variables only, so requiring it would silently drop
        // every plain `var` in a script, which is most of a simulation.
        let usage = usage.to::<i64>() as u64;
        if usage & PropertyUsageFlags::SCRIPT_VARIABLE.ord() == 0 {
            continue;
        }
        let Some(name) = property.get("name") else {
            continue;
        };
        // Godot fills this entry with a String, not a StringName, and a
        // direct conversion between the two refuses rather than coercing.
        names.push(StringName::from(&name.to::<GString>()));
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    // The hash itself is plain arithmetic and needs no engine, so its
    // contract is tested here rather than only inside Godot.
    #[test]
    fn the_hash_depends_on_the_path_as_well_as_the_value() {
        let mut here = Hasher::new();
        here.leaf("player.health", Value::I64(10));
        let mut there = Hasher::new();
        there.leaf("enemy.health", Value::I64(10));
        assert_ne!(here.finish(), there.finish());
    }

    #[test]
    fn the_hash_distinguishes_types_that_share_a_bit_pattern() {
        let mut integer = Hasher::new();
        integer.leaf("x", Value::I64(1));
        let mut boolean = Hasher::new();
        boolean.leaf("x", Value::Bool(true));
        assert_ne!(integer.finish(), boolean.finish());
    }

    #[test]
    fn the_hash_is_order_dependent_so_the_walk_must_be_ordered() {
        let mut forward = Hasher::new();
        forward.leaf("a", Value::I64(1));
        forward.leaf("b", Value::I64(2));
        let mut backward = Hasher::new();
        backward.leaf("b", Value::I64(2));
        backward.leaf("a", Value::I64(1));
        assert_ne!(
            forward.finish(),
            backward.finish(),
            "this is why nodes and dictionary keys are sorted before walking"
        );
    }
}
