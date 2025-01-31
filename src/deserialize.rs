//! Deserialization of an evaluated program to plain Rust types.

use std::collections::HashMap;

use serde::de::{
    Deserialize, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};

use crate::identifier::Ident;
use crate::term::{MetaValue, RichTerm, Term};

macro_rules! deserialize_number {
    ($method:ident, $type:tt, $visit:ident) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match unwrap_term(self)? {
                Term::Num(n) => visitor.$visit(n as $type),
                other => Err(RustDeserializationError::InvalidType {
                    expected: "Num".to_string(),
                    occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
                }),
            }
        }
    };
}

macro_rules! deserialize_number_round {
    ($method:ident, $type:tt, $visit:ident) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match unwrap_term(self)? {
                Term::Num(n) => visitor.$visit(n.round() as $type),
                other => Err(RustDeserializationError::InvalidType {
                    expected: "Num".to_string(),
                    occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
                }),
            }
        }
    };
}

/// An error occurred during deserialization to Rust.
#[derive(Debug, PartialEq, Clone)]
pub enum RustDeserializationError {
    InvalidType { expected: String, occurred: String },
    MissingValue,
    EmptyMetaValue,
    UnimplementedType { occurred: String },
    InvalidRecordLength(usize),
    InvalidArrayLength(usize),
    Other(String),
}

impl<'de> serde::Deserializer<'de> for RichTerm {
    type Error = RustDeserializationError;

    /// Catch-all deserialization
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Null => visitor.visit_unit(),
            Term::Bool(v) => visitor.visit_bool(v),
            Term::Num(v) => visitor.visit_f64(v),
            Term::Str(v) => visitor.visit_string(v),
            Term::Enum(v) => visitor.visit_enum(EnumDeserializer {
                variant: v.label,
                rich_term: None,
            }),
            Term::Record(v, _) => visit_record(v, visitor),
            Term::Array(v, _) => visit_array(v, visitor),
            Term::MetaValue(_) => visitor.visit_unit(),
            other => Err(RustDeserializationError::UnimplementedType {
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    deserialize_number_round!(deserialize_i8, i8, visit_i8);
    deserialize_number_round!(deserialize_i16, i16, visit_i16);
    deserialize_number_round!(deserialize_i32, i32, visit_i32);
    deserialize_number_round!(deserialize_i64, i64, visit_i64);
    deserialize_number_round!(deserialize_i128, i128, visit_i128);
    deserialize_number_round!(deserialize_u8, u8, visit_u8);
    deserialize_number_round!(deserialize_u16, u16, visit_u16);
    deserialize_number_round!(deserialize_u32, u32, visit_u32);
    deserialize_number_round!(deserialize_u64, u64, visit_u64);
    deserialize_number_round!(deserialize_u128, u128, visit_u128);
    deserialize_number!(deserialize_f32, f32, visit_f32);
    deserialize_number!(deserialize_f64, f64, visit_f64);

    /// Deserialize nullable field.
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Null => visitor.visit_none(),
            some => visitor.visit_some(RichTerm::from(some)),
        }
    }

    /// deserialize `RichTerm::Enum` tags or `RichTerm::Record`s with a single item.
    fn deserialize_enum<V>(
        self,
        _name: &str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let (variant, rich_term) = match unwrap_term(self)? {
            Term::Enum(ident) => (ident.label, None),
            Term::Record(v, _) => {
                let mut iter = v.into_iter();
                let (variant, value) = match iter.next() {
                    Some(v) => v,
                    None => {
                        return Err(RustDeserializationError::InvalidType {
                            expected: "Record with single key".to_string(),
                            occurred: "Record without keys".to_string(),
                        });
                    }
                };
                if iter.next().is_some() {
                    return Err(RustDeserializationError::InvalidType {
                        expected: "Record with single key".to_string(),
                        occurred: "Record with multiple keys".to_string(),
                    });
                }
                (variant.label, Some(value))
            }
            other => {
                return Err(RustDeserializationError::InvalidType {
                    expected: "Enum or Record".to_string(),
                    occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
                });
            }
        };

        visitor.visit_enum(EnumDeserializer { variant, rich_term })
    }

    /// Deserialize pass-through tuples/structs.
    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    /// Deserialize `RichTerm::Bool`
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Bool(v) => visitor.visit_bool(v),
            other => Err(RustDeserializationError::InvalidType {
                expected: "Bool".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    /// Deserialize `RichTerm::Str` as char
    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    /// Deserialize `RichTerm::Str` as str
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    /// Deserialize `RichTerm::Str` as String
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Str(v) => visitor.visit_string(v),
            other => Err(RustDeserializationError::InvalidType {
                expected: "Str".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    /// Deserialize `RichTerm::Str` as String or `RichTerm::Array` as array,
    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_byte_buf(visitor)
    }

    /// Deserialize `RichTerm::Str` as String or `RichTerm::Array` as array,
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Str(v) => visitor.visit_string(v),
            Term::Array(v, _) => visit_array(v, visitor),
            other => Err(RustDeserializationError::InvalidType {
                expected: "Str or Array".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    /// Deserialize `RichTerm::Null` as `()`.
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Null => visitor.visit_unit(),
            other => Err(RustDeserializationError::InvalidType {
                expected: "Null".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    /// Deserialize `RichTerm::Null` as `()`.
    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    /// Deserialize `RichTerm::Array` as `Vec<T>`.
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Array(v, _) => visit_array(v, visitor),
            other => Err(RustDeserializationError::InvalidType {
                expected: "Array".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    /// Deserialize `RichTerm::Array` as `Vec<T>`.
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    /// Deserialize `RichTerm::Array` as `Vec<T>`.
    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    /// Deserialize `RichTerm::Record` as `HashMap<K, V>`.
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Record(v, _) => visit_record(v, visitor),
            other => Err(RustDeserializationError::InvalidType {
                expected: "Record".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    /// Deserialize `RichTerm::Record` as `struct`.
    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match unwrap_term(self)? {
            Term::Array(v, _) => visit_array(v, visitor),
            Term::Record(v, _) => visit_record(v, visitor),
            other => Err(RustDeserializationError::InvalidType {
                expected: "Record".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
        }
    }

    /// Deserialize `Ident` as `String`.
    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        drop(self);
        visitor.visit_unit()
    }
}

struct ArrayDeserializer {
    iter: std::vec::IntoIter<RichTerm>,
}

impl ArrayDeserializer {
    fn new(vec: Vec<RichTerm>) -> Self {
        ArrayDeserializer {
            iter: vec.into_iter(),
        }
    }
}

impl<'de> SeqAccess<'de> for ArrayDeserializer {
    type Error = RustDeserializationError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

fn unwrap_term(mut rich_term: RichTerm) -> Result<Term, RustDeserializationError> {
    loop {
        rich_term = match Term::from(rich_term) {
            Term::MetaValue(MetaValue { value, .. }) => {
                if let Some(rich_term) = value {
                    rich_term
                } else {
                    break Err(RustDeserializationError::EmptyMetaValue);
                }
            }
            other => break Ok(other),
        }
    }
}

fn visit_array<'de, V>(
    array: Vec<RichTerm>,
    visitor: V,
) -> Result<V::Value, RustDeserializationError>
where
    V: Visitor<'de>,
{
    let len = array.len();
    let mut deserializer = ArrayDeserializer::new(array);
    let seq = visitor.visit_seq(&mut deserializer)?;
    let remaining = deserializer.iter.len();
    if remaining == 0 {
        Ok(seq)
    } else {
        Err(RustDeserializationError::InvalidArrayLength(len))
    }
}

struct RecordDeserializer {
    iter: <HashMap<Ident, RichTerm> as IntoIterator>::IntoIter,
    rich_term: Option<RichTerm>,
}

impl RecordDeserializer {
    fn new(map: HashMap<Ident, RichTerm>) -> Self {
        RecordDeserializer {
            iter: map.into_iter(),
            rich_term: None,
        }
    }
}

impl<'de> MapAccess<'de> for RecordDeserializer {
    type Error = RustDeserializationError;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.iter.next() {
            Some((key, value)) => {
                self.rich_term = Some(value);
                seed.deserialize(key.label.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.rich_term.take() {
            Some(value) => seed.deserialize(value),
            _ => Err(RustDeserializationError::MissingValue),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.iter.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

fn visit_record<'de, V>(
    record: HashMap<Ident, RichTerm>,
    visitor: V,
) -> Result<V::Value, RustDeserializationError>
where
    V: Visitor<'de>,
{
    let len = record.len();
    let mut deserializer = RecordDeserializer::new(record);
    let map = visitor.visit_map(&mut deserializer)?;
    let remaining = deserializer.iter.len();
    if remaining == 0 {
        Ok(map)
    } else {
        Err(RustDeserializationError::InvalidRecordLength(len))
    }
}

struct VariantDeserializer {
    rich_term: Option<RichTerm>,
}

impl<'de> VariantAccess<'de> for VariantDeserializer {
    type Error = RustDeserializationError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        match self.rich_term {
            Some(value) => Deserialize::deserialize(value),
            None => Ok(()),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.rich_term {
            Some(value) => seed.deserialize(value),
            None => Err(RustDeserializationError::MissingValue),
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.rich_term.map(unwrap_term) {
            Some(Ok(Term::Array(v, _))) => {
                if v.is_empty() {
                    visitor.visit_unit()
                } else {
                    visit_array(v, visitor)
                }
            }
            Some(Ok(other)) => Err(RustDeserializationError::InvalidType {
                expected: "Array variant".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
            Some(Err(err)) => Err(err),
            None => Err(RustDeserializationError::MissingValue),
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.rich_term.map(unwrap_term) {
            Some(Ok(Term::Record(v, _))) => visit_record(v, visitor),
            Some(Ok(other)) => Err(RustDeserializationError::InvalidType {
                expected: "Array variant".to_string(),
                occurred: other.type_of().unwrap_or_else(|| "Other".to_string()),
            }),
            Some(Err(err)) => Err(err),
            None => Err(RustDeserializationError::MissingValue),
        }
    }
}

struct EnumDeserializer {
    variant: String,
    rich_term: Option<RichTerm>,
}

impl<'de> EnumAccess<'de> for EnumDeserializer {
    type Error = RustDeserializationError;
    type Variant = VariantDeserializer;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, VariantDeserializer), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant = self.variant.into_deserializer();
        let visitor = VariantDeserializer {
            rich_term: self.rich_term,
        };
        seed.deserialize(variant).map(|v| (v, visitor))
    }
}

impl std::fmt::Display for RustDeserializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RustDeserializationError::InvalidType {
                ref expected,
                ref occurred,
            } => write!(f, "invalid type: {occurred}, expected: {expected}"),
            RustDeserializationError::MissingValue => write!(f, "missing value"),
            RustDeserializationError::EmptyMetaValue => write!(f, "empty Metavalue"),
            RustDeserializationError::InvalidRecordLength(len) => {
                write!(f, "invalid record length, expected {len}")
            }
            RustDeserializationError::InvalidArrayLength(len) => {
                write!(f, "invalid array length, expected {len}")
            }
            RustDeserializationError::UnimplementedType { ref occurred } => {
                write!(f, "unimplemented conversion from type: {occurred}")
            }
            RustDeserializationError::Other(ref err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for RustDeserializationError {}

impl serde::de::Error for RustDeserializationError {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        RustDeserializationError::Other(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde::Deserialize;

    use super::RustDeserializationError;
    use crate::program::Program;

    #[test]
    fn rust_deserialize_struct_with_fields() {
        #[derive(Debug, PartialEq, Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum E {
            Foo,
            Bar,
        }

        #[derive(Debug, PartialEq, Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum H {
            Foo(u16),
            Bar(String),
        }

        #[derive(Debug, PartialEq, Deserialize)]
        struct A {
            a: f64,
            b: String,
            c: (),
            d: bool,
            e: E,
            f: Option<bool>,
            g: i16,
            h: H,
        }

        assert_eq!(
            A::deserialize(
                Program::new_from_source(
                    Cursor::new(
                        br#"{ a = 10, b = "test string", c = null, d = true, e = `foo, f = null, g = -10, h = { bar = "some other string" } }"#.to_vec()
                    ),
                    "source"
                )
                .expect("program should't fail")
                .eval_full()
                .expect("evaluation should't fail")
            )
            .expect("deserialization should't fail"),
            A {
                a: 10.0,
                b: "test string".to_string(),
                c: (),
                d: true,
                e: E::Foo,
                f: None,
                g: -10,
                h: H::Bar("some other string".to_string())
            }
        )
    }

    #[test]
    fn rust_deserialize_array_of_numbers() {
        assert_eq!(
            Vec::<f64>::deserialize(
                Program::new_from_source(Cursor::new(br#"[1, 2, 3, 4]"#.to_vec()), "source")
                    .expect("program should't fail")
                    .eval_full()
                    .expect("evaluation should't fail")
            )
            .expect("deserialization should't fail"),
            vec![1.0, 2.0, 3.0, 4.0]
        )
    }

    #[test]
    fn rust_deserialize_fail_non_data() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct A;

        assert_eq!(
            A::deserialize(
                Program::new_from_source(Cursor::new(br#"fun a b => a + b"#.to_vec()), "source")
                    .expect("program should't fail")
                    .eval_full()
                    .expect("evaluation should't fail")
            ),
            Err(RustDeserializationError::InvalidType {
                expected: "Null".to_string(),
                occurred: "Fun".to_string()
            })
        )
    }

    #[test]
    fn rust_deserialize_ignore_metavalue() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct A {
            a: f64,
        }

        assert_eq!(
            A::deserialize(
                Program::new_from_source(Cursor::new(br#"{ a = (10 | Num) }"#.to_vec()), "source")
                    .expect("program should't fail")
                    .eval_full()
                    .expect("evaluation should't fail")
            )
            .expect("deserialization should't fail"),
            A { a: 10.0 }
        )
    }
}
