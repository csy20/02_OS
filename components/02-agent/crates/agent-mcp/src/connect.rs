use agent_core::{paths::StoragePaths, AgentError, Result};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

pub enum AgentTarget {
    Codex,
    Claude,
    OpenCode,
    Generic,
}

impl AgentTarget {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "codex" => Self::Codex,
            "claude" => Self::Claude,
            "opencode" => Self::OpenCode,
            _ => Self::Generic,
        }
    }

    pub fn default_config_path(&self) -> Option<PathBuf> {
        let home = StoragePaths::home_dir().ok()?;
        match self {
            Self::Codex => Some(home.join(".config/codex/mcp.json")),
            Self::Claude => Some(home.join(".config/Claude/claude_desktop_config.json")),
            Self::OpenCode => Some(home.join(".config/opencode/mcp.json")),
            Self::Generic => None,
        }
    }
}

pub struct AgentConnector;

impl AgentConnector {
    /// Generate MCP configuration snippet for target agent.
    pub fn generate_config(_target: &AgentTarget) -> serde_json::Value {
        json!({
            "mcpServers": {
                "02": {
                    "command": "02",
                    "args": ["mcp"]
                }
            }
        })
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

                // If existing file, merge mcpServers
                let final_json = if path.exists() {
                    let existing_text =
                        fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
                    let mut existing_val: serde_json::Value =
                        serde_json::from_str(&existing_text).unwrap_or_else(|_| json!({}));

                    if let Some(servers) = existing_val
                        .get_mut("mcpServers")
                        .and_then(|s| s.as_object_mut())
                    {
                        servers.insert(
                            "02".to_string(),
                            json!({ "command": "02", "args": ["mcp"] }),
                        );
                    } else if let Some(root_obj) = existing_val.as_object_mut() {
                        root_obj.insert(
                            "mcpServers".to_string(),
                            json!({
                                "02": { "command": "02", "args": ["mcp"] }
                            }),
                        );
                    }
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
