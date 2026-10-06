use agent_core::{paths::StoragePaths, AgentError, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub enum AgentTarget {
    Codex,
    Claude,
    OpenCode,
    Cursor,
    Gemini,
    Zed,
    Generic,
}

impl AgentTarget {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "codex" => Self::Codex,
            "claude" | "claude-code" | "claudecode" => Self::Claude,
            "opencode" => Self::OpenCode,
            "cursor" => Self::Cursor,
            "gemini" | "gemini-cli" => Self::Gemini,
            "zed" => Self::Zed,
            _ => Self::Generic,
        }
    }

    pub fn config_path(&self, home: &Path) -> Option<PathBuf> {
        match self {
            Self::Codex => Some(home.join(".codex/config.toml")),
            Self::Claude => Some(home.join(".claude.json")),
            Self::OpenCode => Some(home.join(".config/opencode/opencode.json")),
            Self::Cursor => Some(home.join(".cursor/mcp.json")),
            Self::Gemini => Some(home.join(".gemini/settings.json")),
            Self::Zed => Some(home.join(".config/zed/settings.json")),
            Self::Generic => None,
        }
    }

    pub fn default_config_path(&self) -> Option<PathBuf> {
        let home = StoragePaths::home_dir().ok()?;
        self.config_path(&home)
    }
}

pub struct AgentConnector;

impl AgentConnector {
    /// Logical MCP snippet. Codex is represented as JSON here; the file format is TOML.
    pub fn generate_config(target: &AgentTarget) -> Value {
        match target {
            AgentTarget::Zed => json!({
                "context_servers": {
                    "02": {
                        "source": "custom",
                        "command": "02",
                        "args": ["mcp"],
                        "env": {}
                    }
                }
            }),
            AgentTarget::OpenCode => json!({
                "mcp": {
                    "02": {
                        "type": "local",
                        "command": ["02", "mcp"]
                    }
                }
            }),
            AgentTarget::Codex => json!({
                "mcp_servers": {
                    "02": {
                        "command": "02",
                        "args": ["mcp"]
                    }
                }
            }),
            _ => json!({
                "mcpServers": {
                    "02": {
                        "command": "02",
                        "args": ["mcp"]
                    }
                }
            }),
        }
    }

    /// Client-native text (TOML for Codex, JSON otherwise).
    pub fn render_config(target: &AgentTarget) -> Result<String> {
        let text = if matches!(target, AgentTarget::Codex) {
            let mut doc = Vec::new();
            upsert_codex(&mut doc).map_err(AgentError::Config)?;
            render_toml(&doc)
        } else {
            serde_json::to_string_pretty(&Self::generate_config(target))
                .map_err(|err| AgentError::General(format!("JSON serialization error: {err}")))?
        };
        require_valid_02(target, &text)?;
        Ok(text)
    }

    /// Parse a client config into JSON. Codex input is TOML.
    pub fn parse_config_document(target: &AgentTarget, text: &str) -> Result<Value> {
        if matches!(target, AgentTarget::Codex) {
            let doc = parse_toml(text).map_err(AgentError::Config)?;
            return Ok(table_to_json(&doc));
        }
        serde_json::from_str(text)
            .map_err(|err| AgentError::Config(format!("invalid MCP config JSON: {err}")))
    }

    /// Connect and write or print the MCP configuration for a coding agent.
    pub fn connect(target_name: &str, write_to_file: bool) -> Result<String> {
        let home = StoragePaths::home_dir()?;
        Self::connect_at(&home, target_name, write_to_file)
    }

    /// Same as `connect`, but resolve config paths under `home` (tests use a temp home).
    pub fn connect_at(home: &Path, target_name: &str, write_to_file: bool) -> Result<String> {
        let target = AgentTarget::parse(target_name);
        let rendered = Self::render_config(&target)?;
        if write_to_file {
            if let Some(path) = target.config_path(home) {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)?;
                    }
                }
                let final_text = if path.exists() {
                    let existing_text = fs::read_to_string(&path)?;
                    Self::merge_config_text(&target, &existing_text)?
                } else {
                    rendered
                };
                require_valid_02(&target, &final_text)?;
                fs::write(&path, &final_text)?;
                let reread = fs::read_to_string(&path)?;
                require_valid_02(&target, &reread).map_err(|_| {
                    AgentError::Config(format!(
                        "written MCP config at {} does not contain a client-valid 02 entry",
                        path.display()
                    ))
                })?;
                return Ok(format!(
                    "Successfully configured '{}' MCP server in {}",
                    target_name,
                    path.display()
                ));
            }
        }
        Ok(rendered)
    }

    /// Merge the 02 server entry into an existing client config.
    pub fn merge_config_text(target: &AgentTarget, existing_text: &str) -> Result<String> {
        let written = if matches!(target, AgentTarget::Codex) {
            let mut doc = if existing_text.trim().is_empty() {
                Vec::new()
            } else {
                parse_toml(existing_text).map_err(|err| {
                    AgentError::Config(format!("invalid Codex config.toml: {err}"))
                })?
            };
            upsert_codex(&mut doc).map_err(AgentError::Config)?;
            render_toml(&doc)
        } else {
            merge_json(target, existing_text)?
        };
        require_valid_02(target, &written)?;
        Ok(written)
    }
}

fn merge_json(target: &AgentTarget, existing_text: &str) -> Result<String> {
    let mut existing: Value = if existing_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(existing_text)
            .map_err(|err| AgentError::Config(format!("invalid MCP config JSON: {err}")))?
    };
    let root = existing
        .as_object_mut()
        .ok_or_else(|| AgentError::Config("MCP config root must be a JSON object".to_string()))?;
    match target {
        AgentTarget::Zed => upsert_json_entry(
            root,
            "context_servers",
            json!({
                "source": "custom",
                "command": "02",
                "args": ["mcp"],
                "env": {}
            }),
        )?,
        AgentTarget::OpenCode => upsert_json_entry(
            root,
            "mcp",
            json!({
                "type": "local",
                "command": ["02", "mcp"]
            }),
        )?,
        AgentTarget::Codex => {
            return Err(AgentError::Config(
                "Codex config is TOML, not JSON".to_string(),
            ));
        }
        _ => upsert_json_entry(
            root,
            "mcpServers",
            json!({ "command": "02", "args": ["mcp"] }),
        )?,
    }
    serde_json::to_string_pretty(&existing)
        .map_err(|err| AgentError::Config(format!("MCP config serialization failed: {err}")))
}

fn upsert_json_entry(root: &mut Map<String, Value>, key: &str, fields: Value) -> Result<()> {
    let servers = root.entry(key).or_insert_with(|| json!({}));
    let Some(servers) = servers.as_object_mut() else {
        return Err(AgentError::Config(format!("{key} must be a JSON object")));
    };
    let entry = servers.entry("02".to_string()).or_insert_with(|| json!({}));
    let Some(entry) = entry.as_object_mut() else {
        return Err(AgentError::Config(format!(
            "{key}.02 must be a JSON object"
        )));
    };
    let Some(fields) = fields.as_object() else {
        return Err(AgentError::Config(
            "server entry must be an object".to_string(),
        ));
    };
    for (field, value) in fields {
        if field == "env" && entry.contains_key("env") {
            continue;
        }
        entry.insert(field.clone(), value.clone());
    }
    Ok(())
}

fn require_valid_02(target: &AgentTarget, text: &str) -> Result<()> {
    let ok = match target {
        AgentTarget::Codex => {
            let doc = parse_toml(text)
                .map_err(|err| AgentError::Config(format!("invalid Codex config.toml: {err}")))?;
            codex_entry_valid(&doc)
        }
        _ => {
            let value: Value = serde_json::from_str(text).map_err(|err| {
                AgentError::Config(format!("written MCP config is not JSON: {err}"))
            })?;
            json_entry_valid(target, &value)
        }
    };
    if !ok {
        return Err(AgentError::Config(
            "written MCP config is missing a client-valid 02 entry".into(),
        ));
    }
    Ok(())
}

fn json_entry_valid(target: &AgentTarget, value: &Value) -> bool {
    match target {
        AgentTarget::Zed => {
            let entry = &value["context_servers"]["02"];
            entry.get("source").and_then(|v| v.as_str()) == Some("custom") && stdio_command(entry)
        }
        AgentTarget::OpenCode => {
            let entry = &value["mcp"]["02"];
            entry.get("type").and_then(|v| v.as_str()) == Some("local")
                && command_array(entry, &["02", "mcp"])
        }
        AgentTarget::Codex => false,
        _ => stdio_command(&value["mcpServers"]["02"]),
    }
}

fn stdio_command(entry: &Value) -> bool {
    entry.get("command").and_then(|v| v.as_str()) == Some("02") && command_args(entry, &["mcp"])
}

fn command_args(entry: &Value, expected: &[&str]) -> bool {
    entry
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.len() == expected.len()
                && arr
                    .iter()
                    .zip(expected)
                    .all(|(value, want)| value.as_str() == Some(*want))
        })
        .unwrap_or(false)
}

fn command_array(entry: &Value, expected: &[&str]) -> bool {
    entry
        .get("command")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.len() == expected.len()
                && arr
                    .iter()
                    .zip(expected)
                    .all(|(value, want)| value.as_str() == Some(*want))
        })
        .unwrap_or(false)
}

#[derive(Clone, Debug)]
enum TVal {
    String(String),
    Int(i64),
    Float(String),
    Bool(bool),
    Raw(String),
    Array(Vec<TVal>),
    Table(Vec<(String, TVal)>),
}

fn upsert_codex(doc: &mut Vec<(String, TVal)>) -> std::result::Result<(), String> {
    insert_path(
        doc,
        &key_path(&["mcp_servers", "02", "command"]),
        TVal::String("02".to_string()),
    )?;
    insert_path(
        doc,
        &key_path(&["mcp_servers", "02", "args"]),
        TVal::Array(vec![TVal::String("mcp".to_string())]),
    )?;
    Ok(())
}

fn key_path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn codex_entry_valid(doc: &[(String, TVal)]) -> bool {
    let Some(TVal::Table(entry)) = get_path(doc, &["mcp_servers", "02"]) else {
        return false;
    };
    string_at(entry, "command") == Some("02")
        && strings_at(entry, "args") == Some(vec!["mcp".to_string()])
}

fn string_at<'a>(table: &'a [(String, TVal)], key: &str) -> Option<&'a str> {
    match table.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
        Some(TVal::String(value)) => Some(value),
        _ => None,
    }
}

fn strings_at(table: &[(String, TVal)], key: &str) -> Option<Vec<String>> {
    let Some(TVal::Array(items)) = table.iter().find(|(k, _)| k == key).map(|(_, v)| v) else {
        return None;
    };
    items
        .iter()
        .map(|item| match item {
            TVal::String(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn get_path<'a>(root: &'a [(String, TVal)], path: &[&str]) -> Option<&'a TVal> {
    if path.is_empty() {
        return None;
    }
    let value = root.iter().find(|(k, _)| k == path[0]).map(|(_, v)| v)?;
    if path.len() == 1 {
        return Some(value);
    }
    match value {
        TVal::Table(inner) => get_path(inner, &path[1..]),
        _ => None,
    }
}

fn insert_path(
    root: &mut Vec<(String, TVal)>,
    path: &[String],
    val: TVal,
) -> std::result::Result<(), String> {
    if path.is_empty() {
        return Err("empty key path".into());
    }
    let key = &path[0];
    if path.len() == 1 {
        if let Some(pos) = root.iter().position(|(k, _)| k == key) {
            root[pos].1 = val;
        } else {
            root.push((key.clone(), val));
        }
        return Ok(());
    }
    if !root.iter().any(|(k, _)| k == key) {
        root.push((key.clone(), TVal::Table(Vec::new())));
    }
    let pos = root.iter().position(|(k, _)| k == key).unwrap();
    match &mut root[pos].1 {
        TVal::Table(inner) => insert_path(inner, &path[1..], val),
        _ => Err(format!("key '{key}' is not a table")),
    }
}

fn ensure_table(
    root: &mut Vec<(String, TVal)>,
    path: &[String],
) -> std::result::Result<(), String> {
    if path.is_empty() {
        return Err("empty table path".into());
    }
    let key = &path[0];
    if !root.iter().any(|(k, _)| k == key) {
        root.push((key.clone(), TVal::Table(Vec::new())));
    }
    let pos = root.iter().position(|(k, _)| k == key).unwrap();
    if path.len() == 1 {
        return match &root[pos].1 {
            TVal::Table(_) => Ok(()),
            _ => Err(format!("key '{key}' is not a table")),
        };
    }
    match &mut root[pos].1 {
        TVal::Table(inner) => ensure_table(inner, &path[1..]),
        _ => Err(format!("key '{key}' is not a table")),
    }
}

fn table_to_json(table: &[(String, TVal)]) -> Value {
    let mut map = Map::new();
    for (key, value) in table {
        map.insert(key.clone(), tval_to_json(value));
    }
    Value::Object(map)
}

fn tval_to_json(value: &TVal) -> Value {
    match value {
        TVal::String(text) | TVal::Raw(text) | TVal::Float(text) => Value::String(text.clone()),
        TVal::Int(number) => json!(number),
        TVal::Bool(flag) => Value::Bool(*flag),
        TVal::Array(items) => Value::Array(items.iter().map(tval_to_json).collect()),
        TVal::Table(table) => table_to_json(table),
    }
}

fn render_toml(root: &[(String, TVal)]) -> String {
    let mut out = String::new();
    for (key, value) in root {
        if !matches!(value, TVal::Table(_)) {
            out.push_str(&format!("{} = {}\n", render_key(key), render_val(value)));
        }
    }
    render_tables(&mut out, "", root);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn render_tables(out: &mut String, prefix: &str, table: &[(String, TVal)]) {
    for (key, value) in table {
        let TVal::Table(inner) = value else {
            continue;
        };
        let path = if prefix.is_empty() {
            render_key(key)
        } else {
            format!("{prefix}.{}", render_key(key))
        };
        if inner
            .iter()
            .any(|(_, child)| !matches!(child, TVal::Table(_)))
        {
            out.push('\n');
            out.push_str(&format!("[{path}]\n"));
            for (child_key, child_val) in inner {
                if !matches!(child_val, TVal::Table(_)) {
                    out.push_str(&format!(
                        "{} = {}\n",
                        render_key(child_key),
                        render_val(child_val)
                    ));
                }
            }
        }
        render_tables(out, &path, inner);
    }
}

fn render_key(key: &str) -> String {
    if !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        key.to_string()
    } else {
        format!("\"{}\"", escape_basic(key))
    }
}

fn render_val(value: &TVal) -> String {
    match value {
        TVal::String(text) => format!("\"{}\"", escape_basic(text)),
        TVal::Int(number) => number.to_string(),
        TVal::Float(text) | TVal::Raw(text) => text.clone(),
        TVal::Bool(true) => "true".to_string(),
        TVal::Bool(false) => "false".to_string(),
        TVal::Array(items) => {
            let parts: Vec<String> = items.iter().map(render_val).collect();
            format!("[{}]", parts.join(", "))
        }
        TVal::Table(table) => {
            let parts: Vec<String> = table
                .iter()
                .map(|(key, value)| format!("{} = {}", render_key(key), render_val(value)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn escape_basic(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn eof(&self) -> bool {
        self.i >= self.s.len()
    }

    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.i += ch.len_utf8();
        Some(ch)
    }

    fn err(&self, msg: &str) -> String {
        format!("{msg} at byte {}", self.i)
    }

    fn skip_inline(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.bump();
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
                self.bump();
            }
            if self.peek() == Some('#') {
                while matches!(self.peek(), Some(ch) if ch != '\n') {
                    self.bump();
                }
                continue;
            }
            break;
        }
    }

    fn finish_line(&mut self) -> std::result::Result<(), String> {
        self.skip_inline();
        if self.peek() == Some('#') {
            while matches!(self.peek(), Some(ch) if ch != '\n') {
                self.bump();
            }
        }
        match self.peek() {
            None => Ok(()),
            Some('\n') => {
                self.bump();
                Ok(())
            }
            Some('\r') => {
                self.bump();
                if self.peek() == Some('\n') {
                    self.bump();
                }
                Ok(())
            }
            Some(ch) => Err(format!("unexpected {ch:?} after value at byte {}", self.i)),
        }
    }

    fn parse_key(&mut self) -> std::result::Result<Vec<String>, String> {
        let mut parts = Vec::new();
        loop {
            self.skip_inline();
            parts.push(self.parse_key_segment()?);
            self.skip_inline();
            if self.peek() == Some('.') {
                self.bump();
                continue;
            }
            break;
        }
        if parts.iter().any(|part| part.is_empty()) {
            return Err(self.err("empty key"));
        }
        Ok(parts)
    }

    fn parse_key_segment(&mut self) -> std::result::Result<String, String> {
        match self.peek() {
            Some('"') => {
                if self.rest().starts_with("\"\"\"") {
                    return Err(self.err("multiline string key is not supported"));
                }
                self.bump();
                self.parse_basic_string()
            }
            Some('\'') => {
                if self.rest().starts_with("'''") {
                    return Err(self.err("multiline string key is not supported"));
                }
                self.bump();
                self.parse_literal_string()
            }
            Some(ch) if is_bare(ch) => {
                let start = self.i;
                while matches!(self.peek(), Some(ch) if is_bare(ch)) {
                    self.bump();
                }
                Ok(self.s[start..self.i].to_string())
            }
            _ => Err(self.err("expected key")),
        }
    }

    fn parse_basic_string(&mut self) -> std::result::Result<String, String> {
        let mut out = String::new();
        loop {
            let ch = match self.bump() {
                Some(ch) => ch,
                None => return Err(self.err("unterminated string")),
            };
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let escaped = match self.bump() {
                        Some(ch) => ch,
                        None => return Err(self.err("unterminated escape")),
                    };
                    match escaped {
                        'b' => out.push('\u{0008}'),
                        't' => out.push('\t'),
                        'n' => out.push('\n'),
                        'f' => out.push('\u{000c}'),
                        'r' => out.push('\r'),
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        'u' => {
                            let cp = self.read_hex(4)?;
                            let Some(decoded) = char::from_u32(cp) else {
                                return Err(self.err("invalid unicode escape"));
                            };
                            out.push(decoded);
                        }
                        'U' => {
                            let cp = self.read_hex(8)?;
                            let Some(decoded) = char::from_u32(cp) else {
                                return Err(self.err("invalid unicode escape"));
                            };
                            out.push(decoded);
                        }
                        '\n' => self.skip_inline(),
                        '\r' => {
                            if self.peek() == Some('\n') {
                                self.bump();
                            }
                            self.skip_inline();
                        }
                        _ => return Err(self.err("invalid escape")),
                    }
                }
                '\n' | '\r' => return Err(self.err("newline in basic string")),
                _ => out.push(ch),
            }
        }
    }

    fn parse_literal_string(&mut self) -> std::result::Result<String, String> {
        let mut out = String::new();
        loop {
            let ch = match self.bump() {
                Some(ch) => ch,
                None => return Err(self.err("unterminated literal string")),
            };
            match ch {
                '\'' => return Ok(out),
                '\n' | '\r' => return Err(self.err("newline in literal string")),
                _ => out.push(ch),
            }
        }
    }

    fn read_hex(&mut self, n: usize) -> std::result::Result<u32, String> {
        let mut hex = String::new();
        for _ in 0..n {
            let ch = match self.bump() {
                Some(ch) => ch,
                None => return Err(self.err("short hex escape")),
            };
            if !ch.is_ascii_hexdigit() {
                return Err(self.err("bad hex escape"));
            }
            hex.push(ch);
        }
        u32::from_str_radix(&hex, 16).map_err(|err| err.to_string())
    }

    fn parse_string_value(&mut self) -> std::result::Result<TVal, String> {
        if self.rest().starts_with("\"\"\"") {
            return self.parse_multiline(true);
        }
        if self.rest().starts_with("'''") {
            return self.parse_multiline(false);
        }
        if self.peek() == Some('"') {
            self.bump();
            return Ok(TVal::String(self.parse_basic_string()?));
        }
        if self.peek() == Some('\'') {
            self.bump();
            return Ok(TVal::String(self.parse_literal_string()?));
        }
        Err(self.err("expected string"))
    }

    fn parse_multiline(&mut self, basic: bool) -> std::result::Result<TVal, String> {
        self.i += 3;
        if self.rest().starts_with("\r\n") {
            self.i += 2;
        } else if self.rest().starts_with('\n') {
            self.i += 1;
        }
        let end = if basic { "\"\"\"" } else { "'''" };
        let mut out = String::new();
        loop {
            if self.rest().starts_with(end) {
                self.i += 3;
                break;
            }
            if self.eof() {
                return Err(self.err("unterminated multiline string"));
            }
            if basic && self.peek() == Some('\\') {
                self.bump();
                let escaped = match self.bump() {
                    Some(ch) => ch,
                    None => return Err(self.err("unterminated escape")),
                };
                match escaped {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\n' => self.skip_inline(),
                    '\r' => {
                        if self.peek() == Some('\n') {
                            self.bump();
                        }
                        self.skip_inline();
                    }
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
                continue;
            }
            out.push(match self.bump() {
                Some(ch) => ch,
                None => return Err(self.err("unterminated multiline string")),
            });
        }
        Ok(TVal::String(out))
    }

    fn parse_value(&mut self) -> std::result::Result<TVal, String> {
        self.skip_ws_and_comments();
        match self.peek() {
            None => Err(self.err("expected value")),
            Some('"') | Some('\'') => self.parse_string_value(),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_inline_table(),
            Some(ch) if ch == '+' || ch == '-' || ch.is_ascii_digit() => self.parse_numberish(),
            Some(_) => {
                let ident = self.parse_bare()?;
                match ident.as_str() {
                    "true" => Ok(TVal::Bool(true)),
                    "false" => Ok(TVal::Bool(false)),
                    "inf" | "nan" => Ok(TVal::Raw(ident)),
                    _ => Err(self.err("invalid value")),
                }
            }
        }
    }

    fn parse_bare(&mut self) -> std::result::Result<String, String> {
        let start = self.i;
        while matches!(self.peek(), Some(ch) if is_bare(ch)) {
            self.bump();
        }
        if self.i == start {
            return Err(self.err("expected value"));
        }
        Ok(self.s[start..self.i].to_string())
    }

    fn parse_numberish(&mut self) -> std::result::Result<TVal, String> {
        let token = self.read_scalar_token()?;
        if matches!(
            token.as_str(),
            "inf" | "+inf" | "-inf" | "nan" | "+nan" | "-nan"
        ) {
            return Ok(TVal::Raw(token));
        }
        let looks_like_date = token.contains(':')
            || token.contains('T')
            || token.contains('t')
            || token.contains('Z');
        if !looks_like_date {
            if let Some(number) = parse_int_token(&token) {
                if !token.contains('.') && !token.contains('e') && !token.contains('E') {
                    return Ok(TVal::Int(number));
                }
            }
        }
        if looks_like_date {
            return Ok(TVal::Raw(token));
        }
        let cleaned: String = token.chars().filter(|ch| *ch != '_').collect();
        if cleaned.parse::<f64>().is_ok()
            && (token.contains('.') || token.contains('e') || token.contains('E'))
        {
            return Ok(TVal::Float(token));
        }
        Ok(TVal::Raw(token))
    }

    fn read_scalar_token(&mut self) -> std::result::Result<String, String> {
        let start = self.i;
        while matches!(self.peek(), Some(ch) if !ch.is_whitespace() && !matches!(ch, '#' | ',' | ']' | '}'))
        {
            self.bump();
        }
        if self.i == start {
            return Err(self.err("expected value"));
        }
        Ok(self.s[start..self.i].to_string())
    }

    fn parse_array(&mut self) -> std::result::Result<TVal, String> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek() == Some(']') {
                self.bump();
                break;
            }
            items.push(self.parse_value()?);
            self.skip_ws_and_comments();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "expected , or ] in array, got {other:?} at byte {}",
                        self.i
                    ));
                }
            }
        }
        Ok(TVal::Array(items))
    }

    fn parse_inline_table(&mut self) -> std::result::Result<TVal, String> {
        self.bump();
        let mut table = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek() == Some('}') {
                self.bump();
                break;
            }
            let key = self.parse_key()?;
            self.skip_inline();
            if self.bump() != Some('=') {
                return Err(self.err("expected = in inline table"));
            }
            let value = self.parse_value()?;
            insert_path(&mut table, &key, value)?;
            self.skip_ws_and_comments();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some('}') => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(format!(
                        "expected , or }} in inline table, got {other:?} at byte {}",
                        self.i
                    ));
                }
            }
        }
        Ok(TVal::Table(table))
    }
}

fn parse_toml(input: &str) -> std::result::Result<Vec<(String, TVal)>, String> {
    let s = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut parser = Parser { s, i: 0 };
    let mut root = Vec::new();
    let mut current: Vec<String> = Vec::new();
    parser.skip_ws_and_comments();
    while !parser.eof() {
        if parser.rest().starts_with("[[") {
            return Err(parser.err("TOML array-of-tables is not supported"));
        }
        if parser.peek() == Some('[') {
            parser.bump();
            let path = parser.parse_key()?;
            parser.skip_inline();
            if parser.bump() != Some(']') {
                return Err(parser.err("expected ] after table header"));
            }
            parser.finish_line()?;
            ensure_table(&mut root, &path)?;
            current = path;
        } else {
            let key = parser.parse_key()?;
            parser.skip_inline();
            if parser.bump() != Some('=') {
                return Err(parser.err("expected = after key"));
            }
            let value = parser.parse_value()?;
            parser.finish_line()?;
            let mut full = current.clone();
            full.extend(key);
            insert_path(&mut root, &full, value)?;
        }
        parser.skip_ws_and_comments();
    }
    Ok(root)
}

fn parse_int_token(token: &str) -> Option<i64> {
    let (neg, rest) = if let Some(stripped) = token.strip_prefix('+') {
        (false, stripped)
    } else if let Some(stripped) = token.strip_prefix('-') {
        (true, stripped)
    } else {
        (false, token)
    };
    if rest.is_empty()
        || rest.starts_with('_')
        || rest.ends_with('_')
        || rest.contains("__")
        || !rest.chars().all(|ch| ch.is_ascii_digit() || ch == '_')
    {
        return None;
    }
    let digits: String = rest.chars().filter(|ch| *ch != '_').collect();
    let value: i64 = digits.parse().ok()?;
    if neg {
        value.checked_neg()
    } else {
        Some(value)
    }
}

fn is_bare(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}
