use agent_core::{paths::StoragePaths, AgentError, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

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
            "claude" => Self::Claude,
            "opencode" => Self::OpenCode,
            "cursor" => Self::Cursor,
            "gemini" | "gemini-cli" => Self::Gemini,
            "zed" => Self::Zed,
            _ => Self::Generic,
        }
    }

    pub fn default_config_path(&self) -> Option<PathBuf> {
        let home = StoragePaths::home_dir().ok()?;
        match self {
            Self::Codex => Some(home.join(".config/codex/mcp.json")),
            Self::Claude => Some(home.join(".config/Claude/claude_desktop_config.json")),
            Self::OpenCode => Some(home.join(".config/opencode/mcp.json")),
            Self::Cursor => Some(home.join(".cursor/mcp.json")),
            Self::Gemini => Some(home.join(".gemini/antigravity-cli/mcp_config.json")),
            Self::Zed => Some(home.join(".config/zed/settings.json")),
            Self::Generic => None,
        }
    }
}

pub struct AgentConnector;

impl AgentConnector {
    /// Generate MCP configuration snippet for target agent.
    pub fn generate_config(target: &AgentTarget) -> serde_json::Value {
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

    /// Connect and write or print the MCP configuration for a coding agent.
    pub fn connect(target_name: &str, write_to_file: bool) -> Result<String> {
        let target = AgentTarget::parse(target_name);
        let config = Self::generate_config(&target);
        let config_str = serde_json::to_string_pretty(&config)
            .map_err(|e| AgentError::General(format!("JSON serialization error: {}", e)))?;

        if write_to_file {
            if let Some(path) = target.default_config_path() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }

                let final_json = if path.exists() {
                    let existing_text =
                        fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
                    let mut existing_val: Value =
                        serde_json::from_str(&existing_text).unwrap_or_else(|_| json!({}));
                    merge_server_entry(&target, &mut existing_val);
                    serde_json::to_string_pretty(&existing_val)
                        .unwrap_or_else(|_| config_str.clone())
                } else {
                    config_str.clone()
                };

                fs::write(&path, &final_json)?;
                return Ok(format!(
                    "Successfully configured '{}' MCP server in {}",
                    target_name,
                    path.display()
                ));
            }
        }

        Ok(config_str)
    }
}

fn merge_server_entry(target: &AgentTarget, existing: &mut Value) {
    let Some(root) = existing.as_object_mut() else {
        return;
    };

    if matches!(target, AgentTarget::Zed) {
        let servers = root.entry("context_servers").or_insert_with(|| json!({}));
        if let Some(map) = servers.as_object_mut() {
            map.insert(
                "02".to_string(),
                json!({
                    "source": "custom",
                    "command": "02",
                    "args": ["mcp"],
                    "env": {}
                }),
            );
        }
        return;
    }

    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    if let Some(map) = servers.as_object_mut() {
        map.insert(
            "02".to_string(),
            json!({ "command": "02", "args": ["mcp"] }),
        );
    }
}
