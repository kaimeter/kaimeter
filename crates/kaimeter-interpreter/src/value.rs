//! Typed values carried by invocation records and rule outputs.
//!
//! A value is the bundle's fixed-point scalar, a UTF-8 string, a boolean, a
//! record with named typed fields, or a list of values. Numeric values enter
//! the interpreter only as decimal strings: JSON numbers are rejected so
//! that binary floating point can never form part of a rule evaluation
//! (whitepaper §4.4).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{Map, Value as Json};

use crate::error::RuleError;
use crate::fixed::Fixed;
use crate::schema::{InputDecl, RecordType, ValueType};

/// One typed value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A fixed-point quantity.
    Scalar(Fixed),
    /// A UTF-8 string.
    Text(String),
    /// A boolean.
    Bool(bool),
    /// A list of values.
    List(Vec<Value>),
    /// A record with named fields.
    Record(BTreeMap<String, Value>),
}

impl Value {
    /// Returns the type name used in error messages.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Scalar(_) => "scalar",
            Self::Text(_) => "text",
            Self::Bool(_) => "bool",
            Self::List(_) => "list",
            Self::Record(_) => "record",
        }
    }

    /// Returns the scalar, or a type error.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::TypeMismatch`] for any other value.
    pub fn as_scalar(&self) -> Result<Fixed, RuleError> {
        match self {
            Self::Scalar(value) => Ok(*value),
            other => Err(other.mismatch("scalar")),
        }
    }

    /// Returns the text, or a type error.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::TypeMismatch`] for any other value.
    pub fn as_text(&self) -> Result<&str, RuleError> {
        match self {
            Self::Text(value) => Ok(value),
            other => Err(other.mismatch("text")),
        }
    }

    /// Returns the boolean, or a type error.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::TypeMismatch`] for any other value.
    pub fn as_bool(&self) -> Result<bool, RuleError> {
        match self {
            Self::Bool(value) => Ok(*value),
            other => Err(other.mismatch("bool")),
        }
    }

    /// Returns the list, or a type error.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::TypeMismatch`] for any other value.
    pub fn as_list(&self) -> Result<&[Value], RuleError> {
        match self {
            Self::List(values) => Ok(values),
            other => Err(other.mismatch("list")),
        }
    }

    /// Returns the record, or a type error.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::TypeMismatch`] for any other value.
    pub fn as_record(&self) -> Result<&BTreeMap<String, Value>, RuleError> {
        match self {
            Self::Record(fields) => Ok(fields),
            other => Err(other.mismatch("record")),
        }
    }

    /// Builds the type error for a value that is not `expected`.
    fn mismatch(&self, expected: &'static str) -> RuleError {
        RuleError::TypeMismatch {
            expected,
            found: self.kind_name(),
        }
    }

    /// Coerces one JSON value according to a declared input type.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::TypeMismatch`] when the JSON value does not
    /// match the declaration, [`RuleError::InvalidJson`] errors for JSON
    /// numbers where a decimal string is required, and the field errors of
    /// [`Value::coerce_record`] for malformed records.
    pub fn coerce(
        json: &Json,
        decl: &InputDecl,
        types: &BTreeMap<String, RecordType>,
    ) -> Result<Self, RuleError> {
        match decl {
            InputDecl::Scalar => coerce_scalar(json),
            InputDecl::Text => coerce_text(json),
            InputDecl::Bool => coerce_bool(json),
            InputDecl::List { of } => coerce_list(json, of, types),
        }
    }

    /// Coerces a JSON object according to a declared record type.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::TypeMismatch`] for a non-object,
    /// [`RuleError::UnknownField`] for fields outside the declaration,
    /// [`RuleError::MissingField`] for declared fields that are absent, and
    /// the field coercion errors of [`Value::coerce`].
    pub fn coerce_record(json: &Json, record: &RecordType) -> Result<Self, RuleError> {
        let Json::Object(object) = json else {
            return Err(mismatch("record", json));
        };
        check_unknown_fields(object, record)?;
        let mut fields = BTreeMap::new();
        for (name, kind) in &record.fields {
            let value = field_value(object, name)?;
            fields.insert(name.clone(), coerce_field(value, *kind)?);
        }
        Ok(Self::Record(fields))
    }
}

/// Checks that a record carries no fields outside its declaration.
fn check_unknown_fields(object: &Map<String, Json>, record: &RecordType) -> Result<(), RuleError> {
    match object.keys().find(|key| !record.fields.contains_key(*key)) {
        Some(key) => Err(RuleError::UnknownField { name: key.clone() }),
        None => Ok(()),
    }
}

/// Returns the JSON value of a declared record field.
fn field_value<'a>(object: &'a Map<String, Json>, name: &str) -> Result<&'a Json, RuleError> {
    object.get(name).ok_or_else(|| RuleError::MissingField {
        name: name.to_string(),
    })
}

/// Coerces a declared scalar field.
fn coerce_field(json: &Json, kind: ValueType) -> Result<Value, RuleError> {
    match kind {
        ValueType::Scalar => coerce_scalar(json),
        ValueType::Text => coerce_text(json),
        ValueType::Bool => coerce_bool(json),
    }
}

/// Coerces a decimal-string scalar.
fn coerce_scalar(json: &Json) -> Result<Value, RuleError> {
    match json {
        Json::String(text) => text.parse::<Fixed>().map(Value::Scalar).map_err(|_error| {
            RuleError::InvalidConstantScalar {
                value: text.clone(),
            }
        }),
        other => Err(mismatch("decimal string", other)),
    }
}

/// Coerces a string.
fn coerce_text(json: &Json) -> Result<Value, RuleError> {
    match json {
        Json::String(text) => Ok(Value::Text(text.clone())),
        other => Err(mismatch("text", other)),
    }
}

/// Coerces a boolean.
fn coerce_bool(json: &Json) -> Result<Value, RuleError> {
    match json {
        Json::Bool(value) => Ok(Value::Bool(*value)),
        other => Err(mismatch("bool", other)),
    }
}

/// Coerces a list of records of one declared type.
fn coerce_list(
    json: &Json,
    of: &str,
    types: &BTreeMap<String, RecordType>,
) -> Result<Value, RuleError> {
    let Json::Array(items) = json else {
        return Err(mismatch("list", json));
    };
    let Some(record) = types.get(of) else {
        return Err(RuleError::UnknownType {
            name: of.to_string(),
        });
    };
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        values.push(Value::coerce_record(item, record)?);
    }
    Ok(Value::List(values))
}

/// Builds a type mismatch for a raw JSON value.
fn mismatch(expected: &'static str, json: &Json) -> RuleError {
    RuleError::TypeMismatch {
        expected,
        found: json_kind(json),
    }
}

/// Names the category of a raw JSON value for error messages.
fn json_kind(json: &Json) -> &'static str {
    match json {
        Json::Null => "null",
        Json::Bool(_) => "bool",
        Json::Number(_) => "number",
        Json::String(_) => "text",
        Json::Array(_) => "list",
        Json::Object(_) => "record",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema_types() -> BTreeMap<String, RecordType> {
        let mut fields = BTreeMap::new();
        fields.insert("see".to_string(), ValueType::Scalar);
        fields.insert("origin".to_string(), ValueType::Text);
        fields.insert("eu".to_string(), ValueType::Bool);
        let mut types = BTreeMap::new();
        types.insert("supply".to_string(), RecordType { fields });
        types
    }

    fn json(text: &str) -> Json {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn coerces_scalars_from_decimal_strings() {
        let decl = InputDecl::Scalar;
        let value = Value::coerce(&json(r#""0.25""#), &decl, &schema_types()).unwrap();
        assert_eq!(value, Value::Scalar("0.25".parse().unwrap()));
    }

    #[test]
    fn rejects_json_numbers_for_scalars() {
        let decl = InputDecl::Scalar;
        assert_eq!(
            Value::coerce(&json("0.25"), &decl, &schema_types()),
            Err(RuleError::TypeMismatch {
                expected: "decimal string",
                found: "number",
            })
        );
    }

    #[test]
    fn rejects_malformed_scalar_strings() {
        let decl = InputDecl::Scalar;
        assert_eq!(
            Value::coerce(&json(r#""many""#), &decl, &schema_types()),
            Err(RuleError::InvalidConstantScalar {
                value: String::from("many"),
            })
        );
    }

    #[test]
    fn coerces_text_and_bool() {
        let types = schema_types();
        assert_eq!(
            Value::coerce(&json(r#""CF4""#), &InputDecl::Text, &types).unwrap(),
            Value::Text(String::from("CF4"))
        );
        assert_eq!(
            Value::coerce(&json("true"), &InputDecl::Bool, &types).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            Value::coerce(&json("true"), &InputDecl::Text, &types),
            Err(RuleError::TypeMismatch {
                expected: "text",
                found: "bool",
            })
        );
    }

    #[test]
    fn coerces_lists_of_records() {
        let decl = InputDecl::List {
            of: String::from("supply"),
        };
        let value = Value::coerce(
            &json(r#"[{"see":"1.835","origin":"third_country","eu":false}]"#),
            &decl,
            &schema_types(),
        )
        .unwrap();
        let Value::List(items) = value else {
            panic!("expected a list");
        };
        assert_eq!(items.len(), 1);
        let record = items[0].as_record().unwrap();
        assert_eq!(record["origin"], Value::Text(String::from("third_country")));
        assert_eq!(record["eu"], Value::Bool(false));
    }

    #[test]
    fn rejects_malformed_lists_and_records() {
        let types = schema_types();
        let decl = InputDecl::List {
            of: String::from("supply"),
        };
        assert_eq!(
            Value::coerce(&json("1"), &decl, &types),
            Err(RuleError::TypeMismatch {
                expected: "list",
                found: "number",
            })
        );
        assert_eq!(
            Value::coerce(&json("[1]"), &decl, &types),
            Err(RuleError::TypeMismatch {
                expected: "record",
                found: "number",
            })
        );
        assert_eq!(
            Value::coerce(&json(r#"[{"extra":"x"}]"#), &decl, &types),
            Err(RuleError::UnknownField {
                name: String::from("extra"),
            })
        );
        assert_eq!(
            Value::coerce(&json(r#"[{"see":"1.0"}]"#), &decl, &types),
            Err(RuleError::MissingField {
                name: String::from("eu"),
            })
        );
        assert_eq!(
            Value::coerce(&json(r#"[{"origin":"x"}]"#), &decl, &types),
            Err(RuleError::MissingField {
                name: String::from("eu"),
            })
        );
        assert_eq!(
            Value::coerce(
                &json(r#"[{"see":"1.0","origin":"x","eu":1}]"#),
                &decl,
                &types
            ),
            Err(RuleError::TypeMismatch {
                expected: "bool",
                found: "number",
            })
        );
        assert_eq!(
            Value::coerce(
                &json("[]"),
                &InputDecl::List {
                    of: String::from("missing")
                },
                &types
            ),
            Err(RuleError::UnknownType {
                name: String::from("missing"),
            })
        );
    }

    #[test]
    fn names_value_kinds() {
        assert_eq!(Value::Scalar(Fixed::ZERO).kind_name(), "scalar");
        assert_eq!(Value::Text(String::new()).kind_name(), "text");
        assert_eq!(Value::Bool(true).kind_name(), "bool");
        assert_eq!(Value::List(Vec::new()).kind_name(), "list");
        assert_eq!(Value::Record(BTreeMap::new()).kind_name(), "record");
    }

    #[test]
    fn accessors_report_type_mismatches() {
        let text = Value::Text(String::from("x"));
        assert_eq!(
            text.as_scalar(),
            Err(RuleError::TypeMismatch {
                expected: "scalar",
                found: "text",
            })
        );
        assert_eq!(
            text.as_bool(),
            Err(RuleError::TypeMismatch {
                expected: "bool",
                found: "text",
            })
        );
        assert_eq!(
            text.as_list(),
            Err(RuleError::TypeMismatch {
                expected: "list",
                found: "text",
            })
        );
        assert_eq!(
            text.as_record(),
            Err(RuleError::TypeMismatch {
                expected: "record",
                found: "text",
            })
        );
        assert_eq!(text.as_text().unwrap(), "x");
        assert!(Value::Bool(true).as_bool().unwrap());
        assert!(Value::List(Vec::new()).as_list().unwrap().is_empty());
        assert!(
            Value::Record(BTreeMap::new())
                .as_record()
                .unwrap()
                .is_empty()
        );
        assert_eq!(Value::Scalar(Fixed::ONE).as_scalar().unwrap(), Fixed::ONE);
    }
}
