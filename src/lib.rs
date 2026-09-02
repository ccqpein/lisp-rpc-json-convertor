pub mod json;
pub mod yaml;

use lisp_rpc_rust_parser::{Atom, Expr, TypeValue};
use lisp_rpc_rust_raw_data::Data;

/// Helper function to convert a Data structure into an Expr AST
pub fn data_to_expr(data: &Data) -> anyhow::Result<Expr> {
    match data {
        Data::Data(expr_data) => {
            let mut items = Vec::with_capacity(expr_data.rest_args.len() * 2 + 1);
            items.push(Expr::Atom(Atom {
                value: TypeValue::Symbol(expr_data.get_name().to_string()),
            }));
            for (k_expr, v_data) in &expr_data.rest_args {
                items.push(k_expr.clone());
                items.push(data_to_expr(v_data)?);
            }
            Ok(Expr::List(items))
        }
        Data::List(list_data) => {
            let mut items = Vec::with_capacity(list_data.len());
            for item in list_data {
                items.push(data_to_expr(item)?);
            }
            Ok(Expr::Quote(Box::new(Expr::List(items))))
        }
        Data::Map(map_data) => {
            let mut items = Vec::with_capacity(map_data.len() * 2);
            for (k, v) in map_data {
                items.push(Expr::Atom(Atom {
                    value: TypeValue::Keyword(k.clone()),
                }));
                items.push(data_to_expr(v)?);
            }
            Ok(Expr::Quote(Box::new(Expr::List(items))))
        }
        Data::Value(type_value) => match type_value {
            TypeValue::Symbol(s) if s.eq_ignore_ascii_case("t") => Ok(Expr::Atom(Atom {
                value: TypeValue::Symbol(s.clone()),
            })),
            TypeValue::Symbol(s) => Ok(Expr::Quote(Box::new(Expr::Atom(Atom {
                value: TypeValue::Symbol(s.clone()),
            })))),
            _ => Ok(Expr::Atom(Atom {
                value: type_value.clone(),
            })),
        },
        Data::Error(data_error) => {
            anyhow::bail!("Cannot convert Data::Error to Expr: {}", data_error)
        }
    }
}

// Re-exports for JSON
pub use json::{
    from_json, from_json_str, from_json_str_to_string, from_json_to_string, json_to_lisp_rpc,
    json_to_lisp_rpc_string, to_json, to_json_expr, to_json_from_str, to_json_string,
    to_json_string_pretty, Convertor, JsonConvertor, JsonLispRpcExt, LispRpcJsonExt,
};

// Re-exports for YAML
pub use yaml::{
    from_yaml, from_yaml_str, from_yaml_str_to_string, from_yaml_to_string, to_yaml,
    to_yaml_expr, to_yaml_from_str, to_yaml_string, yaml_to_lisp_rpc, yaml_to_lisp_rpc_string,
    LispRpcYamlExt, YamlConvertor, YamlLispRpcExt,
};
