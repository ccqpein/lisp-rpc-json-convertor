use anyhow::Context;
use lisp_rpc_rust_parser::{Atom, Expr, TypeValue, TypeValueNumber};
use lisp_rpc_rust_raw_data::{
    Data, DataError, ExprData, FromStr, ListData, MapData, Value as RawValue,
};
use serde::ser::Error as _;
use serde_json::{Map, Result, Value};

use crate::data_to_expr;

#[derive(Debug, Clone, Default)]
pub struct JsonConvertor {
    data_name_key: Option<String>,
}

pub type Convertor = JsonConvertor;

impl JsonConvertor {
    pub fn new() -> Self {
        Self {
            data_name_key: None,
        }
    }

    /// Set the key of data_name for this json object (defaults to "data_name")
    pub fn data_name_key(mut self, data_name_key: impl Into<String>) -> Self {
        self.data_name_key = Some(data_name_key.into());
        self
    }

    /// Returns the current data name key configured on this Convertor (defaults to "data_name")
    pub fn get_data_name_key(&self) -> &str {
        self.data_name_key.as_deref().unwrap_or("data_name")
    }

    /// Convert any Data variant to a serde_json::Value
    pub fn to_json(&self, data: &Data) -> Result<Value> {
        match data {
            Data::Data(expr_data) => self.expr_data_to_json(expr_data),
            Data::List(list_data) => self.list_data_to_json(list_data),
            Data::Map(map_data) => self.map_data_to_json(map_data),
            Data::Value(type_value) => self.type_value_to_json(type_value),
            Data::Error(data_error) => self.data_error_to_json(data_error),
        }
    }

    /// Convert Data to a compact JSON string
    pub fn to_json_string(&self, data: &Data) -> Result<String> {
        serde_json::to_string(&self.to_json(data)?)
    }

    /// Convert Data to a pretty-printed JSON string with indentation
    pub fn to_json_string_pretty(&self, data: &Data) -> Result<String> {
        serde_json::to_string_pretty(&self.to_json(data)?)
    }

    /// Parse a Lisp-RPC data string and convert it to a serde_json::Value
    pub fn to_json_from_str(&self, s: &str) -> anyhow::Result<Value> {
        let mut parser = Default::default();
        let data = Data::from_str(&mut parser, s)
            .with_context(|| format!("Failed to parse Lisp-RPC string: {}", s))?;
        Ok(self.to_json(&data)?)
    }

    /// Convert named ExprData to a JSON object
    pub fn expr_data_to_json(&self, expr_data: &ExprData) -> Result<Value> {
        let data_name_key = self.get_data_name_key();
        let mut m = Map::with_capacity(expr_data.rest_args.len() + 1);

        // insert name
        m.insert(data_name_key.to_string(), expr_data.get_name().into());

        for (k, v) in expr_data {
            m.insert(k.to_string(), self.to_json(v)?);
        }

        Ok(m.into())
    }

    /// Convert list data to a JSON array
    pub fn list_data_to_json(&self, list_data: &ListData) -> Result<Value> {
        let mut arr = Vec::with_capacity(list_data.len());
        for item in list_data {
            arr.push(self.to_json(item)?);
        }
        Ok(Value::Array(arr))
    }

    /// Convert anonymous map data to a JSON object
    pub fn map_data_to_json(&self, map_data: &MapData) -> Result<Value> {
        let mut m = Map::with_capacity(map_data.len());
        for (k, v) in map_data {
            m.insert(k.to_string(), self.to_json(v)?);
        }
        Ok(m.into())
    }

    /// Convert scalar type values to a JSON value
    pub fn type_value_to_json(&self, type_value: &RawValue) -> Result<Value> {
        match type_value {
            RawValue::String(s) => Ok(Value::String(s.clone())),
            n @ RawValue::Number(_) => {
                if let Some(i) = n.to_int() {
                    Ok(Value::Number(i.into()))
                } else if let Some(f) = n.to_float() {
                    match serde_json::Number::from_f64(f) {
                        Some(num) => Ok(Value::Number(num)),
                        None => Ok(Value::Null),
                    }
                } else {
                    Err(serde_json::Error::custom(format!(
                        "cannot convert number {:?} to json",
                        n
                    )))
                }
            }
            RawValue::Keyword(k) => Ok(Value::String(k.clone())),
            RawValue::Symbol(s) => {
                if s.eq_ignore_ascii_case("t") || s.eq_ignore_ascii_case("true") {
                    Ok(Value::Bool(true))
                } else if s.eq_ignore_ascii_case("nil") {
                    Ok(Value::Null)
                } else if s.eq_ignore_ascii_case("false") {
                    Ok(Value::Bool(false))
                } else {
                    Ok(Value::String(s.clone()))
                }
            }
        }
    }

    /// Handle DataError by converting into a serde error
    pub fn data_error_to_json(&self, data_error: &DataError) -> Result<Value> {
        Err(serde_json::Error::custom(data_error.to_string()))
    }

    /// Convert a serde_json::Value to a Lisp-RPC Data structure
    pub fn from_json(&self, value: &Value) -> anyhow::Result<Data> {
        match value {
            Value::Null => Ok(Data::Value(TypeValue::Symbol("nil".to_string()))),
            Value::Bool(b) => {
                if *b {
                    Ok(Data::Value(TypeValue::Symbol("t".to_string())))
                } else {
                    Ok(Data::Value(TypeValue::Symbol("false".to_string())))
                }
            }
            Value::Number(num) => {
                if let Some(i) = num.as_i64() {
                    Ok(Data::Value(TypeValue::Number(TypeValueNumber::Int(i))))
                } else if let Some(u) = num.as_u64() {
                    if u <= i64::MAX as u64 {
                        Ok(Data::Value(TypeValue::Number(TypeValueNumber::Int(u as i64))))
                    } else {
                        Ok(Data::Value(TypeValue::Number(TypeValueNumber::Float(u as f64))))
                    }
                } else if let Some(f) = num.as_f64() {
                    Ok(Data::Value(TypeValue::Number(TypeValueNumber::Float(f))))
                } else {
                    anyhow::bail!("Cannot convert number {} to Lisp-RPC", num)
                }
            }
            Value::String(s) => Ok(Data::Value(TypeValue::String(s.clone()))),
            Value::Array(arr) => {
                let mut item_exprs = Vec::with_capacity(arr.len());
                for item in arr {
                    let item_data = self.from_json(item)?;
                    item_exprs.push(data_to_expr(&item_data)?);
                }
                let quote_expr = Expr::Quote(Box::new(Expr::List(item_exprs)));
                let list_data = ListData::from_expr(&quote_expr)
                    .with_context(|| "Failed to construct ListData from JSON array")?;
                Ok(Data::List(list_data))
            }
            Value::Object(map) => {
                let data_name_key = self.get_data_name_key();
                if let Some(name_val) = map.get(data_name_key) {
                    let name_str = match name_val {
                        Value::String(s) => s.as_str(),
                        other => anyhow::bail!(
                            "Expected '{}' value to be a string identifier, found: {:?}",
                            data_name_key,
                            other
                        ),
                    };
                    let mut rest_args = Vec::with_capacity(map.len().saturating_sub(1));
                    for (k, v) in map {
                        if k == data_name_key {
                            continue;
                        }
                        let v_data = self.from_json(v)?;
                        let k_expr = Expr::Atom(Atom {
                            value: TypeValue::Keyword(k.clone()),
                        });
                        rest_args.push((k_expr, v_data));
                    }
                    let expr_data = ExprData::new(name_str, rest_args.into_iter())
                        .with_context(|| format!("Failed to create ExprData '{}'", name_str))?;
                    Ok(Data::Data(expr_data))
                } else {
                    let mut kv_exprs = Vec::with_capacity(map.len() * 2);
                    for (k, v) in map {
                        let v_data = self.from_json(v)?;
                        let v_expr = data_to_expr(&v_data)?;
                        kv_exprs.push(Expr::Atom(Atom {
                            value: TypeValue::Keyword(k.clone()),
                        }));
                        kv_exprs.push(v_expr);
                    }
                    let quote_expr = Expr::Quote(Box::new(Expr::List(kv_exprs)));
                    let map_data = MapData::from_expr(&quote_expr)
                        .with_context(|| "Failed to construct MapData from JSON object")?;
                    Ok(Data::Map(map_data))
                }
            }
        }
    }

    /// Convert a JSON string to a Lisp-RPC Data
    pub fn from_json_str(&self, json_str: &str) -> anyhow::Result<Data> {
        let val: Value = serde_json::from_str(json_str)
            .with_context(|| format!("Failed to parse JSON string: {}", json_str))?;
        self.from_json(&val)
    }

    /// Convert a serde_json::Value to a Lisp-RPC S-expression string
    pub fn from_json_to_string(&self, value: &Value) -> anyhow::Result<String> {
        let data = self.from_json(value)?;
        Ok(data.to_string())
    }

    /// Convert a JSON string to a Lisp-RPC S-expression string
    pub fn from_json_str_to_string(&self, json_str: &str) -> anyhow::Result<String> {
        let data = self.from_json_str(json_str)?;
        Ok(data.to_string())
    }

    /// Convert a JSON object to a named ExprData
    pub fn json_to_expr_data(&self, value: &Value) -> anyhow::Result<ExprData> {
        match self.from_json(value)? {
            Data::Data(expr_data) => Ok(expr_data),
            _ => anyhow::bail!("Expected JSON object with data_name to convert to ExprData"),
        }
    }

    /// Convert a JSON object without data_name to an anonymous MapData
    pub fn json_to_map_data(&self, value: &Value) -> anyhow::Result<MapData> {
        match self.from_json(value)? {
            Data::Map(map_data) => Ok(map_data),
            _ => anyhow::bail!("Expected JSON object without data_name to convert to MapData"),
        }
    }

    /// Convert a JSON array to a ListData
    pub fn json_to_list_data(&self, value: &Value) -> anyhow::Result<ListData> {
        match self.from_json(value)? {
            Data::List(list_data) => Ok(list_data),
            _ => anyhow::bail!("Expected JSON array to convert to ListData"),
        }
    }
}

/// Standalone helper to convert Data using default settings
pub fn to_json(data: &Data) -> Result<Value> {
    JsonConvertor::default().to_json(data)
}

/// Standalone helper to convert ExprData using default settings
pub fn to_json_expr(expr_data: &ExprData) -> Result<Value> {
    JsonConvertor::default().expr_data_to_json(expr_data)
}

/// Standalone helper to convert Data to a compact JSON string
pub fn to_json_string(data: &Data) -> Result<String> {
    JsonConvertor::default().to_json_string(data)
}

/// Standalone helper to convert Data to a pretty-printed JSON string
pub fn to_json_string_pretty(data: &Data) -> Result<String> {
    JsonConvertor::default().to_json_string_pretty(data)
}

/// Standalone helper to parse a Lisp-RPC string and convert to JSON
pub fn to_json_from_str(s: &str) -> anyhow::Result<Value> {
    JsonConvertor::default().to_json_from_str(s)
}

/// Standalone helper to convert a serde_json::Value to Lisp-RPC Data
pub fn from_json(value: &Value) -> anyhow::Result<Data> {
    JsonConvertor::default().from_json(value)
}

/// Standalone helper to convert a JSON string to Lisp-RPC Data
pub fn from_json_str(json_str: &str) -> anyhow::Result<Data> {
    JsonConvertor::default().from_json_str(json_str)
}

/// Standalone helper to convert a serde_json::Value to a Lisp-RPC string
pub fn from_json_to_string(value: &Value) -> anyhow::Result<String> {
    JsonConvertor::default().from_json_to_string(value)
}

/// Standalone helper to convert a JSON string to a Lisp-RPC string
pub fn from_json_str_to_string(json_str: &str) -> anyhow::Result<String> {
    JsonConvertor::default().from_json_str_to_string(json_str)
}

/// Standalone helper to convert a serde_json::Value to Lisp-RPC Data (alias for `from_json`)
pub fn json_to_lisp_rpc(value: &Value) -> anyhow::Result<Data> {
    JsonConvertor::default().from_json(value)
}

/// Standalone helper to convert a serde_json::Value to a Lisp-RPC string (alias for `from_json_to_string`)
pub fn json_to_lisp_rpc_string(value: &Value) -> anyhow::Result<String> {
    JsonConvertor::default().from_json_to_string(value)
}

/// Extension trait to add `.to_json()` methods directly on Lisp-RPC data structures
pub trait LispRpcJsonExt {
    /// Convert to JSON using default JsonConvertor settings
    fn to_json(&self) -> Result<Value> {
        self.to_json_with(&JsonConvertor::default())
    }

    /// Convert to JSON using a custom JsonConvertor
    fn to_json_with(&self, convertor: &JsonConvertor) -> Result<Value>;
}

impl LispRpcJsonExt for Data {
    fn to_json_with(&self, convertor: &JsonConvertor) -> Result<Value> {
        convertor.to_json(self)
    }
}

impl LispRpcJsonExt for ExprData {
    fn to_json_with(&self, convertor: &JsonConvertor) -> Result<Value> {
        convertor.expr_data_to_json(self)
    }
}

impl LispRpcJsonExt for ListData {
    fn to_json_with(&self, convertor: &JsonConvertor) -> Result<Value> {
        convertor.list_data_to_json(self)
    }
}

impl LispRpcJsonExt for MapData {
    fn to_json_with(&self, convertor: &JsonConvertor) -> Result<Value> {
        convertor.map_data_to_json(self)
    }
}

impl LispRpcJsonExt for RawValue {
    fn to_json_with(&self, convertor: &JsonConvertor) -> Result<Value> {
        convertor.type_value_to_json(self)
    }
}

/// Extension trait for `serde_json::Value` to convert directly into Lisp-RPC structures
pub trait JsonLispRpcExt {
    /// Convert to Lisp-RPC Data using default JsonConvertor settings
    fn to_lisp_rpc(&self) -> anyhow::Result<Data> {
        self.to_lisp_rpc_with(&JsonConvertor::default())
    }

    /// Convert to Lisp-RPC Data using a custom JsonConvertor
    fn to_lisp_rpc_with(&self, convertor: &JsonConvertor) -> anyhow::Result<Data>;

    /// Convert to Lisp-RPC S-expression string using default JsonConvertor settings
    fn to_lisp_rpc_string(&self) -> anyhow::Result<String> {
        self.to_lisp_rpc_string_with(&JsonConvertor::default())
    }

    /// Convert to Lisp-RPC S-expression string using a custom JsonConvertor
    fn to_lisp_rpc_string_with(&self, convertor: &JsonConvertor) -> anyhow::Result<String>;
}

impl JsonLispRpcExt for Value {
    fn to_lisp_rpc_with(&self, convertor: &JsonConvertor) -> anyhow::Result<Data> {
        convertor.from_json(self)
    }

    fn to_lisp_rpc_string_with(&self, convertor: &JsonConvertor) -> anyhow::Result<String> {
        convertor.from_json_to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_named_data_conversion() {
        let convertor = JsonConvertor::new();
        let lisp_str = r#"(book-info :title "1984" :id 1 :available 'T)"#;
        let data = Data::from_root_str(lisp_str, None).expect("failed to parse root str");
        let json_val = convertor.to_json(&data).expect("failed to convert to json");

        assert_eq!(
            json_val,
            json!({
                "data_name": "book-info",
                "title": "1984",
                "id": 1,
                "available": true
            })
        );
    }

    #[test]
    fn test_custom_data_name_key() {
        let convertor = JsonConvertor::new().data_name_key("data-name");
        let lisp_str = r#"(user-info :name "Alice")"#;
        let data = Data::from_root_str(lisp_str, None).expect("failed to parse root str");
        let json_val = convertor.to_json(&data).expect("failed to convert to json");

        assert_eq!(
            json_val,
            json!({
                "data-name": "user-info",
                "name": "Alice"
            })
        );
    }

    #[test]
    fn test_nested_structures() {
        let convertor = JsonConvertor::new();
        let lisp_str =
            r#"(order :id 101 :tags '("sale" "urgent") :customer '(:name "Bob" :vip 'T))"#;
        let data = Data::from_root_str(lisp_str, None).expect("failed to parse root str");
        let json_val = convertor.to_json(&data).expect("failed to convert to json");

        assert_eq!(
            json_val,
            json!({
                "data_name": "order",
                "id": 101,
                "tags": ["sale", "urgent"],
                "customer": {
                    "name": "Bob",
                    "vip": true
                }
            })
        );
    }

    #[test]
    fn test_nested_data_in_data() {
        let convertor = JsonConvertor::new();
        let lisp_str = r#"(user :id 1 :device (device-info :os "macOS" :version 14))"#;
        let data = Data::from_root_str(lisp_str, None).expect("failed to parse root str");
        let json_val = convertor.to_json(&data).expect("failed to convert to json");

        assert_eq!(
            json_val,
            json!({
                "data_name": "user",
                "id": 1,
                "device": {
                    "data_name": "device-info",
                    "os": "macOS",
                    "version": 14
                }
            })
        );
    }

    #[test]
    fn test_anonymous_map() {
        let convertor = JsonConvertor::new();
        let lisp_str = r#"'(:name "Alice" :age 30 :score 98.5)"#;
        let map_data =
            MapData::from_str(&mut Default::default(), lisp_str).expect("failed to parse map");
        let data = Data::Map(map_data);
        let json_val = convertor.to_json(&data).expect("failed to convert map");

        assert_eq!(
            json_val,
            json!({
                "name": "Alice",
                "age": 30,
                "score": 98.5
            })
        );
    }

    #[test]
    fn test_list_data() {
        let convertor = JsonConvertor::new();
        let lisp_str = r#"'(1 2 3 "four" 'five)"#;
        let list_data =
            ListData::from_str(&mut Default::default(), lisp_str).expect("failed to parse list");
        let data = Data::List(list_data);
        let json_val = convertor.to_json(&data).expect("failed to convert list");

        assert_eq!(json_val, json!([1, 2, 3, "four", "five"]));
    }

    #[test]
    fn test_to_json_from_str_and_to_json_string() {
        let lisp_str = r#"(book :title "Dune" :year 1965)"#;
        let val = to_json_from_str(lisp_str).expect("failed to convert from str");
        assert_eq!(val["data_name"], "book");
        assert_eq!(val["title"], "Dune");
        assert_eq!(val["year"], 1965);

        let data = Data::from_root_str(lisp_str, None).unwrap();
        let json_str = to_json_string(&data).unwrap();
        assert_eq!(json_str, r#"{"data_name":"book","title":"Dune","year":1965}"#);

        let pretty_str = to_json_string_pretty(&data).unwrap();
        assert!(pretty_str.contains('\n'));
    }

    #[test]
    fn test_extension_trait() {
        let lisp_str = r#"(server :host "localhost" :port 8080)"#;
        let data = Data::from_root_str(lisp_str, None).unwrap();

        // Use extension trait directly on Data
        let val = data.to_json().unwrap();
        assert_eq!(val["data_name"], "server");
        assert_eq!(val["host"], "localhost");
        assert_eq!(val["port"], 8080);

        // Use extension trait with custom JsonConvertor
        let custom_convertor = JsonConvertor::new().data_name_key("_type");
        let custom_val = data.to_json_with(&custom_convertor).unwrap();
        assert_eq!(custom_val["_type"], "server");

        // Use extension trait directly on ExprData
        if let Data::Data(expr_data) = data {
            let expr_val = expr_data.to_json().unwrap();
            assert_eq!(expr_val["data_name"], "server");
        }
    }

    // --- Tests for JSON to Lisp-RPC conversion ---

    #[test]
    fn test_json_to_named_data() {
        let convertor = JsonConvertor::new();
        let json_val = json!({
            "data_name": "book-info",
            "title": "1984",
            "id": 1,
            "available": true
        });

        let data = convertor.from_json(&json_val).expect("failed to convert json to data");
        let lisp_str = data.to_string();

        assert_eq!(lisp_str, r#"(book-info :available t :id 1 :title "1984")"#);

        // Round-trip back to JSON
        let back_to_json = convertor.to_json(&data).expect("failed round-trip to json");
        assert_eq!(back_to_json, json_val);
    }

    #[test]
    fn test_json_to_custom_data_name_key() {
        let convertor = JsonConvertor::new().data_name_key("custom_type");
        let json_val = json!({
            "custom_type": "user",
            "name": "Alice",
            "active": true
        });

        let data = convertor.from_json(&json_val).unwrap();
        assert_eq!(data.to_string(), r#"(user :active t :name "Alice")"#);

        // Round-trip
        let back_json = convertor.to_json(&data).unwrap();
        assert_eq!(back_json, json_val);
    }

    #[test]
    fn test_json_to_anonymous_map() {
        let convertor = JsonConvertor::new();
        let json_val = json!({
            "name": "Alice",
            "age": 30,
            "score": 98.5
        });

        let data = convertor.from_json(&json_val).unwrap();
        assert_eq!(data.to_string(), r#"'(:age 30 :name "Alice" :score 98.5)"#);

        let back_json = convertor.to_json(&data).unwrap();
        assert_eq!(back_json, json_val);
    }

    #[test]
    fn test_json_to_list() {
        let convertor = JsonConvertor::new();
        let json_val = json!(["a", "b", 123, true]);

        let data = convertor.from_json(&json_val).unwrap();
        assert_eq!(data.to_string(), r#"'("a" "b" 123 t)"#);

        let back_json = convertor.to_json(&data).unwrap();
        assert_eq!(back_json, json_val);
    }

    #[test]
    fn test_json_to_list_of_named_data() {
        let convertor = JsonConvertor::new();
        let json_val = json!([
            {"data_name": "book", "id": 1},
            {"data_name": "book", "id": 2}
        ]);

        let data = convertor.from_json(&json_val).unwrap();
        assert_eq!(data.to_string(), r#"'((book :id 1) (book :id 2))"#);

        let back_json = convertor.to_json(&data).unwrap();
        assert_eq!(back_json, json_val);
    }

    #[test]
    fn test_json_to_nested_structures() {
        let convertor = JsonConvertor::new();
        let json_val = json!({
            "data_name": "order",
            "id": 101,
            "tags": ["sale", "urgent"],
            "customer": {
                "name": "Bob",
                "vip": true
            }
        });

        let data = convertor.from_json(&json_val).unwrap();
        assert_eq!(
            data.to_string(),
            r#"(order :customer '(:name "Bob" :vip t) :id 101 :tags '("sale" "urgent"))"#
        );

        let back_json = convertor.to_json(&data).unwrap();
        assert_eq!(back_json, json_val);
    }

    #[test]
    fn test_json_to_nested_data_in_data() {
        let convertor = JsonConvertor::new();
        let json_val = json!({
            "data_name": "user",
            "id": 1,
            "device": {
                "data_name": "device-info",
                "os": "macOS",
                "version": 14
            }
        });

        let data = convertor.from_json(&json_val).unwrap();
        assert_eq!(
            data.to_string(),
            r#"(user :device (device-info :os "macOS" :version 14) :id 1)"#
        );

        let back_json = convertor.to_json(&data).unwrap();
        assert_eq!(back_json, json_val);
    }

    #[test]
    fn test_json_scalars() {
        let convertor = JsonConvertor::new();

        let s_val = json!("hello");
        let data = convertor.from_json(&s_val).unwrap();
        assert_eq!(data.to_string(), r#""hello""#);

        let n_val = json!(42);
        let data = convertor.from_json(&n_val).unwrap();
        assert_eq!(data.to_string(), "42");

        let f_val = json!(3.14);
        let data = convertor.from_json(&f_val).unwrap();
        assert_eq!(data.to_string(), "3.14");

        let b_val = json!(true);
        let data = convertor.from_json(&b_val).unwrap();
        assert_eq!(data.to_string(), "t");

        let false_val = json!(false);
        let data = convertor.from_json(&false_val).unwrap();
        assert_eq!(data.to_string(), "false");

        let null_val = json!(null);
        let data = convertor.from_json(&null_val).unwrap();
        assert_eq!(data.to_string(), "nil");
    }

    #[test]
    fn test_from_json_str_and_from_json_to_string() {
        let json_str = r#"{"data_name":"book","title":"Dune","year":1965}"#;
        let lisp_str = from_json_str_to_string(json_str).unwrap();
        assert_eq!(lisp_str, r#"(book :title "Dune" :year 1965)"#);

        let data = from_json_str(json_str).unwrap();
        assert_eq!(data.to_string(), r#"(book :title "Dune" :year 1965)"#);
    }

    #[test]
    fn test_json_extension_trait() {
        let json_val = json!({
            "data_name": "server",
            "host": "localhost",
            "port": 8080
        });

        // Use extension trait directly on Value
        let lisp_str = json_val.to_lisp_rpc_string().unwrap();
        assert_eq!(lisp_str, r#"(server :host "localhost" :port 8080)"#);

        let data = json_val.to_lisp_rpc().unwrap();
        assert_eq!(data.to_string(), r#"(server :host "localhost" :port 8080)"#);

        // With custom convertor
        let custom_convertor = JsonConvertor::new().data_name_key("_type");
        let custom_json = json!({
            "_type": "server",
            "host": "localhost"
        });
        let custom_str = custom_json.to_lisp_rpc_string_with(&custom_convertor).unwrap();
        assert_eq!(custom_str, r#"(server :host "localhost")"#);
    }
}
