use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    #[serde(default = "default_max_file_size_kb")]
    pub max_file_size_kb: u64,

    #[serde(default = "default_max_context_tokens")]
    pub default_context_tokens: usize,

    #[serde(default = "default_exclude_secrets")]
    pub exclude_secrets: bool,

    #[serde(default = "default_true")]
    pub index_tests: bool,

    #[serde(default = "default_true")]
    pub index_docs: bool,

    #[serde(default)]
    pub custom_ignores: Vec<String>,
}

fn default_max_file_size_kb() -> u64 {
    2048 // 2MB cap to protect memory & CPU
}

fn default_max_context_tokens() -> usize {
    8000
}

fn default_exclude_secrets() -> bool {
    true
}

fn default_true() -> bool {
    true
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            max_file_size_kb: default_max_file_size_kb(),
            default_context_tokens: default_max_context_tokens(),
            exclude_secrets: default_exclude_secrets(),
            index_tests: true,
            index_docs: true,
            custom_ignores: vec![
                "target/".to_string(),
                "node_modules/".to_string(),
                "__pycache__/".to_string(),
                ".venv/".to_string(),
                "dist/".to_string(),
                "build/".to_string(),
                "out/".to_string(),
            ],
        }
    }
}

impl RepoConfig {
    pub const CONFIG_DIR: &'static str = ".02agent";
    pub const CONFIG_FILE: &'static str = "config.toml";

    pub fn load_or_default<P: AsRef<Path>>(repo_root: P) -> Self {
        let config_path = repo_root
            .as_ref()
            .join(Self::CONFIG_DIR)
            .join(Self::CONFIG_FILE);
        if config_path.is_file() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save_to_repo<P: AsRef<Path>>(&self, repo_root: P) -> Result<()> {
        let dir = repo_root.as_ref().join(Self::CONFIG_DIR);
        fs::create_dir_all(&dir)?;
        let file_path = dir.join(Self::CONFIG_FILE);
        let content = toml::to_string_pretty(self).map_err(crate::error::AgentError::TomlSer)?;
        fs::write(file_path, content)?;
        Ok(())
    }
}
