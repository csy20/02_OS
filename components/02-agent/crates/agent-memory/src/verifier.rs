use agent_core::{
    types::{EvidenceMemory, MemoryStatus, RepoId},
    Result,
};
use agent_index::IndexDatabase;
use chrono::Utc;
use std::path::Path;

pub struct MemoryVerifier;

impl MemoryVerifier {
    /// Verify an evidence-backed memory against current Git HEAD and the SQLite symbol index.
    pub fn verify<P: AsRef<Path>>(
        memory: &EvidenceMemory,
        repo_root: P,
        repo_id: &RepoId,
        current_head: Option<&str>,
        db: &IndexDatabase,
    ) -> Result<EvidenceMemory> {
        let mut verified = memory.clone();
        if memory.status == MemoryStatus::Invalidated {
            return Ok(verified);
        }
        let root = repo_root.as_ref();

        let mut all_files_exist = true;
        let mut all_symbols_exist = true;
        let mut any_fingerprint_mismatch = false;

        for item in &memory.evidence {
            let file_path = root.join(&item.file);
            if !file_path.exists() {
                all_files_exist = false;
                break;
            }

            // Verify each referenced symbol
            for sym_name in &item.symbols {
                let found_symbols = db.find_symbols_by_name(repo_id, sym_name)?;
                if found_symbols.is_empty() {
                    // Check if file is non-code (manifest, markdown, config) where symbol names are keys
                    if !item.file.ends_with(".rs")
                        && !item.file.ends_with(".py")
                        && !item.file.ends_with(".c")
                        && !item.file.ends_with(".sh")
                        && !item.file.ends_with(".js")
                        && !item.file.ends_with(".ts")
                    {
                        // Non-code anchor: file existence is sufficient evidence
                        continue;
                    }

                    all_symbols_exist = false;
                    break;
                }

                // Check fingerprint if recorded
                if let Some(ref recorded_fp) = item.fingerprint {
                    if !found_symbols.iter().any(|s| &s.fingerprint == recorded_fp) {
                        any_fingerprint_mismatch = true;
                    }
                }
            }
        }

        if !all_files_exist || !all_symbols_exist {
            verified.status = MemoryStatus::Stale;
            verified.confidence = 0.0;
        } else if any_fingerprint_mismatch {
            verified.status = MemoryStatus::Degraded;
            verified.confidence = 0.70;
        } else {
            verified.status = MemoryStatus::Fresh;
            verified.confidence = 1.0;
            if let Some(head) = current_head {
                verified.valid_at = head.to_string();
            }
        }

        verified.updated_at = Utc::now();
        Ok(verified)
    }
}
