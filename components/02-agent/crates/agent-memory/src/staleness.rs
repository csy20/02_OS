use crate::store::MemoryStore;
use agent_core::{
    config::RepoConfig,
    contain::{contains_symlink, read_regular_text_within},
    types::{EvidenceMemory, Language, MemoryStatus, RepoId, SecretPattern},
    Result,
};
use agent_git::GitRepo;
use agent_index::scanner::custom_ignored;
use agent_index::IndexDatabase;
use agent_parser::CodeExtractor;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn is_code_evidence(path: &str) -> bool {
    Language::path_extracts_symbols(path)
}

struct LiveSymbols {
    names: HashSet<String>,
    fingerprints: HashMap<String, Vec<String>>,
}

fn live_symbols(root: &Path, relative: &str, max_bytes: u64) -> Option<LiveSymbols> {
    let path = Path::new(relative);
    let content = read_regular_text_within(root, path, max_bytes).ok()?;
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    let extracted =
        CodeExtractor::extract(relative, &content, Language::from_extension(extension)).ok()?;
    let mut names = HashSet::new();
    let mut fingerprints: HashMap<String, Vec<String>> = HashMap::new();
    for symbol in extracted.symbols {
        names.insert(symbol.name.clone());
        names.insert(symbol.qualified_name.clone());
        fingerprints
            .entry(symbol.name.clone())
            .or_default()
            .push(symbol.fingerprint.clone());
        fingerprints
            .entry(symbol.qualified_name)
            .or_default()
            .push(symbol.fingerprint);
    }
    Some(LiveSymbols {
        names,
        fingerprints,
    })
}

fn fingerprint_matches(live: &LiveSymbols, symbol: &str, recorded: &str) -> bool {
    live.fingerprints
        .get(symbol)
        .map(|found| found.iter().any(|fingerprint| fingerprint == recorded))
        .unwrap_or(false)
}

struct SymbolChanges {
    pairs: HashSet<(String, String)>,
    names: HashSet<String>,
}

impl SymbolChanges {
    fn new() -> Self {
        Self {
            pairs: HashSet::new(),
            names: HashSet::new(),
        }
    }

    fn insert(&mut self, file: &str, name: &str, qualified: &str) {
        self.pairs.insert((file.to_string(), name.to_string()));
        if qualified != name {
            self.pairs.insert((file.to_string(), qualified.to_string()));
        }
        self.names.insert(name.to_string());
    }

    fn touches(&self, file: &str, symbol: &str) -> bool {
        self.pairs.contains(&(file.to_string(), symbol.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalenessReport {
    pub modified_files: Vec<String>,
    pub changed_symbols: Vec<String>,
    pub fresh_count: usize,
    pub degraded_count: usize,
    pub stale_count: usize,
    pub affected_memories: Vec<EvidenceMemory>,
}

pub struct StalenessEngine;

impl StalenessEngine {
    /// Detect changes in the Git working tree against the indexed state and update memory statuses.
    pub fn evaluate<P: AsRef<Path>>(
        repo_root: P,
        repo_id: &RepoId,
        git_repo: &GitRepo,
        db: &IndexDatabase,
        store: &MemoryStore,
    ) -> Result<StalenessReport> {
        let root = repo_root.as_ref();
        let diff_hunks = git_repo.get_diff_hunks()?;
        let config = RepoConfig::load_or_default(root);
        let max_bytes = config.max_file_size_kb.saturating_mul(1024);

        let mut modified_files = Vec::new();
        let mut changed_symbols = SymbolChanges::new();

        for file_diff in &diff_hunks {
            modified_files.push(file_diff.file_path.clone());
            let relative = Path::new(&file_diff.file_path);
            if SecretPattern::is_secret(relative)
                || custom_ignored(relative, &config.custom_ignores)
                || contains_symlink(root, relative)
            {
                continue;
            }

            if file_diff.is_deleted || !root.join(relative).exists() {
                let old_syms = db.find_symbols_by_file(repo_id, &file_diff.file_path)?;
                for s in old_syms {
                    changed_symbols.insert(&file_diff.file_path, &s.name, &s.qualified_name);
                }
                continue;
            }

            // Bounded, symlink-rejecting read. Oversized and external paths are skipped.
            let content = match read_regular_text_within(root, relative, max_bytes) {
                Ok(content) => content,
                Err(_) => continue,
            };

            let ext = relative.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang = Language::from_extension(ext);
            let fresh_extraction =
                CodeExtractor::extract(&file_diff.file_path, &content, lang).ok();

            if let Some(ref fresh) = fresh_extraction {
                let fresh_names: HashSet<&str> = fresh
                    .symbols
                    .iter()
                    .map(|symbol| symbol.name.as_str())
                    .collect();
                if let Ok(old_symbols) = db.find_symbols_by_file(repo_id, &file_diff.file_path) {
                    for old in old_symbols {
                        if !fresh_names.contains(old.name.as_str()) {
                            changed_symbols.insert(
                                &file_diff.file_path,
                                &old.name,
                                &old.qualified_name,
                            );
                        }
                    }
                }
            }

            for hunk in &file_diff.hunks {
                let hunk_range_start = hunk.new_start;
                let hunk_range_end = hunk.new_start + hunk.new_lines;

                if let Some(ref fresh) = fresh_extraction {
                    for sym in &fresh.symbols {
                        // Check if hunk overlaps with symbol line boundaries
                        if sym.start_line <= hunk_range_end && sym.end_line >= hunk_range_start {
                            // Check if fingerprint changed compared to database record
                            let existing = db.find_symbols_by_name(repo_id, &sym.name)?;
                            let matched = existing.iter().find(|e| e.file_path == sym.file_path);

                            if let Some(old) = matched {
                                if old.fingerprint != sym.fingerprint {
                                    changed_symbols.insert(
                                        &sym.file_path,
                                        &sym.name,
                                        &sym.qualified_name,
                                    );
                                }
                            } else {
                                // Brand new or moved symbol
                                changed_symbols.insert(
                                    &sym.file_path,
                                    &sym.name,
                                    &sym.qualified_name,
                                );
                            }
                        }
                    }
                }
            }
        }

        // Evaluate and update evidence memories
        let memories = store.load_all()?;
        let mut affected_memories = Vec::new();
        let mut fresh_count = 0;
        let mut degraded_count = 0;
        let mut stale_count = 0;
        let mut live_cache: HashMap<String, Option<LiveSymbols>> = HashMap::new();

        for mut mem in memories {
            let mut memory_changed = false;
            let mut has_stale_evidence = false;
            let mut has_degraded_evidence = false;

            for ev in &mem.evidence {
                let relative = Path::new(&ev.file);
                if contains_symlink(root, relative) || !root.join(relative).is_file() {
                    has_stale_evidence = true;
                    break;
                }

                if is_code_evidence(&ev.file) {
                    let live = live_cache
                        .entry(ev.file.clone())
                        .or_insert_with(|| live_symbols(root, &ev.file, max_bytes));
                    match live {
                        Some(live) => {
                            let symbols_supplied = !ev.symbols.is_empty();
                            for sym in &ev.symbols {
                                if !live.names.contains(sym) {
                                    has_stale_evidence = true;
                                } else if changed_symbols.touches(&ev.file, sym) {
                                    has_degraded_evidence = true;
                                }
                            }
                            if symbols_supplied {
                                if let Some(recorded) = &ev.fingerprint {
                                    let mismatched = ev.symbols.iter().any(|sym| {
                                        live.names.contains(sym)
                                            && !fingerprint_matches(live, sym, recorded)
                                    });
                                    if mismatched {
                                        has_degraded_evidence = true;
                                    }
                                }
                            }
                        }
                        None => has_stale_evidence = true,
                    }
                } else {
                    for sym in &ev.symbols {
                        if changed_symbols.touches(&ev.file, sym) {
                            has_degraded_evidence = true;
                        }
                    }
                }
            }

            if mem.status == MemoryStatus::Invalidated {
                stale_count += 1;
                continue;
            }

            let old_status = mem.status;
            if has_stale_evidence {
                mem.status = MemoryStatus::Stale;
                mem.confidence = 0.0;
            } else if has_degraded_evidence {
                mem.status = MemoryStatus::Degraded;
                mem.confidence = 0.70;
            } else {
                // No changed symbols touching this memory -> preserve fresh status!
                mem.status = MemoryStatus::Fresh;
                mem.confidence = 1.0;
            }

            if mem.status != old_status {
                mem.updated_at = Utc::now();
                memory_changed = true;
                affected_memories.push(mem.clone());
            }

            match mem.status {
                MemoryStatus::Fresh => fresh_count += 1,
                MemoryStatus::Degraded => degraded_count += 1,
                MemoryStatus::Stale | MemoryStatus::Invalidated => stale_count += 1,
            }

            if memory_changed {
                store.save(&mem)?;
            }
        }

        let mut sym_vec: Vec<String> = changed_symbols.names.into_iter().collect();
        sym_vec.sort();

        Ok(StalenessReport {
            modified_files,
            changed_symbols: sym_vec,
            fresh_count,
            degraded_count,
            stale_count,
            affected_memories,
        })
    }
}
