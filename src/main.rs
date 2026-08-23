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
    about = "Convert Lisp-RPC data to JSON"
)]
struct Args {
    /// Raw Lisp-RPC data string directly passed in
    #[arg(short, long, value_name = "DATA", conflicts_with = "file")]
    data: Option<String>,

    /// Path to a file containing Lisp-RPC data (use "-" for stdin)
    #[arg(short, long, value_name = "FILE", conflicts_with = "data")]
    file: Option<PathBuf>,

    /// Output file path (defaults to stdout)
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// Custom key name for the data type name (defaults to "data_name")
    #[arg(short = 'k', long = "key", value_name = "KEY")]
    data_name_key: Option<String>,

    /// Output pretty-formatted JSON with indentation
    #[arg(short, long, default_value_t = false)]
    pretty: bool,

    /// Positional input (raw Lisp-RPC data string, file path, or "-" for stdin)
    #[arg(value_name = "INPUT", conflicts_with_all = ["data", "file"])]
    input: Option<String>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let mut convertor = Convertor::new();
    if let Some(ref key) = args.data_name_key {
        convertor = convertor.data_name_key(key.as_str());
    }

    let json_val = process_input(&args, &convertor)?;

    let json_str = if args.pretty {
        serde_json::to_string_pretty(&json_val).context("Failed to format JSON")?
    } else {
        serde_json::to_string(&json_val).context("Failed to format JSON")?
    };

    if let Some(out_path) = args.output {
        fs::write(&out_path, &json_str)
            .with_context(|| format!("Failed to write output to {:?}", out_path))?;
    } else {
        println!("{}", json_str);
    }

    Ok(())
}

fn process_input(args: &Args, convertor: &Convertor) -> Result<Value> {
    if let Some(ref data_str) = args.data {
        convert_from_string(data_str, convertor)
    } else if let Some(ref file_path) = args.file {
        if file_path.to_str() == Some("-") {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;
            convert_from_string(&buffer, convertor)
        } else {
            convert_from_file(file_path, convertor)
        }
    } else if let Some(ref input_str) = args.input {
        if input_str == "-" {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;
            convert_from_string(&buffer, convertor)
        } else {
            let path = Path::new(input_str);
            if path.exists() && path.is_file() {
                convert_from_file(path, convertor)
            } else {
                convert_from_string(input_str, convertor)
            }
        }
    } else if !io::stdin().is_terminal() {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        convert_from_string(&buffer, convertor)
    } else {
        bail!("No input provided. Pass Lisp-RPC data via -d, -f, positional argument, or pipe via stdin.\nUse --help for more information.");
    }
}

/// Convert a file containing one or more Lisp-RPC data expressions to JSON
fn convert_from_file(path: &Path, convertor: &Convertor) -> Result<Value> {
    let df = DataFile::new(path.to_path_buf())
        .with_context(|| format!("Failed to read and parse Lisp-RPC file at {:?}", path))?;

    let mut items = Vec::new();
    for expr_data in df.iter() {
        let val = convertor
            .to_json(&Data::Data(expr_data.clone()))
            .context("Failed to convert ExprData to JSON")?;
        items.push(val);
    }

    if items.len() == 1 {
        Ok(items.into_iter().next().unwrap())
    } else {
        Ok(Value::Array(items))
    }
}

/// Convert a string containing one or more Lisp-RPC data expressions to JSON
fn convert_from_string(content: &str, convertor: &Convertor) -> Result<Value> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(Value::Array(vec![]));
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
        if let Ok(val) = convert_from_file(&temp_file_path, convertor) {
            let _ = fs::remove_file(&temp_file_path);
            return Ok(val);
        }
        let _ = fs::remove_file(&temp_file_path);
    }

    // Fallback: parse single Data (for anonymous maps '(:k v), lists '(a b), or scalars)
    let mut parser = Default::default();
    let data = Data::from_str(&mut parser, trimmed)
        .with_context(|| format!("Failed to parse Lisp-RPC data from: {}", trimmed))?;

    convertor
        .to_json(&data)
        .context("Failed to convert Lisp-RPC Data to JSON")
}
