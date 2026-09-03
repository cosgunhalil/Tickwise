//! A Tickwise probe for any Bones ECS world.
//!
//! Bones, the framework behind Fish Folk: Jumpy, carries a runtime schema
//! for every component and resource. This crate walks those schemas to
//! hash and dump a `World` without the game writing a single probe line.
//! Register the component and resource types that influence simulation,
//! and [`BonesProbe`] does the rest.
//!
//! Bones ECS itself exposes no iteration over all stores, so coverage is
//! declared explicitly. That matches the Tickwise hash coverage checklist:
//! you decide what is gameplay state and what is not.
//!
//! Paths in dumps look like `Position[3].x` for the `x` field of the
//! `Position` component on entity index 3, `Score.value` for a resource
//! field, and `entities` for the number of alive entities.

use bones_ecs::prelude::*;
use tickwise::{DeterminismProbe, StateDump, Value};

/// A probe over a Bones `World` covering registered components and
/// resources.
///
/// The light hash covers the alive entity count plus the resources added
/// with [`light_resource`](BonesProbe::light_resource). The full hash and
/// the dump cover every registered component and resource.
pub struct BonesProbe<'w> {
    world: &'w World,
    components: Vec<&'static Schema>,
    resources: Vec<&'static Schema>,
    light_resources: Vec<&'static Schema>,
}

impl<'w> BonesProbe<'w> {
    /// Starts a probe over the world with nothing registered yet.
    pub fn new(world: &'w World) -> Self {
        Self {
            world,
            components: Vec::new(),
            resources: Vec::new(),
            light_resources: Vec::new(),
        }
    }

    /// Registers a component type for the full hash and the dump.
    pub fn component<T: HasSchema>(mut self) -> Self {
        self.components.push(T::schema());
        self
    }

    /// Registers a resource type for the full hash and the dump.
    pub fn resource<T: HasSchema>(mut self) -> Self {
        self.resources.push(T::schema());
        self
    }

    /// Registers a resource type for the light hash as well. Keep this
    /// list short: it runs every tick.
    pub fn light_resource<T: HasSchema>(mut self) -> Self {
        let schema = T::schema();
        if !self.resources.contains(&schema) {
            self.resources.push(schema);
        }
        self.light_resources.push(schema);
        self
    }

    fn alive_entity_count(&self) -> u64 {
        self.world
            .resources
            .get::<Entities>()
            .map(|entities| entities.iter().count() as u64)
            .unwrap_or(0)
    }

    fn walk_resource(&self, schema: &'static Schema, path: &mut String, sink: &mut dyn Sink) {
        let untyped = self.world.resources.untyped();
        if !untyped.contains(schema.id()) {
            return;
        }
        let cell = untyped.get(schema);
        let borrowed = cell.borrow();
        let Some(boxed) = borrowed.as_ref() else {
            return;
        };
        let base = path.len();
        path.push_str(&schema.name);
        walk(boxed.as_ref().access(), path, sink);
        path.truncate(base);
    }

    fn walk_component(&self, schema: &'static Schema, path: &mut String, sink: &mut dyn Sink) {
        let Some(entities) = self.world.resources.get::<Entities>() else {
            return;
        };
        let store = self.world.components.get_by_schema(schema);
        let store = store.borrow();
        let base = path.len();
        path.push_str(&schema.name);
        for entity in entities.iter_with_bitset(store.bitset()) {
            let Some(component) = store.get_ref(entity) else {
                continue;
            };
            let entity_base = path.len();
            path.push('[');
            path.push_str(&entity.index().to_string());
            path.push(']');
            walk(component.access(), path, sink);
            path.truncate(entity_base);
        }
        path.truncate(base);
    }

    fn walk_all(&self, sink: &mut dyn Sink) {
        let mut path = String::with_capacity(128);
        sink.leaf("entities", Value::Len(self.alive_entity_count()));
        for schema in &self.resources {
            self.walk_resource(schema, &mut path, sink);
        }
        for schema in &self.components {
            self.walk_component(schema, &mut path, sink);
        }
    }
}

impl DeterminismProbe for BonesProbe<'_> {
    fn light_hash(&self) -> u64 {
        let mut hasher = Hasher::new();
        hasher.leaf("entities", Value::Len(self.alive_entity_count()));
        let mut path = String::with_capacity(64);
        for schema in &self.light_resources {
            self.walk_resource(schema, &mut path, &mut hasher);
        }
        hasher.finish()
    }

    fn full_hash(&self) -> u64 {
        let mut hasher = Hasher::new();
        self.walk_all(&mut hasher);
        hasher.finish()
    }

    fn state_dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        self.walk_all(&mut dump);
        dump
    }
}

/// Receives each leaf value the schema walk produces.
trait Sink {
    fn leaf(&mut self, path: &str, value: Value);
}

impl Sink for StateDump {
    fn leaf(&mut self, path: &str, value: Value) {
        self.insert(path, value);
    }
}

/// FNV-1a over path bytes and value bits, so the hash depends on both
/// what the values are and where they live.
struct Hasher(u64);

impl Hasher {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
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

fn push_child(path: &mut String, name: &str) {
    if !path.is_empty() {
        path.push('.');
    }
    path.push_str(name);
}

/// Renders a map key as a path segment.
fn key_segment(key: SchemaRef<'_>) -> String {
    match key.access() {
        SchemaRefAccess::Primitive(primitive) => match primitive {
            PrimitiveRef::Bool(v) => v.to_string(),
            PrimitiveRef::U8(v) => v.to_string(),
            PrimitiveRef::U16(v) => v.to_string(),
            PrimitiveRef::U32(v) => v.to_string(),
            PrimitiveRef::U64(v) => v.to_string(),
            PrimitiveRef::U128(v) => v.to_string(),
            PrimitiveRef::I8(v) => v.to_string(),
            PrimitiveRef::I16(v) => v.to_string(),
            PrimitiveRef::I32(v) => v.to_string(),
            PrimitiveRef::I64(v) => v.to_string(),
            PrimitiveRef::I128(v) => v.to_string(),
            PrimitiveRef::String(v) => v.to_string(),
            PrimitiveRef::F32(v) => format!("{v:?}"),
            PrimitiveRef::F64(v) => format!("{v:?}"),
            PrimitiveRef::Opaque { schema_ref, .. } => opaque_segment(schema_ref),
        },
        other => opaque_segment(other.into_schema_ref()),
    }
}

fn opaque_segment(value: SchemaRef<'_>) -> String {
    match value.hash() {
        Some(hash) => format!("#{hash:016x}"),
        None => "#opaque".to_string(),
    }
}

/// Walks a schema value, emitting a leaf for every primitive and a length
/// for every collection.
fn walk(access: SchemaRefAccess<'_>, path: &mut String, sink: &mut dyn Sink) {
    match access {
        SchemaRefAccess::Primitive(primitive) => {
            let value = match primitive {
                PrimitiveRef::Bool(v) => Value::Bool(*v),
                PrimitiveRef::U8(v) => Value::U64(u64::from(*v)),
                PrimitiveRef::U16(v) => Value::U64(u64::from(*v)),
                PrimitiveRef::U32(v) => Value::U64(u64::from(*v)),
                PrimitiveRef::U64(v) => Value::U64(*v),
                PrimitiveRef::U128(v) => Value::Str(v.to_string()),
                PrimitiveRef::I8(v) => Value::I64(i64::from(*v)),
                PrimitiveRef::I16(v) => Value::I64(i64::from(*v)),
                PrimitiveRef::I32(v) => Value::I64(i64::from(*v)),
                PrimitiveRef::I64(v) => Value::I64(*v),
                PrimitiveRef::I128(v) => Value::Str(v.to_string()),
                PrimitiveRef::F32(v) => Value::F32(*v),
                PrimitiveRef::F64(v) => Value::F64(*v),
                PrimitiveRef::String(v) => Value::Str(v.clone()),
                // A type without a schema layout. Its own hash function is
                // the best we can do, and its absence is recorded honestly.
                PrimitiveRef::Opaque { schema_ref, .. } => match schema_ref.hash() {
                    Some(hash) => Value::U64(hash),
                    None => Value::Str("opaque, no hash".to_string()),
                },
            };
            sink.leaf(path, value);
        }
        SchemaRefAccess::Struct(structure) => {
            for (index, field) in structure.fields().enumerate() {
                let base = path.len();
                match field.name {
                    Some(name) => push_child(path, name),
                    None => push_child(path, &index.to_string()),
                }
                walk(field.value.access(), path, sink);
                path.truncate(base);
            }
        }
        SchemaRefAccess::Enum(enumeration) => {
            let base = path.len();
            let variant = enumeration.variant_name();
            let value = enumeration.value();
            if value.info().fields.is_empty() {
                sink.leaf(path, Value::Str(variant.to_string()));
            } else {
                push_child(path, variant);
                walk(SchemaRefAccess::Struct(value), path, sink);
            }
            path.truncate(base);
        }
        SchemaRefAccess::Vec(vector) => {
            sink.leaf(path, Value::Len(vector.len() as u64));
            for (index, element) in vector.iter().enumerate() {
                let base = path.len();
                path.push('[');
                path.push_str(&index.to_string());
                path.push(']');
                walk(element.access(), path, sink);
                path.truncate(base);
            }
        }
        SchemaRefAccess::Map(map) => {
            sink.leaf(path, Value::Len(map.len() as u64));
            // Sort entries by rendered key so the walk is deterministic
            // even when the underlying map is not.
            let mut entries: Vec<(String, SchemaRef<'_>)> = map
                .iter()
                .map(|(key, value)| (key_segment(key), value))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, value) in entries {
                let base = path.len();
                path.push('[');
                path.push_str(&key);
                path.push(']');
                walk(value.access(), path, sink);
                path.truncate(base);
            }
        }
    }
}
