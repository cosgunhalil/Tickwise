//! A serde `Serializer` that emits field paths into a [`StateDump`].
//!
//! Path conventions: struct fields join with a dot, sequence and tuple
//! elements use `[index]`, map entries use `[key]`, and every collection
//! records a [`Value::Len`] at its own path. `Option::None` and unit
//! become [`Value::Null`], unit enum variants become the variant name as
//! a string, and data-carrying variants nest their payload under
//! `path.VariantName`, so a variant change reads as a structural
//! difference.
//!
//! Because a dump is a sorted map, a `HashMap` in the source state dumps
//! in canonical key order no matter how it iterates. The dump is
//! deterministic even when the state is not, which is one more reason
//! the diff can be trusted.

use crate::dump::{StateDump, Value};
use serde::ser::{self, Impossible, Serialize, Serializer};

/// Error produced while turning a value into a dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpError(String);

impl std::fmt::Display for DumpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot dump value: {}", self.0)
    }
}

impl std::error::Error for DumpError {}

impl ser::Error for DumpError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

/// Turns any `Serialize` value into a structural dump.
pub fn to_dump<T: Serialize + ?Sized>(value: &T) -> Result<StateDump, DumpError> {
    let mut dump = StateDump::empty();
    value.serialize(PathSerializer {
        dump: &mut dump,
        path: String::new(),
    })?;
    Ok(dump)
}

fn child(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{path}.{name}")
    }
}

fn indexed(path: &str, index: usize) -> String {
    format!("{path}[{index}]")
}

struct PathSerializer<'a> {
    dump: &'a mut StateDump,
    path: String,
}

impl PathSerializer<'_> {
    fn put<V: Into<Value>>(self, value: V) -> Result<(), DumpError> {
        self.dump.insert(self.path, value);
        Ok(())
    }
}

impl<'a> Serializer for PathSerializer<'a> {
    type Ok = ();
    type Error = DumpError;
    type SerializeSeq = SeqSerializer<'a>;
    type SerializeTuple = SeqSerializer<'a>;
    type SerializeTupleStruct = SeqSerializer<'a>;
    type SerializeTupleVariant = SeqSerializer<'a>;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = StructSerializer<'a>;
    type SerializeStructVariant = StructSerializer<'a>;

    fn serialize_bool(self, v: bool) -> Result<(), DumpError> {
        self.put(v)
    }
    fn serialize_i8(self, v: i8) -> Result<(), DumpError> {
        self.put(i64::from(v))
    }
    fn serialize_i16(self, v: i16) -> Result<(), DumpError> {
        self.put(i64::from(v))
    }
    fn serialize_i32(self, v: i32) -> Result<(), DumpError> {
        self.put(i64::from(v))
    }
    fn serialize_i64(self, v: i64) -> Result<(), DumpError> {
        self.put(v)
    }
    fn serialize_u8(self, v: u8) -> Result<(), DumpError> {
        self.put(u64::from(v))
    }
    fn serialize_u16(self, v: u16) -> Result<(), DumpError> {
        self.put(u64::from(v))
    }
    fn serialize_u32(self, v: u32) -> Result<(), DumpError> {
        self.put(u64::from(v))
    }
    fn serialize_u64(self, v: u64) -> Result<(), DumpError> {
        self.put(v)
    }
    fn serialize_f32(self, v: f32) -> Result<(), DumpError> {
        self.put(v)
    }
    fn serialize_f64(self, v: f64) -> Result<(), DumpError> {
        self.put(v)
    }
    fn serialize_char(self, v: char) -> Result<(), DumpError> {
        self.put(v.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<(), DumpError> {
        self.put(v)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), DumpError> {
        self.put(v.to_vec())
    }
    fn serialize_none(self) -> Result<(), DumpError> {
        self.put(Value::Null)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), DumpError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), DumpError> {
        self.put(Value::Null)
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), DumpError> {
        self.put(Value::Null)
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<(), DumpError> {
        self.put(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<(), DumpError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), DumpError> {
        let path = child(&self.path, variant);
        value.serialize(PathSerializer {
            dump: self.dump,
            path,
        })
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<SeqSerializer<'a>, DumpError> {
        Ok(SeqSerializer {
            dump: self.dump,
            path: self.path,
            index: 0,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<SeqSerializer<'a>, DumpError> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<SeqSerializer<'a>, DumpError> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<SeqSerializer<'a>, DumpError> {
        Ok(SeqSerializer {
            path: child(&self.path, variant),
            dump: self.dump,
            index: 0,
        })
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<MapSerializer<'a>, DumpError> {
        Ok(MapSerializer {
            dump: self.dump,
            path: self.path,
            key: None,
            count: 0,
        })
    }
    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<StructSerializer<'a>, DumpError> {
        Ok(StructSerializer {
            dump: self.dump,
            path: self.path,
        })
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<StructSerializer<'a>, DumpError> {
        Ok(StructSerializer {
            path: child(&self.path, variant),
            dump: self.dump,
        })
    }
}

/// Sequences, tuples, and tuple variants: elements at `path[i]`, then a
/// length entry at `path`.
pub struct SeqSerializer<'a> {
    dump: &'a mut StateDump,
    path: String,
    index: usize,
}

impl SeqSerializer<'_> {
    fn element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), DumpError> {
        let path = indexed(&self.path, self.index);
        self.index += 1;
        value.serialize(PathSerializer {
            dump: self.dump,
            path,
        })
    }

    fn finish(self) -> Result<(), DumpError> {
        self.dump.insert(self.path, Value::Len(self.index as u64));
        Ok(())
    }
}

impl ser::SerializeSeq for SeqSerializer<'_> {
    type Ok = ();
    type Error = DumpError;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), DumpError> {
        self.element(value)
    }
    fn end(self) -> Result<(), DumpError> {
        self.finish()
    }
}

impl ser::SerializeTuple for SeqSerializer<'_> {
    type Ok = ();
    type Error = DumpError;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), DumpError> {
        self.element(value)
    }
    fn end(self) -> Result<(), DumpError> {
        self.finish()
    }
}

impl ser::SerializeTupleStruct for SeqSerializer<'_> {
    type Ok = ();
    type Error = DumpError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), DumpError> {
        self.element(value)
    }
    fn end(self) -> Result<(), DumpError> {
        self.finish()
    }
}

impl ser::SerializeTupleVariant for SeqSerializer<'_> {
    type Ok = ();
    type Error = DumpError;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), DumpError> {
        self.element(value)
    }
    fn end(self) -> Result<(), DumpError> {
        self.finish()
    }
}

/// Maps: entries at `path[key]`, then a length entry at `path`. Keys must
/// be strings, integers, bools, or chars.
pub struct MapSerializer<'a> {
    dump: &'a mut StateDump,
    path: String,
    key: Option<String>,
    count: usize,
}

impl ser::SerializeMap for MapSerializer<'_> {
    type Ok = ();
    type Error = DumpError;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), DumpError> {
        self.key = Some(key.serialize(KeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), DumpError> {
        let key = self
            .key
            .take()
            .ok_or_else(|| DumpError("map value without a key".to_string()))?;
        self.count += 1;
        value.serialize(PathSerializer {
            dump: self.dump,
            path: format!("{}[{key}]", self.path),
        })
    }

    fn end(self) -> Result<(), DumpError> {
        self.dump.insert(self.path, Value::Len(self.count as u64));
        Ok(())
    }
}

/// Structs and struct variants: fields at `path.field`.
pub struct StructSerializer<'a> {
    dump: &'a mut StateDump,
    path: String,
}

impl ser::SerializeStruct for StructSerializer<'_> {
    type Ok = ();
    type Error = DumpError;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), DumpError> {
        let path = child(&self.path, key);
        value.serialize(PathSerializer {
            dump: self.dump,
            path,
        })
    }
    fn end(self) -> Result<(), DumpError> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for StructSerializer<'_> {
    type Ok = ();
    type Error = DumpError;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), DumpError> {
        ser::SerializeStruct::serialize_field(self, key, value)
    }
    fn end(self) -> Result<(), DumpError> {
        Ok(())
    }
}

/// Renders map keys as path segments.
struct KeySerializer;

fn bad_key<T>(what: &str) -> Result<T, DumpError> {
    Err(DumpError(format!(
        "map keys must be strings, integers, bools, or chars, found {what}"
    )))
}

impl Serializer for KeySerializer {
    type Ok = String;
    type Error = DumpError;
    type SerializeSeq = Impossible<String, DumpError>;
    type SerializeTuple = Impossible<String, DumpError>;
    type SerializeTupleStruct = Impossible<String, DumpError>;
    type SerializeTupleVariant = Impossible<String, DumpError>;
    type SerializeMap = Impossible<String, DumpError>;
    type SerializeStruct = Impossible<String, DumpError>;
    type SerializeStructVariant = Impossible<String, DumpError>;

    fn serialize_bool(self, v: bool) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_i8(self, v: i8) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_i16(self, v: i16) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_i32(self, v: i32) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_i64(self, v: i64) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_u8(self, v: u8) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_u16(self, v: u16) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_u32(self, v: u32) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_u64(self, v: u64) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_f32(self, _v: f32) -> Result<String, DumpError> {
        bad_key("f32")
    }
    fn serialize_f64(self, _v: f64) -> Result<String, DumpError> {
        bad_key("f64")
    }
    fn serialize_char(self, v: char) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<String, DumpError> {
        Ok(v.to_string())
    }
    fn serialize_bytes(self, _v: &[u8]) -> Result<String, DumpError> {
        bad_key("bytes")
    }
    fn serialize_none(self) -> Result<String, DumpError> {
        bad_key("none")
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<String, DumpError> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<String, DumpError> {
        bad_key("unit")
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<String, DumpError> {
        bad_key("unit struct")
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<String, DumpError> {
        Ok(variant.to_string())
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<String, DumpError> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<String, DumpError> {
        bad_key("newtype variant")
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, DumpError> {
        bad_key("sequence")
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, DumpError> {
        bad_key("tuple")
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, DumpError> {
        bad_key("tuple struct")
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, DumpError> {
        bad_key("tuple variant")
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, DumpError> {
        bad_key("map")
    }
    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, DumpError> {
        bad_key("struct")
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, DumpError> {
        bad_key("struct variant")
    }
}
