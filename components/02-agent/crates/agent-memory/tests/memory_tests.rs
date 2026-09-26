use agent_core::types::{EvidenceItem, EvidenceMemory, MemoryKind, MemoryStatus, RepoId};
use agent_index::IndexDatabase;
use agent_memory::{MemoryStore, MemoryVerifier};
use chrono::Utc;
use tempfile::tempdir;

#[test]
fn test_memory_store_crud() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("memories.jsonl");
    let store = MemoryStore::new(file_path);

    let mem = EvidenceMemory {
        id: "mem_auth_17".to_string(),
        claim: "Refresh tokens are single use and rotated upon successful exchange".to_string(),
        kind: MemoryKind::ArchitecturalFact,
        evidence: vec![EvidenceItem {
            file: "src/auth/refresh.rs".to_string(),
            symbols: vec!["rotate_refresh_token".to_string()],
            commit: "a812fe3".to_string(),
            fingerprint: None,
        }],
        valid_at: "a812fe3".to_string(),
        confidence: 1.0,
        status: MemoryStatus::Fresh,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    store.save(&mem).unwrap();

    let all = store.load_all().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, "mem_auth_17");

    let found = store.get("mem_auth_17").unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().claim, mem.claim);

    let deleted = store.delete("mem_auth_17").unwrap();
    assert!(deleted);
    assert_eq!(store.load_all().unwrap().len(), 0);
}

#[test]
fn test_memory_verifier_staleness() {
    let dir = tempdir().unwrap();
    let repo_id = RepoId::from_path(dir.path());
    let db = IndexDatabase::open_in_memory().unwrap();

    let mem = EvidenceMemory {
        id: "mem_missing_file".to_string(),
        claim: "Non-existent file test".to_string(),
        kind: MemoryKind::ArchitecturalFact,
        evidence: vec![EvidenceItem {
            file: "does_not_exist.rs".to_string(),
            symbols: vec!["fake_symbol".to_string()],
            commit: "1111111".to_string(),
            fingerprint: None,
        }],
        valid_at: "1111111".to_string(),
        confidence: 1.0,
        status: MemoryStatus::Fresh,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let verified =
        MemoryVerifier::verify(&mem, dir.path(), &repo_id, Some("2222222"), &db).unwrap();
    assert_eq!(verified.status, MemoryStatus::Stale);
    assert_eq!(verified.confidence, 0.0);
}
