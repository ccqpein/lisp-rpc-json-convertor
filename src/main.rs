use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use lisp_rpc_json_convertor::Convertor;
use lisp_rpc_rust_raw_data::files::DataFile;
use lisp_rpc_rust_raw_data::{Data, FromStr};
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(
    name = "lisp-rpc-json-convertor",
    author,
    version,
    about = "Convert between Lisp-RPC data and JSON"
)]
struct Args {
    /// Raw Lisp-RPC data string directly passed in (converts Lisp-RPC -> JSON)
    #[arg(short = 'd', long = "data", value_name = "DATA", conflicts_with_all = ["file", "json"])]
    data: Option<String>,

    /// Raw JSON data string directly passed in (converts JSON -> Lisp-RPC)
    #[arg(short = 'j', long = "json", value_name = "JSON", conflicts_with_all = ["data", "file"])]
    json: Option<String>,

    /// Path to a file containing Lisp-RPC or JSON data (use "-" for stdin)
    #[arg(short = 'f', long = "file", value_name = "FILE", conflicts_with_all = ["data", "json"])]
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

    /// Positional input (raw Lisp-RPC / JSON data string, file path, or "-" for stdin)
    #[arg(value_name = "INPUT", conflicts_with_all = ["data", "json", "file"])]
    input: Option<String>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let mut convertor = Convertor::new();
    if let Some(ref key) = args.data_name_key {
        convertor = convertor.data_name_key(key.as_str());
    }

    let result_str = process_input(&args, &convertor)?;

    if let Some(out_path) = args.output {
        fs::write(&out_path, &result_str)
            .with_context(|| format!("Failed to write output to {:?}", out_path))?;
    } else {
        println!("{}", result_str);
    }

    Ok(())
}

fn process_input(args: &Args, convertor: &Convertor) -> Result<String> {
    if let Some(ref json_str) = args.json {
        let val: Value = serde_json::from_str(json_str)
            .with_context(|| format!("Failed to parse JSON string: {}", json_str))?;
        convert_json_value(&val, convertor)
    } else if let Some(ref data_str) = args.data {
        convert_lisp_from_string(data_str, convertor, args.pretty)
    } else if let Some(ref file_path) = args.file {
        if file_path.to_str() == Some("-") {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;
            convert_auto(&buffer, convertor, args.pretty)
        } else {
            convert_from_file_path(file_path, convertor, args.pretty)
        }
    } else if let Some(ref input_str) = args.input {
        if input_str == "-" {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;
            convert_auto(&buffer, convertor, args.pretty)
        } else {
            let path = Path::new(input_str);
            if path.exists() && path.is_file() {
                convert_from_file_path(path, convertor, args.pretty)
            } else {
                convert_auto(input_str, convertor, args.pretty)
            }
        }
    } else if !io::stdin().is_terminal() {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        convert_auto(&buffer, convertor, args.pretty)
    } else {
        bail!("No input provided. Pass data via -d/--data, -j/--json, -f/--file, positional argument, or pipe via stdin.\nUse --help for more information.");
    }
}

/// Convert file path depending on file extension or content
fn convert_from_file_path(path: &Path, convertor: &Convertor, pretty: bool) -> Result<String> {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if ext.eq_ignore_ascii_case("json") {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read JSON file at {:?}", path))?;
            let val: Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse JSON file at {:?}", path))?;
            return convert_json_value(&val, convertor);
        } else if ext.eq_ignore_ascii_case("lisp")
            || ext.eq_ignore_ascii_case("lsp")
            || ext.eq_ignore_ascii_case("scm")
            || ext.eq_ignore_ascii_case("cl")
        {
            return convert_lisp_from_file(path, convertor, pretty);
        }
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file at {:?}", path))?;
    convert_auto(&content, convertor, pretty)
}

/// Automatically detect whether the content is JSON or Lisp-RPC and convert accordingly
fn convert_auto(content: &str, convertor: &Convertor, pretty: bool) -> Result<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            return convert_json_value(&val, convertor);
        }
    }

    if trimmed.starts_with('(') || trimmed.starts_with('\'') || trimmed.starts_with(';') {
        return convert_lisp_from_string(trimmed, convertor, pretty);
    }

    // Try parsing as JSON first
    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        return convert_json_value(&val, convertor);
    }

    // Fallback: parse as Lisp-RPC
    convert_lisp_from_string(trimmed, convertor, pretty)
}

/// Convert a serde_json::Value to Lisp-RPC string format
fn convert_json_value(val: &Value, convertor: &Convertor) -> Result<String> {
    let data = convertor
        .from_json(val)
        .context("Failed to convert JSON to Lisp-RPC data")?;
    Ok(data.to_string())
}

/// Convert a file containing one or more Lisp-RPC data expressions to JSON string
fn convert_lisp_from_file(path: &Path, convertor: &Convertor, pretty: bool) -> Result<String> {
    let df = DataFile::new(path.to_path_buf())
        .with_context(|| format!("Failed to read and parse Lisp-RPC file at {:?}", path))?;

    let mut items = Vec::new();
    for expr_data in df.iter() {
        let val = convertor
            .to_json(&Data::Data(expr_data.clone()))
            .context("Failed to convert ExprData to JSON")?;
        items.push(val);
    }

    let json_val = if items.len() == 1 {
        items.into_iter().next().unwrap()
    } else {
        Value::Array(items)
    };

    if pretty {
        serde_json::to_string_pretty(&json_val).context("Failed to format JSON")
    } else {
        serde_json::to_string(&json_val).context("Failed to format JSON")
    }
}

/// Convert a string containing one or more Lisp-RPC data expressions to JSON string
fn convert_lisp_from_string(content: &str, convertor: &Convertor, pretty: bool) -> Result<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok("[]".to_string());
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
        if let Ok(json_str) = convert_lisp_from_file(&temp_file_path, convertor, pretty) {
            let _ = fs::remove_file(&temp_file_path);
            return Ok(json_str);
        }
        let _ = fs::remove_file(&temp_file_path);
    }

    // Fallback: parse single Data (for anonymous maps '(:k v), lists '(a b), or scalars)
    let mut parser = Default::default();
    let data = Data::from_str(&mut parser, trimmed)
        .with_context(|| format!("Failed to parse Lisp-RPC data from: {}", trimmed))?;

    let json_val = convertor
        .to_json(&data)
        .context("Failed to convert Lisp-RPC Data to JSON")?;

    if pretty {
        serde_json::to_string_pretty(&json_val).context("Failed to format JSON")
    } else {
        serde_json::to_string(&json_val).context("Failed to format JSON")
    }
}
