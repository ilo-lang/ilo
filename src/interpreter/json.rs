//! Value ↔ serde_json::Value conversions.
//!
//! Used by the tools infrastructure to marshal ilo values over HTTP/JSON
//! and by the `--json` output mode.

use super::Value;
use crate::ast::Type;

#[allow(dead_code)] // used when `tools` feature is enabled and in tests
impl Value {
    /// Serialize an ilo `Value` to a `serde_json::Value`.
    ///
    /// Returns `Err` only for `Value::FnRef`, which cannot be serialized.
    pub fn to_json(&self) -> Result<serde_json::Value, String> {
        match self {
            Value::Number(n) => {
                if !n.is_finite() {
                    // NaN / Infinity are not valid JSON numbers
                    return Ok(serde_json::Value::Null);
                }
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    Ok(serde_json::Value::Number(serde_json::Number::from(
                        *n as i64,
                    )))
                } else {
                    serde_json::Number::from_f64(*n)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| format!("cannot serialize number {n} to JSON"))
                }
            }
            Value::Text(s) => Ok(serde_json::Value::String(s.clone())),
            Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
            Value::Nil => Ok(serde_json::Value::Null),
            Value::List(items) => {
                let arr: Result<Vec<_>, _> = items.iter().map(|v| v.to_json()).collect();
                Ok(serde_json::Value::Array(arr?))
            }
            Value::Map(m) => {
                // Numeric keys are stringified, matching JS/Python JSON
                // serialisation conventions. The round-trip is lossy: a map
                // with numeric keys will come back as a record with text keys.
                let mut json_map = serde_json::Map::with_capacity(m.len());
                for (k, v) in m.iter() {
                    json_map.insert(k.to_display_string(), v.to_json()?);
                }
                Ok(serde_json::Value::Object(json_map))
            }
            Value::Record { fields, .. } => {
                let mut map = serde_json::Map::with_capacity(fields.len());
                for (k, v) in fields {
                    map.insert(k.clone(), v.to_json()?);
                }
                Ok(serde_json::Value::Object(map))
            }
            Value::Ok(inner) => {
                let mut map = serde_json::Map::with_capacity(1);
                map.insert("ok".to_string(), inner.to_json()?);
                Ok(serde_json::Value::Object(map))
            }
            Value::Err(inner) => {
                let mut map = serde_json::Map::with_capacity(1);
                map.insert("err".to_string(), inner.to_json()?);
                Ok(serde_json::Value::Object(map))
            }
            Value::FnRef(_) => Err("functions cannot be serialized".to_string()),
            Value::Closure { .. } => Err("closures cannot be serialized".to_string()),
        }
    }

    /// Deserialize a `serde_json::Value` into an ilo `Value`.
    ///
    /// `type_hint` is used as an escape hatch: if the hint is `Type::Text` and
    /// the JSON value is not a string, it is serialized to its JSON string
    /// representation and returned as `Value::Text`.
    pub fn from_json(json: &serde_json::Value, type_hint: Option<&Type>) -> Result<Value, String> {
        // Type::Text escape hatch: coerce any JSON value to a text string.
        if matches!(type_hint, Some(Type::Text)) {
            if let serde_json::Value::String(s) = json {
                return Ok(Value::Text(s.clone()));
            }
            return Ok(Value::Text(json.to_string()));
        }

        match json {
            serde_json::Value::Number(n) => {
                // Prefer f64 directly; fall back through i64 for large integers.
                let f = n
                    .as_f64()
                    .or_else(|| n.as_i64().map(|i| i as f64))
                    .unwrap_or(0.0);
                Ok(Value::Number(f))
            }
            serde_json::Value::String(s) => Ok(Value::Text(s.clone())),
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            serde_json::Value::Null => Ok(Value::Nil),
            serde_json::Value::Array(arr) => {
                let items: Result<Vec<_>, _> =
                    arr.iter().map(|v| Value::from_json(v, None)).collect();
                Ok(Value::List(items?))
            }
            serde_json::Value::Object(map) => {
                // `{"ok": ...}` → Value::Ok(...)
                if map.len() == 1 {
                    if let Some(inner) = map.get("ok") {
                        let v = Value::from_json(inner, None)?;
                        return Ok(Value::Ok(Box::new(v)));
                    }
                    if let Some(inner) = map.get("err") {
                        let v = Value::from_json(inner, None)?;
                        return Ok(Value::Err(Box::new(v)));
                    }
                }
                // Generic object → Record with type_name "_"
                let mut fields = std::collections::HashMap::with_capacity(map.len());
                for (k, v) in map {
                    fields.insert(k.clone(), Value::from_json(v, None)?);
                }
                Ok(Value::Record {
                    type_name: "_".to_string(),
                    fields,
                })
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── to_json ─────────────────────────────────────────────────────────

    #[test]
    fn to_json_number_integer() {
        let v = Value::Number(42.0);
        assert_eq!(v.to_json().unwrap(), json!(42));
    }

    #[test]
    fn to_json_number_float() {
        let v = Value::Number(3.14);
        assert_eq!(v.to_json().unwrap(), json!(3.14));
    }

    #[test]
    fn to_json_nan_is_null() {
        let v = Value::Number(f64::NAN);
        assert_eq!(v.to_json().unwrap(), json!(null));
    }

    #[test]
    fn to_json_infinity_is_null() {
        let v = Value::Number(f64::INFINITY);
        assert_eq!(v.to_json().unwrap(), json!(null));
    }

    #[test]
    fn to_json_text() {
        let v = Value::Text("hello".to_string());
        assert_eq!(v.to_json().unwrap(), json!("hello"));
    }

    #[test]
    fn to_json_bool() {
        assert_eq!(Value::Bool(true).to_json().unwrap(), json!(true));
        assert_eq!(Value::Bool(false).to_json().unwrap(), json!(false));
    }

    #[test]
    fn to_json_nil() {
        assert_eq!(Value::Nil.to_json().unwrap(), json!(null));
    }

    #[test]
    fn to_json_list() {
        let v = Value::List(vec![Value::Number(1.0), Value::Text("a".to_string())]);
        assert_eq!(v.to_json().unwrap(), json!([1, "a"]));
    }

    #[test]
    fn to_json_record() {
        let mut fields = std::collections::HashMap::new();
        fields.insert("x".to_string(), Value::Number(5.0));
        let v = Value::Record {
            type_name: "Point".to_string(),
            fields,
        };
        let j = v.to_json().unwrap();
        assert_eq!(j["x"], json!(5));
    }

    #[test]
    fn to_json_ok() {
        let v = Value::Ok(Box::new(Value::Number(1.0)));
        assert_eq!(v.to_json().unwrap(), json!({"ok": 1}));
    }

    #[test]
    fn to_json_err() {
        let v = Value::Err(Box::new(Value::Text("oops".to_string())));
        assert_eq!(v.to_json().unwrap(), json!({"err": "oops"}));
    }

    #[test]
    fn to_json_fnref_is_error() {
        let v = Value::FnRef("f".to_string());
        assert!(v.to_json().is_err());
    }

    // ── from_json ────────────────────────────────────────────────────────

    #[test]
    fn from_json_number() {
        let v = Value::from_json(&json!(3.14), None).unwrap();
        assert_eq!(v, Value::Number(3.14));
    }

    #[test]
    fn from_json_string() {
        let v = Value::from_json(&json!("hi"), None).unwrap();
        assert_eq!(v, Value::Text("hi".to_string()));
    }

    #[test]
    fn from_json_bool() {
        assert_eq!(
            Value::from_json(&json!(true), None).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn from_json_null() {
        assert_eq!(Value::from_json(&json!(null), None).unwrap(), Value::Nil);
    }

    #[test]
    fn from_json_array() {
        let v = Value::from_json(&json!([1, 2, 3]), None).unwrap();
        assert_eq!(
            v,
            Value::List(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn from_json_object_generic() {
        let v = Value::from_json(&json!({"a": 1, "b": "two"}), None).unwrap();
        match v {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "_");
                assert_eq!(fields["a"], Value::Number(1.0));
                assert_eq!(fields["b"], Value::Text("two".to_string()));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    #[test]
    fn from_json_ok_wrapper() {
        let v = Value::from_json(&json!({"ok": 42}), None).unwrap();
        assert_eq!(v, Value::Ok(Box::new(Value::Number(42.0))));
    }

    #[test]
    fn from_json_err_wrapper() {
        let v = Value::from_json(&json!({"err": "bad"}), None).unwrap();
        assert_eq!(v, Value::Err(Box::new(Value::Text("bad".to_string()))));
    }

    #[test]
    fn from_json_type_hint_text_coerces() {
        // A JSON number coerced to text when hint is Type::Text
        let v = Value::from_json(&json!(42), Some(&Type::Text)).unwrap();
        assert_eq!(v, Value::Text("42".to_string()));
    }

    #[test]
    fn from_json_type_hint_text_passthrough() {
        // A JSON string stays text when hint is Type::Text
        let v = Value::from_json(&json!("hello"), Some(&Type::Text)).unwrap();
        assert_eq!(v, Value::Text("hello".to_string()));
    }

    // ── round-trips ──────────────────────────────────────────────────────

    #[test]
    fn round_trip_number() {
        let v = Value::Number(99.5);
        let j = v.to_json().unwrap();
        let back = Value::from_json(&j, None).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn round_trip_ok_nil() {
        let v = Value::Ok(Box::new(Value::Nil));
        let j = v.to_json().unwrap();
        let back = Value::from_json(&j, None).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn from_json_single_entry_map_not_ok_not_err() {
        // A map with 1 entry whose key is neither "ok" nor "err" → generic Record
        let v = Value::from_json(&json!({"x": 99}), None).unwrap();
        match v {
            Value::Record { type_name, fields } => {
                assert_eq!(type_name, "_");
                assert_eq!(fields["x"], Value::Number(99.0));
            }
            other => panic!("expected Record, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_list_of_text() {
        let v = Value::List(vec![
            Value::Text("a".to_string()),
            Value::Text("b".to_string()),
        ]);
        let j = v.to_json().unwrap();
        let back = Value::from_json(&j, None).unwrap();
        assert_eq!(back, v);
    }
}
