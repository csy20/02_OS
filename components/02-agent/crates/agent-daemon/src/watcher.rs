use agent_core::{config::RepoConfig, paths::StoragePaths, Result};
use agent_git::GitRepo;
use agent_index::{IndexDatabase, RepoScanner};
use agent_memory::{MemoryStore, StalenessEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchedState {
    pub repositories: HashSet<PathBuf>,
}

pub struct RepoWatcher {
    state_file: PathBuf,
}

impl RepoWatcher {
    pub fn new() -> Result<Self> {
        let dir = StoragePaths::data_dir()?;
        fs::create_dir_all(&dir)?;
        Ok(Self {
            state_file: dir.join("watched_repos.json"),
        })
    }

    pub fn load_state(&self) -> WatchedState {
        if self.state_file.exists() {
            if let Ok(text) = fs::read_to_string(&self.state_file) {
                if let Ok(state) = serde_json::from_str(&text) {
                    return state;
                }
            }
        }
        WatchedState::default()
    }

    pub fn save_state(&self, state: &WatchedState) -> Result<()> {
        let text = serde_json::to_string_pretty(state)
            .map_err(|e| agent_core::AgentError::General(format!("Serialize error: {}", e)))?;
        fs::write(&self.state_file, text)?;
        Ok(())
    }

    pub fn add(&self, path: PathBuf) -> Result<bool> {
        let mut state = self.load_state();
        let inserted = state.repositories.insert(path);
        if inserted {
            self.save_state(&state)?;
        }
        Ok(inserted)
    }

    pub fn remove(&self, path: &Path) -> Result<bool> {
        let mut state = self.load_state();
        let removed = state.repositories.remove(path);
        if removed {
            self.save_state(&state)?;
        }
        Ok(removed)
    }

    pub fn list(&self) -> Vec<PathBuf> {
        self.load_state().repositories.into_iter().collect()
    }

    /// Background tick: check every watched repo and incrementally update if changes detected.
    pub fn sync_all(&self) {
        let state = self.load_state();
        for repo_path in state.repositories {
            if !repo_path.exists() {
                continue;
            }

            let git_repo = match GitRepo::open(&repo_path) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let repo_info = match git_repo.info() {
                Ok(i) => i,
                Err(_) => continue,
            };

            let db_path = match StoragePaths::repo_db_path(&repo_info.id) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if !db_path.exists() {
                continue;
            }

            let mut db = match IndexDatabase::open(&db_path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let stats = match db.get_stats(&repo_info.id) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Check if commit changed or working tree is dirty
            let needs_sync =
                stats.last_indexed_commit != repo_info.head_commit || repo_info.modified_count > 0;

            if needs_sync {
                let config = RepoConfig::load_or_default(&repo_path);
                let scanner = RepoScanner::new(&repo_path, config);
                let mut scanned = scanner.scan();

                for item in &mut scanned {
                    if let Ok(Some(blob_id)) = git_repo.get_blob_id(&item.file.relative_path) {
                        item.file.git_blob_id = Some(blob_id);
                    }
                }

                let existing_files = db.list_files(&repo_info.id).unwrap_or_default();
                let mut existing_map = std::collections::HashMap::new();
                for f in existing_files {
                    existing_map.insert(f.relative_path, f.file_hash);
                }

                let mut current_paths = std::collections::HashSet::new();
                let mut files_to_update = Vec::new();

                for item in scanned {
                    current_paths.insert(item.file.relative_path.clone());
                    let needs_update = match existing_map.get(&item.file.relative_path) {
                        Some(old_hash) => old_hash != &item.file.file_hash,
                        None => true,
                    };

                    if needs_update {
                        files_to_update.push(item);
                    }
                }

                let mut files_to_delete = Vec::new();
                for old_path in existing_map.keys() {
                    if !current_paths.contains(old_path) {
                        files_to_delete.push(old_path.clone());
                    }
                }

                let mut new_symbols = Vec::new();
                let mut new_references = Vec::new();

                for item in &files_to_update {
                    if item.file.kind == agent_core::types::FileKind::Source || item.file.is_test {
                        if let Some(ref text) = item.content {
                            if let Ok(extracted) = agent_parser::CodeExtractor::extract(
                                &item.file.relative_path,
                                text,
                                item.file.language,
                            ) {
                                new_symbols.extend(extracted.symbols);
                                new_references.extend(extracted.references);
                            }
                        }
                    }
                }

                let _ = db.update_incremental(
                    &repo_info.id,
                    &files_to_update,
                    &files_to_delete,
                    &new_symbols,
                    &new_references,
                    repo_info.head_commit.as_deref(),
                );

                if let Ok(store) = MemoryStore::for_repo(&repo_info.id) {
                    let _ = StalenessEngine::evaluate(
                        &repo_path,
                        &repo_info.id,
                        &git_repo,
                        &db,
                        &store,
                    );
                }
            }
        }
    }
}
