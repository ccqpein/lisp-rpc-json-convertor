use anyhow::Context;
use lisp_rpc_rust_parser::{Atom, Expr, TypeValue, TypeValueNumber};
use lisp_rpc_rust_raw_data::{
    Data, DataError, ExprData, FromStr, ListData, MapData, Value as RawValue,
};
use serde_yaml::{Mapping, Number, Value};

use crate::data_to_expr;

#[derive(Debug, Clone, Default)]
pub struct YamlConvertor {
    data_name_key: Option<String>,
}

impl YamlConvertor {
    pub fn new() -> Self {
        Self {
            data_name_key: None,
        }
    }

    /// Set the key of data_name for this YAML object (defaults to "data_name")
    pub fn data_name_key(mut self, data_name_key: impl Into<String>) -> Self {
        self.data_name_key = Some(data_name_key.into());
        self
    }

    /// Returns the current data name key configured on this Convertor (defaults to "data_name")
    pub fn get_data_name_key(&self) -> &str {
        self.data_name_key.as_deref().unwrap_or("data_name")
    }

    /// Convert any Data variant to a serde_yaml::Value
    pub fn to_yaml(&self, data: &Data) -> Result<Value, serde_yaml::Error> {
        match data {
            Data::Data(expr_data) => self.expr_data_to_yaml(expr_data),
            Data::List(list_data) => self.list_data_to_yaml(list_data),
            Data::Map(map_data) => self.map_data_to_yaml(map_data),
            Data::Value(type_value) => self.type_value_to_yaml(type_value),
            Data::Error(data_error) => self.data_error_to_yaml(data_error),
        }
    }

    /// Convert Data to a YAML string
    pub fn to_yaml_string(&self, data: &Data) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(&self.to_yaml(data)?)
    }

    /// Parse a Lisp-RPC data string and convert it to a serde_yaml::Value
    pub fn to_yaml_from_str(&self, s: &str) -> anyhow::Result<Value> {
        let mut parser = Default::default();
        let data = Data::from_str(&mut parser, s)
            .with_context(|| format!("Failed to parse Lisp-RPC string: {}", s))?;
        Ok(self.to_yaml(&data)?)
    }

    /// Convert named ExprData to a YAML mapping
    pub fn expr_data_to_yaml(&self, expr_data: &ExprData) -> Result<Value, serde_yaml::Error> {
        let data_name_key = self.get_data_name_key();
        let mut m = Mapping::with_capacity(expr_data.rest_args.len() + 1);

        // insert name
        m.insert(
            Value::String(data_name_key.to_string()),
            Value::String(expr_data.get_name().to_string()),
        );

        for (k, v) in expr_data {
            m.insert(Value::String(k.to_string()), self.to_yaml(v)?);
        }

        Ok(Value::Mapping(m))
    }

    /// Convert list data to a YAML sequence
    pub fn list_data_to_yaml(&self, list_data: &ListData) -> Result<Value, serde_yaml::Error> {
        let mut arr = Vec::with_capacity(list_data.len());
        for item in list_data {
            arr.push(self.to_yaml(item)?);
        }
        Ok(Value::Sequence(arr))
    }

    /// Convert anonymous map data to a YAML mapping
    pub fn map_data_to_yaml(&self, map_data: &MapData) -> Result<Value, serde_yaml::Error> {
        let mut m = Mapping::with_capacity(map_data.len());
        for (k, v) in map_data {
            m.insert(Value::String(k.to_string()), self.to_yaml(v)?);
        }
        Ok(Value::Mapping(m))
    }

    /// Convert scalar type values to a YAML value
    pub fn type_value_to_yaml(&self, type_value: &RawValue) -> Result<Value, serde_yaml::Error> {
        match type_value {
            RawValue::String(s) => Ok(Value::String(s.clone())),
            n @ RawValue::Number(_) => {
                if let Some(i) = n.to_int() {
                    Ok(Value::Number(Number::from(i)))
                } else if let Some(f) = n.to_float() {
                    Ok(Value::Number(Number::from(f)))
                } else {
                    Err(serde::ser::Error::custom(format!(
                        "cannot convert number {:?} to yaml",
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
    pub fn data_error_to_yaml(&self, data_error: &DataError) -> Result<Value, serde_yaml::Error> {
        Err(serde::ser::Error::custom(data_error.to_string()))
    }

    /// Convert a serde_yaml::Value to a Lisp-RPC Data structure
    pub fn from_yaml(&self, value: &Value) -> anyhow::Result<Data> {
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
                    anyhow::bail!("Cannot convert number {:?} to Lisp-RPC", num)
                }
            }
            Value::String(s) => Ok(Data::Value(TypeValue::String(s.clone()))),
            Value::Sequence(seq) => {
                let mut item_exprs = Vec::with_capacity(seq.len());
                for item in seq {
                    let item_data = self.from_yaml(item)?;
                    item_exprs.push(data_to_expr(&item_data)?);
                }
                let quote_expr = Expr::Quote(Box::new(Expr::List(item_exprs)));
                let list_data = ListData::from_expr(&quote_expr)
                    .with_context(|| "Failed to construct ListData from YAML sequence")?;
                Ok(Data::List(list_data))
            }
            Value::Mapping(map) => {
                let data_name_key = self.get_data_name_key();
                let data_name_val_key = Value::String(data_name_key.to_string());
                if let Some(name_val) = map.get(&data_name_val_key) {
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
                        if k == &data_name_val_key {
                            continue;
                        }
                        let k_str = match k {
                            Value::String(s) => s.as_str(),
                            other => anyhow::bail!(
                                "Expected mapping key to be a string, found: {:?}",
                                other
                            ),
                        };
                        let v_data = self.from_yaml(v)?;
                        let k_expr = Expr::Atom(Atom {
                            value: TypeValue::Keyword(k_str.to_string()),
                        });
                        rest_args.push((k_expr, v_data));
                    }
                    let expr_data = ExprData::new(name_str, rest_args.into_iter())
                        .with_context(|| format!("Failed to create ExprData '{}'", name_str))?;
                    Ok(Data::Data(expr_data))
                } else {
                    let mut kv_exprs = Vec::with_capacity(map.len() * 2);
                    for (k, v) in map {
                        let k_str = match k {
                            Value::String(s) => s.as_str(),
                            other => anyhow::bail!(
                                "Expected mapping key to be a string, found: {:?}",
                                other
                            ),
                        };
                        let v_data = self.from_yaml(v)?;
                        let v_expr = data_to_expr(&v_data)?;
                        kv_exprs.push(Expr::Atom(Atom {
                            value: TypeValue::Keyword(k_str.to_string()),
                        }));
                        kv_exprs.push(v_expr);
                    }
                    let quote_expr = Expr::Quote(Box::new(Expr::List(kv_exprs)));
                    let map_data = MapData::from_expr(&quote_expr)
                        .with_context(|| "Failed to construct MapData from YAML mapping")?;
                    Ok(Data::Map(map_data))
                }
            }
            Value::Tagged(tagged) => self.from_yaml(&tagged.value),
        }
    }

    /// Convert a YAML string to a Lisp-RPC Data
    pub fn from_yaml_str(&self, yaml_str: &str) -> anyhow::Result<Data> {
        let val: Value = serde_yaml::from_str(yaml_str)
            .with_context(|| format!("Failed to parse YAML string: {}", yaml_str))?;
        self.from_yaml(&val)
    }

    /// Convert a serde_yaml::Value to a Lisp-RPC S-expression string
    pub fn from_yaml_to_string(&self, value: &Value) -> anyhow::Result<String> {
        let data = self.from_yaml(value)?;
        Ok(data.to_string())
    }

    /// Convert a YAML string to a Lisp-RPC S-expression string
    pub fn from_yaml_str_to_string(&self, yaml_str: &str) -> anyhow::Result<String> {
        let data = self.from_yaml_str(yaml_str)?;
        Ok(data.to_string())
    }

    /// Convert a YAML mapping to a named ExprData
    pub fn yaml_to_expr_data(&self, value: &Value) -> anyhow::Result<ExprData> {
        match self.from_yaml(value)? {
            Data::Data(expr_data) => Ok(expr_data),
            _ => anyhow::bail!("Expected YAML mapping with data_name to convert to ExprData"),
        }
    }

    /// Convert a YAML mapping without data_name to an anonymous MapData
    pub fn yaml_to_map_data(&self, value: &Value) -> anyhow::Result<MapData> {
        match self.from_yaml(value)? {
            Data::Map(map_data) => Ok(map_data),
            _ => anyhow::bail!("Expected YAML mapping without data_name to convert to MapData"),
        }
    }

    /// Convert a YAML sequence to a ListData
    pub fn yaml_to_list_data(&self, value: &Value) -> anyhow::Result<ListData> {
        match self.from_yaml(value)? {
            Data::List(list_data) => Ok(list_data),
            _ => anyhow::bail!("Expected YAML sequence to convert to ListData"),
        }
    }
}

/// Standalone helper to convert Data using default settings
pub fn to_yaml(data: &Data) -> Result<Value, serde_yaml::Error> {
    YamlConvertor::default().to_yaml(data)
}

/// Standalone helper to convert ExprData using default settings
pub fn to_yaml_expr(expr_data: &ExprData) -> Result<Value, serde_yaml::Error> {
    YamlConvertor::default().expr_data_to_yaml(expr_data)
}

/// Standalone helper to convert Data to a YAML string
pub fn to_yaml_string(data: &Data) -> Result<String, serde_yaml::Error> {
    YamlConvertor::default().to_yaml_string(data)
}

/// Standalone helper to parse a Lisp-RPC string and convert to YAML
pub fn to_yaml_from_str(s: &str) -> anyhow::Result<Value> {
    YamlConvertor::default().to_yaml_from_str(s)
}

/// Standalone helper to convert a serde_yaml::Value to Lisp-RPC Data
pub fn from_yaml(value: &Value) -> anyhow::Result<Data> {
    YamlConvertor::default().from_yaml(value)
}

/// Standalone helper to convert a YAML string to Lisp-RPC Data
pub fn from_yaml_str(yaml_str: &str) -> anyhow::Result<Data> {
    YamlConvertor::default().from_yaml_str(yaml_str)
}

/// Standalone helper to convert a serde_yaml::Value to a Lisp-RPC string
pub fn from_yaml_to_string(value: &Value) -> anyhow::Result<String> {
    YamlConvertor::default().from_yaml_to_string(value)
}

/// Standalone helper to convert a YAML string to a Lisp-RPC string
pub fn from_yaml_str_to_string(yaml_str: &str) -> anyhow::Result<String> {
    YamlConvertor::default().from_yaml_str_to_string(yaml_str)
}

/// Standalone helper to convert a serde_yaml::Value to Lisp-RPC Data (alias for `from_yaml`)
pub fn yaml_to_lisp_rpc(value: &Value) -> anyhow::Result<Data> {
    YamlConvertor::default().from_yaml(value)
}

/// Standalone helper to convert a serde_yaml::Value to a Lisp-RPC string (alias for `from_yaml_to_string`)
pub fn yaml_to_lisp_rpc_string(value: &Value) -> anyhow::Result<String> {
    YamlConvertor::default().from_yaml_to_string(value)
}

/// Extension trait to add `.to_yaml()` methods directly on Lisp-RPC data structures
pub trait LispRpcYamlExt {
    /// Convert to YAML using default YamlConvertor settings
    fn to_yaml(&self) -> Result<Value, serde_yaml::Error> {
        self.to_yaml_with(&YamlConvertor::default())
    }

    /// Convert to YAML using a custom YamlConvertor
    fn to_yaml_with(&self, convertor: &YamlConvertor) -> Result<Value, serde_yaml::Error>;
}

impl LispRpcYamlExt for Data {
    fn to_yaml_with(&self, convertor: &YamlConvertor) -> Result<Value, serde_yaml::Error> {
        convertor.to_yaml(self)
    }
}

impl LispRpcYamlExt for ExprData {
    fn to_yaml_with(&self, convertor: &YamlConvertor) -> Result<Value, serde_yaml::Error> {
        convertor.expr_data_to_yaml(self)
    }
}

impl LispRpcYamlExt for ListData {
    fn to_yaml_with(&self, convertor: &YamlConvertor) -> Result<Value, serde_yaml::Error> {
        convertor.list_data_to_yaml(self)
    }
}

impl LispRpcYamlExt for MapData {
    fn to_yaml_with(&self, convertor: &YamlConvertor) -> Result<Value, serde_yaml::Error> {
        convertor.map_data_to_yaml(self)
    }
}

impl LispRpcYamlExt for RawValue {
    fn to_yaml_with(&self, convertor: &YamlConvertor) -> Result<Value, serde_yaml::Error> {
        convertor.type_value_to_yaml(self)
    }
}

/// Extension trait for `serde_yaml::Value` to convert directly into Lisp-RPC structures
pub trait YamlLispRpcExt {
    /// Convert to Lisp-RPC Data using default YamlConvertor settings
    fn to_lisp_rpc(&self) -> anyhow::Result<Data> {
        self.to_lisp_rpc_with(&YamlConvertor::default())
    }

    /// Convert to Lisp-RPC Data using a custom YamlConvertor
    fn to_lisp_rpc_with(&self, convertor: &YamlConvertor) -> anyhow::Result<Data>;

    /// Convert to Lisp-RPC S-expression string using default YamlConvertor settings
    fn to_lisp_rpc_string(&self) -> anyhow::Result<String> {
        self.to_lisp_rpc_string_with(&YamlConvertor::default())
    }

    /// Convert to Lisp-RPC S-expression string using a custom YamlConvertor
    fn to_lisp_rpc_string_with(&self, convertor: &YamlConvertor) -> anyhow::Result<String>;
}

impl YamlLispRpcExt for Value {
    fn to_lisp_rpc_with(&self, convertor: &YamlConvertor) -> anyhow::Result<Data> {
        convertor.from_yaml(self)
    }

    fn to_lisp_rpc_string_with(&self, convertor: &YamlConvertor) -> anyhow::Result<String> {
        convertor.from_yaml_to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_named_data_conversion() {
        let convertor = YamlConvertor::new();
        let lisp_str = r#"(book-info :title "1984" :id 1 :available 'T)"#;
        let data = Data::from_root_str(lisp_str, None).expect("failed to parse root str");
        let yaml_val = convertor.to_yaml(&data).expect("failed to convert to yaml");

        assert_eq!(yaml_val["data_name"], Value::String("book-info".to_string()));
        assert_eq!(yaml_val["title"], Value::String("1984".to_string()));
        assert_eq!(yaml_val["id"], Value::Number(1.into()));
        assert_eq!(yaml_val["available"], Value::Bool(true));
    }

    #[test]
    fn test_yaml_custom_data_name_key() {
        let convertor = YamlConvertor::new().data_name_key("type");
        let lisp_str = r#"(user :name "Alice")"#;
        let data = Data::from_root_str(lisp_str, None).unwrap();
        let yaml_val = convertor.to_yaml(&data).unwrap();

        assert_eq!(yaml_val["type"], Value::String("user".to_string()));
        assert_eq!(yaml_val["name"], Value::String("Alice".to_string()));
    }

    #[test]
    fn test_yaml_anonymous_map() {
        let convertor = YamlConvertor::new();
        let lisp_str = r#"'(:name "Alice" :age 30 :score 98.5)"#;
        let map_data =
            MapData::from_str(&mut Default::default(), lisp_str).expect("failed to parse map");
        let data = Data::Map(map_data);
        let yaml_val = convertor.to_yaml(&data).expect("failed to convert map");

        assert_eq!(yaml_val["name"], Value::String("Alice".to_string()));
        assert_eq!(yaml_val["age"], Value::Number(30.into()));
    }

    #[test]
    fn test_yaml_list() {
        let convertor = YamlConvertor::new();
        let lisp_str = r#"'(1 2 3 "four" 'five)"#;
        let list_data =
            ListData::from_str(&mut Default::default(), lisp_str).expect("failed to parse list");
        let data = Data::List(list_data);
        let yaml_val = convertor.to_yaml(&data).expect("failed to convert list");

        if let Value::Sequence(seq) = yaml_val {
            assert_eq!(seq.len(), 5);
            assert_eq!(seq[0], Value::Number(1.into()));
            assert_eq!(seq[3], Value::String("four".to_string()));
        } else {
            panic!("Expected sequence");
        }
    }

    #[test]
    fn test_from_yaml_named_data() {
        let convertor = YamlConvertor::new();
        let yaml_str = r#"
data_name: book-info
title: "1984"
id: 1
available: true
"#;
        let data = convertor.from_yaml_str(yaml_str).unwrap();
        let back_yaml = convertor.to_yaml(&data).unwrap();

        assert_eq!(back_yaml["data_name"], Value::String("book-info".to_string()));
        assert_eq!(back_yaml["title"], Value::String("1984".to_string()));
        assert_eq!(back_yaml["id"], Value::Number(1.into()));
        assert_eq!(back_yaml["available"], Value::Bool(true));
    }

    #[test]
    fn test_from_yaml_custom_key() {
        let convertor = YamlConvertor::new().data_name_key("_type");
        let yaml_str = r#"
_type: user
name: Alice
active: true
"#;
        let data = convertor.from_yaml_str(yaml_str).unwrap();
        assert_eq!(data.to_string(), r#"(user :name "Alice" :active t)"#);
    }

    #[test]
    fn test_from_yaml_anonymous_map() {
        let convertor = YamlConvertor::new();
        let yaml_str = r#"
first: John
last: Doe
"#;
        let data = convertor.from_yaml_str(yaml_str).unwrap();
        assert_eq!(data.to_string(), r#"'(:first "John" :last "Doe")"#);
    }

    #[test]
    fn test_from_yaml_sequence() {
        let convertor = YamlConvertor::new();
        let yaml_str = r#"
- a
- b
- 123
- true
"#;
        let data = convertor.from_yaml_str(yaml_str).unwrap();
        assert_eq!(data.to_string(), r#"'("a" "b" 123 t)"#);
    }

    #[test]
    fn test_from_yaml_sequence_of_named_data() {
        let convertor = YamlConvertor::new();
        let yaml_str = r#"
- data_name: book
  id: 1
- data_name: book
  id: 2
"#;
        let data = convertor.from_yaml_str(yaml_str).unwrap();
        assert_eq!(data.to_string(), r#"'((book :id 1) (book :id 2))"#);
    }

    #[test]
    fn test_yaml_scalars() {
        let convertor = YamlConvertor::new();

        let s_val = Value::String("hello".to_string());
        assert_eq!(convertor.from_yaml(&s_val).unwrap().to_string(), r#""hello""#);

        let n_val = Value::Number(42.into());
        assert_eq!(convertor.from_yaml(&n_val).unwrap().to_string(), "42");

        let b_val = Value::Bool(true);
        assert_eq!(convertor.from_yaml(&b_val).unwrap().to_string(), "t");

        let f_val = Value::Bool(false);
        assert_eq!(convertor.from_yaml(&f_val).unwrap().to_string(), "false");

        let null_val = Value::Null;
        assert_eq!(convertor.from_yaml(&null_val).unwrap().to_string(), "nil");
    }

    #[test]
    fn test_yaml_extension_trait() {
        let yaml_val: Value = serde_yaml::from_str("data_name: server\nhost: localhost\nport: 8080").unwrap();

        let lisp_str = yaml_val.to_lisp_rpc_string().unwrap();
        assert_eq!(lisp_str, r#"(server :host "localhost" :port 8080)"#);

        let data = yaml_val.to_lisp_rpc().unwrap();
        assert_eq!(data.to_string(), r#"(server :host "localhost" :port 8080)"#);
    }
}
