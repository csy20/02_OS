use crate::error::{AgentError, Result};
use crate::types::RepoId;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

pub struct StoragePaths;

impl StoragePaths {
    /// User home directory: ~/
    pub fn home_dir() -> Result<PathBuf> {
        directories::BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
            .ok_or_else(|| AgentError::General("Could not determine user home directory".into()))
    }

    fn project_dirs() -> Result<ProjectDirs> {
        ProjectDirs::from("org", "02os", "02-agent")
            .ok_or_else(|| AgentError::General("Could not determine user home directory".into()))
    }

    /// Root data dir: ~/.local/share/02-agent
    pub fn data_dir() -> Result<PathBuf> {
        let dirs = Self::project_dirs()?;
        Ok(dirs.data_dir().to_path_buf())
    }

    /// Cache dir: ~/.cache/02-agent
    pub fn cache_dir() -> Result<PathBuf> {
        let dirs = Self::project_dirs()?;
        Ok(dirs.cache_dir().to_path_buf())
    }

    /// Config dir: ~/.config/02-agent
    pub fn config_dir() -> Result<PathBuf> {
        let dirs = Self::project_dirs()?;
        Ok(dirs.config_dir().to_path_buf())
    }

    /// Directory for a specific repository: ~/.local/share/02-agent/repos/<repo-id>/
    pub fn repo_dir(repo_id: &RepoId) -> Result<PathBuf> {
        let base = Self::data_dir()?;
        let path = base.join("repos").join(repo_id.as_str());
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Database path: ~/.local/share/02-agent/repos/<repo-id>/index.sqlite
    pub fn repo_db_path(repo_id: &RepoId) -> Result<PathBuf> {
        let dir = Self::repo_dir(repo_id)?;
        Ok(dir.join("index.sqlite"))
    }

    /// Memories path: ~/.local/share/02-agent/repos/<repo-id>/memories.jsonl
    pub fn repo_memories_path(repo_id: &RepoId) -> Result<PathBuf> {
        let dir = Self::repo_dir(repo_id)?;
        Ok(dir.join("memories.jsonl"))
    }

    /// Architecture facts: ~/.local/share/02-agent/repos/<repo-id>/architecture.jsonl
    pub fn repo_arch_path(repo_id: &RepoId) -> Result<PathBuf> {
        let dir = Self::repo_dir(repo_id)?;
        Ok(dir.join("architecture.jsonl"))
    }

    /// Metadata path: ~/.local/share/02-agent/repos/<repo-id>/metadata.json
    pub fn repo_metadata_path(repo_id: &RepoId) -> Result<PathBuf> {
        let dir = Self::repo_dir(repo_id)?;
        Ok(dir.join("metadata.json"))
    }
}
