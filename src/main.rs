use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;
use lisp_rpc_json_convertor::{Convertor, YamlConvertor};
use lisp_rpc_rust_raw_data::files::DataFile;
use lisp_rpc_rust_raw_data::{Data, FromStr};
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

#[derive(Parser, Debug)]
#[command(
    name = "lisp-rpc-json-convertor",
    author,
    version,
    about = "Convert between Lisp-RPC data and JSON / YAML"
)]
struct Args {
    /// Raw Lisp-RPC data string directly passed in (converts Lisp-RPC -> JSON/YAML)
    #[arg(
        short = 'd',
        long = "data",
        value_name = "DATA",
        allow_hyphen_values = true,
        conflicts_with_all = ["file", "json", "yaml"]
    )]
    data: Option<String>,

    /// Raw JSON data string directly passed in (converts JSON -> Lisp-RPC)
    #[arg(
        short = 'j',
        long = "json",
        value_name = "JSON",
        allow_hyphen_values = true,
        conflicts_with_all = ["data", "file", "yaml"]
    )]
    json: Option<String>,

    /// Raw YAML data string directly passed in (converts YAML -> Lisp-RPC)
    #[arg(
        short = 'y',
        long = "yaml",
        value_name = "YAML",
        allow_hyphen_values = true,
        conflicts_with_all = ["data", "file", "json"]
    )]
    yaml: Option<String>,

    /// Path to a file containing Lisp-RPC, JSON, or YAML data (use "-" for stdin)
    #[arg(
        short = 'f',
        long = "file",
        value_name = "FILE",
        conflicts_with_all = ["data", "json", "yaml"]
    )]
    file: Option<PathBuf>,

    /// Output file path (defaults to stdout)
    #[arg(short = 'o', long = "output", value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// Custom key name for the data type name (defaults to "data_name")
    #[arg(short = 'k', long = "key", value_name = "KEY")]
    data_name_key: Option<String>,

    /// Output pretty-formatted JSON with indentation (when converting to JSON)
    #[arg(short = 'p', long = "pretty", default_value_t = false)]
    pretty: bool,

    /// Positional input (raw Lisp-RPC / JSON / YAML data string, file path, or "-" for stdin)
    #[arg(
        value_name = "INPUT",
        allow_hyphen_values = true,
        conflicts_with_all = ["data", "json", "yaml", "file"]
    )]
    input: Option<String>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let mut json_convertor = Convertor::new();
    let mut yaml_convertor = YamlConvertor::new();
    if let Some(ref key) = args.data_name_key {
        json_convertor = json_convertor.data_name_key(key.as_str());
        yaml_convertor = yaml_convertor.data_name_key(key.as_str());
    }

    let to_yaml_out = args
        .output
        .as_ref()
        .and_then(|p| p.extension())
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"))
        .unwrap_or(false);

    let result_str = process_input(&args, &json_convertor, &yaml_convertor, to_yaml_out)?;

    if let Some(out_path) = args.output {
        fs::write(&out_path, &result_str)
            .with_context(|| format!("Failed to write output to {:?}", out_path))?;
    } else {
        println!("{}", result_str);
    }

    Ok(())
}

fn process_input(
    args: &Args,
    json_convertor: &Convertor,
    yaml_convertor: &YamlConvertor,
    to_yaml_out: bool,
) -> Result<String> {
    if let Some(ref json_str) = args.json {
        let val: JsonValue = serde_json::from_str(json_str)
            .with_context(|| format!("Failed to parse JSON string: {}", json_str))?;
        convert_json_value(&val, json_convertor)
    } else if let Some(ref yaml_str) = args.yaml {
        let val: YamlValue = serde_yaml::from_str(yaml_str)
            .with_context(|| format!("Failed to parse YAML string: {}", yaml_str))?;
        convert_yaml_value(&val, yaml_convertor)
    } else if let Some(ref data_str) = args.data {
        convert_lisp_from_string(data_str, json_convertor, yaml_convertor, args.pretty, to_yaml_out)
    } else if let Some(ref file_path) = args.file {
        if file_path.to_str() == Some("-") {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;
            convert_auto(&buffer, json_convertor, yaml_convertor, args.pretty, to_yaml_out)
        } else {
            convert_from_file_path(file_path, json_convertor, yaml_convertor, args.pretty, to_yaml_out)
        }
    } else if let Some(ref input_str) = args.input {
        if input_str == "-" {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;
            convert_auto(&buffer, json_convertor, yaml_convertor, args.pretty, to_yaml_out)
        } else {
            let path = Path::new(input_str);
            if path.exists() && path.is_file() {
                convert_from_file_path(path, json_convertor, yaml_convertor, args.pretty, to_yaml_out)
            } else {
                convert_auto(input_str, json_convertor, yaml_convertor, args.pretty, to_yaml_out)
            }
        }
    } else if !io::stdin().is_terminal() {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        convert_auto(&buffer, json_convertor, yaml_convertor, args.pretty, to_yaml_out)
    } else {
        bail!("No input provided. Pass data via -d/--data, -j/--json, -y/--yaml, -f/--file, positional argument, or pipe via stdin.\nUse --help for more information.");
    }
}

/// Convert file path depending on file extension or content
fn convert_from_file_path(
    path: &Path,
    json_convertor: &Convertor,
    yaml_convertor: &YamlConvertor,
    pretty: bool,
    to_yaml_out: bool,
) -> Result<String> {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("json") {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read JSON file at {:?}", path))?;
            let val: JsonValue = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse JSON file at {:?}", path))?;
            return convert_json_value(&val, json_convertor);
        } else if ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml") {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read YAML file at {:?}", path))?;
            let val: YamlValue = serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse YAML file at {:?}", path))?;
            return convert_yaml_value(&val, yaml_convertor);
        } else if ext.eq_ignore_ascii_case("lisp")
            || ext.eq_ignore_ascii_case("lsp")
            || ext.eq_ignore_ascii_case("scm")
            || ext.eq_ignore_ascii_case("cl")
        {
            return convert_lisp_from_file(path, json_convertor, yaml_convertor, pretty, to_yaml_out);
        }
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file at {:?}", path))?;
    convert_auto(&content, json_convertor, yaml_convertor, pretty, to_yaml_out)
}

/// Automatically detect whether the content is JSON, YAML, or Lisp-RPC and convert accordingly
fn convert_auto(
    content: &str,
    json_convertor: &Convertor,
    yaml_convertor: &YamlConvertor,
    pretty: bool,
    to_yaml_out: bool,
) -> Result<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(val) = serde_json::from_str::<JsonValue>(trimmed) {
            return convert_json_value(&val, json_convertor);
        }
    }

    if trimmed.starts_with("---") {
        if let Ok(val) = serde_yaml::from_str::<YamlValue>(trimmed) {
            return convert_yaml_value(&val, yaml_convertor);
        }
    }

    if trimmed.starts_with('(') || trimmed.starts_with('\'') || trimmed.starts_with(';') {
        return convert_lisp_from_string(trimmed, json_convertor, yaml_convertor, pretty, to_yaml_out);
    }

    // Try parsing as JSON first
    if let Ok(val) = serde_json::from_str::<JsonValue>(trimmed) {
        return convert_json_value(&val, json_convertor);
    }

    // Try parsing as Lisp-RPC
    if let Ok(result) = convert_lisp_from_string(trimmed, json_convertor, yaml_convertor, pretty, to_yaml_out) {
        return Ok(result);
    }

    // Try parsing as YAML
    if let Ok(val) = serde_yaml::from_str::<YamlValue>(trimmed) {
        return convert_yaml_value(&val, yaml_convertor);
    }

    // Fallback: parse as Lisp-RPC
    convert_lisp_from_string(trimmed, json_convertor, yaml_convertor, pretty, to_yaml_out)
}

/// Convert a serde_json::Value to Lisp-RPC string format
fn convert_json_value(val: &JsonValue, convertor: &Convertor) -> Result<String> {
    let data = convertor
        .from_json(val)
        .context("Failed to convert JSON to Lisp-RPC data")?;
    Ok(data.to_string())
}

/// Convert a serde_yaml::Value to Lisp-RPC string format
fn convert_yaml_value(val: &YamlValue, convertor: &YamlConvertor) -> Result<String> {
    let data = convertor
        .from_yaml(val)
        .context("Failed to convert YAML to Lisp-RPC data")?;
    Ok(data.to_string())
}

/// Convert a file containing one or more Lisp-RPC data expressions to JSON / YAML string
fn convert_lisp_from_file(
    path: &Path,
    json_convertor: &Convertor,
    yaml_convertor: &YamlConvertor,
    pretty: bool,
    to_yaml_out: bool,
) -> Result<String> {
    let df = DataFile::new(path.to_path_buf())
        .with_context(|| format!("Failed to read and parse Lisp-RPC file at {:?}", path))?;

    if to_yaml_out {
        let mut items = Vec::new();
        for expr_data in df.iter() {
            let val = yaml_convertor
                .to_yaml(&Data::Data(expr_data.clone()))
                .context("Failed to convert ExprData to YAML")?;
            items.push(val);
        }
        let yaml_val = if items.len() == 1 {
            items.into_iter().next().unwrap()
        } else {
            YamlValue::Sequence(items)
        };
        serde_yaml::to_string(&yaml_val).context("Failed to format YAML")
    } else {
        let mut items = Vec::new();
        for expr_data in df.iter() {
            let val = json_convertor
                .to_json(&Data::Data(expr_data.clone()))
                .context("Failed to convert ExprData to JSON")?;
            items.push(val);
        }
        let json_val = if items.len() == 1 {
            items.into_iter().next().unwrap()
        } else {
            JsonValue::Array(items)
        };
        if pretty {
            serde_json::to_string_pretty(&json_val).context("Failed to format JSON")
        } else {
            serde_json::to_string(&json_val).context("Failed to format JSON")
        }
    }
}

/// Convert a string containing one or more Lisp-RPC data expressions to JSON / YAML string
fn convert_lisp_from_string(
    content: &str,
    json_convertor: &Convertor,
    yaml_convertor: &YamlConvertor,
    pretty: bool,
    to_yaml_out: bool,
) -> Result<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(if to_yaml_out { "---".to_string() } else { "[]".to_string() });
    }

    // Try parsing via temp file with DataFile (handles single or multiple named expressions)
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join(format!(
        "lisp_rpc_convert_{}_{}.lisp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    if fs::write(&temp_file_path, trimmed).is_ok() {
        if let Ok(out_str) = convert_lisp_from_file(&temp_file_path, json_convertor, yaml_convertor, pretty, to_yaml_out) {
            let _ = fs::remove_file(&temp_file_path);
            return Ok(out_str);
        }
        let _ = fs::remove_file(&temp_file_path);
    }

    // Fallback: parse single Data (for anonymous maps '(:k v), lists '(a b), or scalars)
    let mut parser = Default::default();
    let data = Data::from_str(&mut parser, trimmed)
        .with_context(|| format!("Failed to parse Lisp-RPC data from: {}", trimmed))?;

    if to_yaml_out {
        let yaml_val = yaml_convertor
            .to_yaml(&data)
            .context("Failed to convert Lisp-RPC Data to YAML")?;
        serde_yaml::to_string(&yaml_val).context("Failed to format YAML")
    } else {
        let json_val = json_convertor
            .to_json(&data)
            .context("Failed to convert Lisp-RPC Data to JSON")?;
        if pretty {
            serde_json::to_string_pretty(&json_val).context("Failed to format JSON")
        } else {
            serde_json::to_string(&json_val).context("Failed to format JSON")
        }
    }
}
