use agent_context::estimate_tokens;
use agent_core::{
    config::RepoConfig,
    paths::StoragePaths,
    types::{EvidenceItem, EvidenceMemory, MemoryKind, MemoryStatus, RepoId, RepoInfo},
};
use agent_index::{IndexDatabase, RepoScanner};
use agent_mcp::call_tool;
use agent_memory::MemoryStore;
use chrono::Utc;
use git2::Repository;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

#[test]
fn git_history_tool_redacts_synthetic_tokens() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let repo = Repository::init(root).unwrap();
    let sig = git2::Signature::now("Tester", "test@02os.org").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn rotate_refresh_token() {}\n",
    )
    .unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("src/lib.rs")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let token = format!("ghp_{}", "x".repeat(36));
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("rotate {token} now"),
        &tree,
        &[],
    )
    .unwrap();

    let result = call_tool(root, "git_history", &json!({ "limit": 5 }));
    assert!(!result.is_error, "{}", result.content[0].text);
    let text = &result.content[0].text;
    assert!(text.contains("[REDACTED]"));
    assert!(!text.contains(&token));
}

#[test]
fn context_tool_budgets_the_serialized_payload() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let repo = Repository::init(root).unwrap();
    let sig = git2::Signature::now("Tester", "test@02os.org").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/probe.rs"), "pub fn budgetprobe() {}\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_path(std::path::Path::new("src/probe.rs"))
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let commit_id = repo
        .commit(Some("HEAD"), &sig, &sig, "probe", &tree, &[])
        .unwrap();

    let repo_id = RepoId::from_path(root);
    let mut db = IndexDatabase::open(StoragePaths::repo_db_path(&repo_id).unwrap()).unwrap();
    db.update_repo_info(&RepoInfo {
        id: repo_id.clone(),
        name: "budget".into(),
        root_path: root.to_path_buf(),
        head_commit: Some(commit_id.to_string()),
        branch: Some("master".into()),
        is_clean: true,
        modified_count: 0,
        untracked_count: 0,
    })
    .unwrap();
    let scanned = RepoScanner::new(root, RepoConfig::default()).scan();
    db.save_scanned_files(&repo_id, &scanned).unwrap();
    let mut symbols = Vec::new();
    let mut references = Vec::new();
    for item in &scanned {
        if let Some(text) = &item.content {
            let extracted = agent_parser::CodeExtractor::extract(
                &item.file.relative_path,
                text,
                item.file.language,
            )
            .unwrap();
            symbols.extend(extracted.symbols);
            references.extend(extracted.references);
        }
    }
    db.save_symbols_and_references(&repo_id, &symbols, &references)
        .unwrap();
    drop(db);

    let store = MemoryStore::for_repo(&repo_id).unwrap();
    let mut claims = HashMap::new();
    let budget = 1000usize;
    for n in 0..24 {
        let id = format!("mem_{n:02}");
        let claim = format!("budgetprobe fact {id} {}", "context padding ".repeat(24));
        assert!(estimate_tokens(&claim) < budget);
        claims.insert(id.clone(), claim.clone());
        store
            .save(&EvidenceMemory {
                id,
                claim,
                kind: MemoryKind::ArchitecturalFact,
                evidence: vec![EvidenceItem {
                    file: "src/probe.rs".into(),
                    symbols: vec!["budgetprobe".into()],
                    commit: commit_id.to_string(),
                    fingerprint: None,
                }],
                valid_at: commit_id.to_string(),
                confidence: 1.0,
                status: MemoryStatus::Fresh,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
    }

    let result = call_tool(
        root,
        "repository_context",
        &json!({ "task": "budgetprobe", "token_budget": budget }),
    );
    assert!(!result.is_error, "{}", result.content[0].text);
    let text = &result.content[0].text;
    let tokens = estimate_tokens(text);
    assert!(tokens <= budget);
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
    assert!(parsed.get("markdown").is_none());
    assert_eq!(parsed["task"], "budgetprobe");
    assert_eq!(
        parsed["budget"]["returned_tokens"].as_u64().unwrap() as usize,
        tokens
    );
    assert_eq!(
        parsed["budget"]["budget_limit"].as_u64().unwrap(),
        budget as u64
    );
    let memories = parsed["verified_memories"].as_array().unwrap();
    assert!(!memories.is_empty());
    assert!(memories.len() < claims.len());
    for memory in memories {
        let id = memory["id"].as_str().unwrap();
        assert_eq!(memory["claim"].as_str().unwrap(), claims[id]);
        assert!(memory["evidence"].is_array());
    }
}
