---
name: lisp-rpc-json-convertor
description: >-
  Bidirectional conversion between Lisp-RPC data structures (S-expressions) and JSON or YAML.
  Use this skill whenever you need to convert Lisp-RPC data to JSON or YAML, convert JSON or YAML
  into Lisp-RPC S-expressions, parse RPC responses, construct RPC payloads, or execute the
  `lisp-rpc-json-convertor` CLI.
---

# Lisp-RPC JSON / YAML Convertor

This skill guides agents on how to effectively use `lisp-rpc-json-convertor`, a CLI tool and Rust library for converting bidirectionally between Lisp-RPC S-expressions and JSON / YAML.

---

## 1. Prerequisites & Installation

Verify if `lisp-rpc-json-convertor` is installed in your `$PATH`:

```bash
which lisp-rpc-json-convertor
```

### If not installed:

- **From repository source**:
  ```bash
  # Inside the lisp-rpc-json-convertor repository:
  cargo install --path .

  # Or build debug/release binary and invoke directly:
  cargo build --release
  # Binary path: ./target/release/lisp-rpc-json-convertor
  ```

- **From crates.io** (if published):
  ```bash
  cargo install lisp-rpc-json-convertor
  ```

---

## 2. CLI Reference (`lisp-rpc-json-convertor -h`)

```text
Convert between Lisp-RPC data and JSON / YAML

Usage: lisp-rpc-json-convertor [OPTIONS] [INPUT]

Arguments:
  [INPUT]  Positional input (raw Lisp-RPC / JSON / YAML data string, file path, or "-" for stdin)

Options:
  -d, --data <DATA>      Raw Lisp-RPC data string directly passed in (converts Lisp-RPC -> JSON/YAML)
  -j, --json <JSON>      Raw JSON data string directly passed in (converts JSON -> Lisp-RPC)
  -y, --yaml <YAML>      Raw YAML data string directly passed in (converts YAML -> Lisp-RPC)
  -f, --file <FILE>      Path to a file containing Lisp-RPC, JSON, or YAML data (use "-" for stdin)
  -o, --output <OUTPUT>  Output file path (defaults to stdout)
  -k, --key <KEY>        Custom key name for the data type name (defaults to "data_name")
  -p, --pretty           Output pretty-formatted JSON with indentation (when converting to JSON)
  -h, --help             Print help
  -V, --version          Print version
```

### Crucial CLI Behavior Rules:
1. **Mutually Exclusive Input Flags**: `-d`, `-j`, `-y`, `-f`, and `[INPUT]` conflict with each other. Choose **exactly one** input method per command.
2. **Modifiers Can Be Combined**: `-o` (output file), `-k` (tag key), and `-p` (pretty) can be combined with any chosen input method.
3. **Default Direction**:
   - **JSON or YAML input** &rarr; outputs **Lisp-RPC S-expressions**.
   - **Lisp-RPC input** &rarr; outputs **JSON** by default (add `-p` for indented JSON).
4. **Generating YAML Output**: To output YAML from Lisp-RPC, you must specify an output path ending in `.yaml` or `.yml` with `-o <OUTPUT>` (there is no standalone `--to-yaml` flag).

---

## 3. Core Data Mapping Reference

Lisp-RPC uses S-expression syntax with strict quoting and tagging rules:

| Lisp-RPC Data Structure | JSON / YAML Representation | Quoting & Syntax Notes |
| :--- | :--- | :--- |
| **Named Structure**<br>`(user-info :name "Alice" :id 1 :active t)` | `{"data_name": "user-info", "name": "Alice", "id": 1, "active": true}` | Top-level RPC requests/responses. Tagged with `"data_name"` (or custom `-k`). **Never quote** `(` in Lisp. |
| **Anonymous Map**<br>`'(:host "localhost" :port 8080)` | `{"host": "localhost", "port": 8080}` | Standard JSON object without tag key. In Lisp-RPC, maps **must be quoted** (`'`). |
| **List / Sequence**<br>`'("apple" "banana")` | `["apple", "banana"]` | JSON arrays map to quoted Lisp-RPC lists. |
| **List of Named Structures**<br>`'((book :id 1) (book :id 2))` | `[{"data_name": "book", "id": 1}, {"data_name": "book", "id": 2}]` | Outer list is quoted; inner named structures are **unquoted**. |
| **List of Anonymous Maps**<br>`'('(:id 1) '(:id 2))` | `[{"id": 1}, {"id": 2}]` | Outer list and each inner anonymous map are quoted. |
| **String**<br>`"hello"` | `"hello"` | Standard double-quoted string. |
| **Number**<br>`42` or `3.14` | `42` or `3.14` | Integer or float. |
| **Boolean True**<br>`t` | `true` | Common Lisp boolean truth symbol `t` (unquoted). |
| **Boolean False**<br>`'false` (or absent/nil) | `false` | Symbol `'false` maps to `false`. **Important**: in Lisp-RPC input, `false` must be quoted (`'false`). Unquoted `false` will produce a parse error. |
| **Null**<br>`'nil` (or absent) | `null` | Quoted `'nil` maps to `null`. Unquoted `nil` in map values represents absent/empty. |
| **Symbol / Enum**<br>`'active` | `"active"` | Quoted Lisp symbol maps to string. |

---

## 4. Usage Recipes for Agents

### 4.1. Converting JSON to Lisp-RPC

```bash
# Convert a tagged JSON object to a named Lisp-RPC request:
lisp-rpc-json-convertor -j '{"data_name": "get-book", "title": "Dune", "year": 1965}'
# Output: (get-book :title "Dune" :year 1965)

# Convert untagged JSON to an anonymous map:
lisp-rpc-json-convertor -j '{"first": "John", "last": "Doe"}'
# Output: '(:first "John" :last "Doe")

# Convert a JSON array to a Lisp-RPC list:
lisp-rpc-json-convertor -j '[{"data_name": "book", "id": 1}, {"data_name": "book", "id": 2}]'
# Output: '((book :id 1) (book :id 2))
```

### 4.2. Converting Lisp-RPC to JSON

```bash
# Compact JSON:
lisp-rpc-json-convertor -d '(book :title "Dune" :year 1965)'
# Output: {"data_name":"book","title":"Dune","year":1965}

# Pretty-printed JSON (ideal for inspection):
lisp-rpc-json-convertor -d '(book :title "Dune" :year 1965)' -p
```

### 4.3. Converting YAML to Lisp-RPC

```bash
lisp-rpc-json-convertor -y 'data_name: book
title: Dune
year: 1965'
# Output: (book :title "Dune" :year 1965)
```

### 4.4. Converting Lisp-RPC to YAML

To produce YAML, specify an output file ending in `.yaml` or `.yml` with `-o`:

```bash
lisp-rpc-json-convertor -d '(book :title "Dune" :year 1965)' -o output.yaml
```

### 4.5. Custom Type Tag Key (`-k / --key`)

When the remote schema uses a different type discriminator key (e.g., `"_type"`, `"kind"`, or `"type"`):

```bash
# JSON to Lisp-RPC with custom tag:
lisp-rpc-json-convertor -j '{"_type": "user", "name": "Alice"}' -k "_type"
# Output: (user :name "Alice")

# Lisp-RPC to JSON with custom tag:
lisp-rpc-json-convertor -d '(user :name "Alice")' -k "_type"
# Output: {"_type":"user","name":"Alice"}
```

### 4.6. Shell-Safe Piping via Stdin (Recommended for Agents)

Because Lisp S-expressions contain single quotes (`'`) and double quotes (`"`), direct shell arguments can easily break on shell escaping. Piping via stdin avoids quoting conflicts:

```bash
# Safely convert multi-line Lisp-RPC to JSON via heredoc:
cat <<'EOF' | lisp-rpc-json-convertor -p
(create-order
  :order-id 10042
  :status 'confirmed
  :items '((item :sku "A-1" :qty 2)
           (item :sku "B-2" :qty 1)))
EOF
```

```bash
# Safely convert JSON to Lisp-RPC via echo/pipe:
echo '{"data_name": "fetch-user", "id": 123}' | lisp-rpc-json-convertor
# Output: (fetch-user :id 123)
```

### 4.7. File Conversions & Multi-Record Files

```bash
# Convert JSON or YAML file to Lisp-RPC:
lisp-rpc-json-convertor -f request.json -o request.lisp
lisp-rpc-json-convertor -f config.yaml -o config.lisp

# Convert a .lisp file containing one or multiple S-expressions to JSON:
lisp-rpc-json-convertor -f data.lisp -o data.json -p
```
*Note: If `data.lisp` contains multiple top-level expressions, they are automatically aggregated into a JSON array in the output.*

---

## 5. Verification & Troubleshooting

- **Check exit status**: Returns `0` on success, `1` on error.
- **`cannot generate Data from the symbol Atom { value: Symbol("false") }`**:
  - In Lisp-RPC S-expression input, booleans must be either `t` (true) or `'false` (quoted symbol for false). Unquoted `false` is not a valid bare literal.
- **`Failed to parse Lisp-RPC data`**:
  - Ensure balanced parentheses.
  - Remember the golden rule: named top-level records `(record :k v)` are **not** quoted with `'`.
  - Anonymous maps `'(:k v)` and lists `'(1 2)` **must** have a leading quote `'`.
- **Unexpected auto-detection**:
  - If a positional string or file does not parse as expected, explicitly pass `-d` (Lisp-RPC), `-j` (JSON), or `-y` (YAML) instead of relying on auto-detection.

---

## 6. Rust Library API

If building Rust code directly using `lisp-rpc-json-convertor`:

```toml
[dependencies]
lisp-rpc-json-convertor = "0"
serde_json = "1"
```

```rust
use lisp_rpc_json_convertor::{from_json_str_to_string, JsonLispRpcExt, LispRpcJsonExt, Convertor};
use serde_json::json;

// Helper: JSON string -> Lisp-RPC string
let lisp_str = from_json_str_to_string(r#"{"data_name": "book", "title": "1984"}"#)?;
assert_eq!(lisp_str, "(book :title \"1984\")");

// Trait on serde_json::Value -> Lisp-RPC Data
let val = json!({ "data_name": "server", "host": "localhost" });
let data = val.to_lisp_rpc()?; // lisp_rpc_rust_raw_data::Data
let json_back = data.to_json()?;

// Custom tag key
let conv = Convertor::new().data_name_key("_type");
let data = conv.from_json(&json!({ "_type": "user", "name": "Alice" }))?;
```

---

## 7. How to Copy This Skill to Another Agent

To equip another agent or project with this skill:

1. **Workspace Scope**:
   Copy the skill file into the target repository's `.agents/skills/lisp-rpc-json-convertor/` directory:
   ```bash
   mkdir -p /path/to/target-project/.agents/skills/lisp-rpc-json-convertor
   cp /path/to/lisp-rpc-json-convertor/SKILL.md /path/to/target-project/.agents/skills/lisp-rpc-json-convertor/
   ```

2. **Global User Scope** (available across all Antigravity projects on the machine):
   ```bash
   mkdir -p ~/.gemini/antigravity-cli/skills/lisp-rpc-json-convertor
   cp /path/to/lisp-rpc-json-convertor/SKILL.md ~/.gemini/antigravity-cli/skills/lisp-rpc-json-convertor/
   ```

