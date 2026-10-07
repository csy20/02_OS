use crate::evidence::EvidenceSnapshot;
use agent_core::{
    config::RepoConfig,
    types::{EvidenceItem, EvidenceMemory, MemoryStatus, RepoId, SecretPattern},
    Result,
};
use agent_index::IndexDatabase;
use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;

pub struct MemoryVerifier;

impl MemoryVerifier {
    /// Capture the live symbol or file baseline when evidence is recorded.
    pub fn capture_evidence<P: AsRef<Path>>(
        repo_root: P,
        file: &str,
        symbols: Vec<String>,
        commit: &str,
    ) -> Result<EvidenceItem> {
        let root = repo_root.as_ref();
        let max_bytes = RepoConfig::load_or_default(root)
            .max_file_size_kb
            .saturating_mul(1024);
        let snapshot = EvidenceSnapshot::load(root, file, max_bytes)?;
        let fingerprint = Some(snapshot.baseline(&symbols)?);
        Ok(EvidenceItem {
            file: file.to_string(),
            symbols,
            commit: commit.to_string(),
            fingerprint,
        })
    }

    /// Verify evidence against live files and its recorded baseline, independently of indexing.
    pub fn verify<P: AsRef<Path>>(
        memory: &EvidenceMemory,
        repo_root: P,
        repo_id: &RepoId,
        current_head: Option<&str>,
        db: &IndexDatabase,
    ) -> Result<EvidenceMemory> {
        let mut verified = memory.clone();
        verified.claim = SecretPattern::redact(&verified.claim);
        if memory.status == MemoryStatus::Invalidated {
            return Ok(verified);
        }
        let root = repo_root.as_ref();
        let max_bytes = RepoConfig::load_or_default(root)
            .max_file_size_kb
            .saturating_mul(1024);
        let _ = (repo_id, db);
        let mut missing = false;
        let mut mismatch = false;
        let mut baselines = Vec::new();
        let mut snapshots = HashMap::new();
        for item in &memory.evidence {
            // A memory may cite multiple declarations in one file. Read and
            // parse that file once per verification, keeping one live snapshot.
            let snapshot = match snapshots
                .entry(item.file.clone())
                .or_insert_with(|| EvidenceSnapshot::load(root, &item.file, max_bytes).ok())
            {
                Some(snapshot) => snapshot,
                None => {
                    missing = true;
                    break;
                }
            };
            if !snapshot.symbols_exist(&item.symbols) {
                missing = true;
                break;
            }
            mismatch |= !snapshot.matches(item);
            baselines.push(snapshot.baseline(&item.symbols)?);
        }
        if missing {
            verified.status = MemoryStatus::Stale;
            verified.confidence = 0.0;
        } else if mismatch {
            verified.status = MemoryStatus::Degraded;
            verified.confidence = 0.70;
        } else {
            verified.status = MemoryStatus::Fresh;
            verified.confidence = 1.0;
            // Explicit successful verification can anchor legacy memories lacking a baseline.
            for (item, baseline) in verified.evidence.iter_mut().zip(baselines) {
                if item.fingerprint.is_none() {
                    item.fingerprint = Some(baseline);
                }
            }
            if let Some(head) = current_head {
                verified.valid_at = head.to_string();
            }
        }
        verified.updated_at = Utc::now();
        Ok(verified)
    }
}
