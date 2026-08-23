use anyhow::Context;
use lisp_rpc_rust_raw_data::{Data, DataError, ExprData, FromStr, ListData, MapData, Value as RawValue};
use serde::ser::Error as _;
use serde_json::{Map, Result, Value};

#[derive(Debug, Clone, Default)]
pub struct Convertor {
    data_name_key: Option<String>,
}

impl Convertor {
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
        let data_name_key = self.data_name_key.as_deref().unwrap_or("data_name");
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
}

/// Standalone helper to convert Data using default settings
pub fn to_json(data: &Data) -> Result<Value> {
    Convertor::default().to_json(data)
}

/// Standalone helper to convert ExprData using default settings
pub fn to_json_expr(expr_data: &ExprData) -> Result<Value> {
    Convertor::default().expr_data_to_json(expr_data)
}

/// Standalone helper to convert Data to a compact JSON string
pub fn to_json_string(data: &Data) -> Result<String> {
    Convertor::default().to_json_string(data)
}

/// Standalone helper to convert Data to a pretty-printed JSON string
pub fn to_json_string_pretty(data: &Data) -> Result<String> {
    Convertor::default().to_json_string_pretty(data)
}

/// Standalone helper to parse a Lisp-RPC string and convert to JSON
pub fn to_json_from_str(s: &str) -> anyhow::Result<Value> {
    Convertor::default().to_json_from_str(s)
}

/// Extension trait to add `.to_json()` methods directly on Lisp-RPC data structures
pub trait LispRpcJsonExt {
    /// Convert to JSON using default Convertor settings
    fn to_json(&self) -> Result<Value> {
        self.to_json_with(&Convertor::default())
    }

    /// Convert to JSON using a custom Convertor
    fn to_json_with(&self, convertor: &Convertor) -> Result<Value>;
}

impl LispRpcJsonExt for Data {
    fn to_json_with(&self, convertor: &Convertor) -> Result<Value> {
        convertor.to_json(self)
    }
}

impl LispRpcJsonExt for ExprData {
    fn to_json_with(&self, convertor: &Convertor) -> Result<Value> {
        convertor.expr_data_to_json(self)
    }
}

impl LispRpcJsonExt for ListData {
    fn to_json_with(&self, convertor: &Convertor) -> Result<Value> {
        convertor.list_data_to_json(self)
    }
}

impl LispRpcJsonExt for MapData {
    fn to_json_with(&self, convertor: &Convertor) -> Result<Value> {
        convertor.map_data_to_json(self)
    }
}

impl LispRpcJsonExt for RawValue {
    fn to_json_with(&self, convertor: &Convertor) -> Result<Value> {
        convertor.type_value_to_json(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_named_data_conversion() {
        let convertor = Convertor::new();
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
        let convertor = Convertor::new().data_name_key("data-name");
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
        let convertor = Convertor::new();
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
        let convertor = Convertor::new();
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
        let convertor = Convertor::new();
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
        let convertor = Convertor::new();
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

        // Use extension trait with custom Convertor
        let custom_convertor = Convertor::new().data_name_key("_type");
        let custom_val = data.to_json_with(&custom_convertor).unwrap();
        assert_eq!(custom_val["_type"], "server");

        // Use extension trait directly on ExprData
        if let Data::Data(expr_data) = data {
            let expr_val = expr_data.to_json().unwrap();
            assert_eq!(expr_val["data_name"], "server");
        }
    }
}
