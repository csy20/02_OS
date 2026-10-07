use agent_core::{paths::StoragePaths, MemoryStatus};
use agent_mcp::call_tool;
use agent_memory::{index_pipeline, MemoryStore, RepoHandle, TaskRegistry};
use git2::Repository;
use serde_json::{json, Value};
use std::{fs, path::Path};

fn commit(root: &Path) {
    let repo = Repository::open(root).unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = git2::Signature::now("Audit", "audit@example.invalid").unwrap();
    let parents: Vec<_> = repo
        .head()
        .ok()
        .map(|head| head.peel_to_commit().unwrap())
        .into_iter()
        .collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "evidence fixture",
        &tree,
        &parents.iter().collect::<Vec<_>>(),
    )
    .unwrap();
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    Repository::init(dir.path()).unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::write(
        dir.path().join("src/auth.rs"),
        "pub fn authenticate() -> bool { true }\npub fn unrelated() -> bool { true }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("docs/rule.md"),
        "auditprobe accepts requests\n",
    )
    .unwrap();
    commit(dir.path());
    reindex(dir.path());
    dir
}

fn reindex(root: &Path) {
    let mut handle = RepoHandle::open(root).unwrap();
    index_pipeline(&mut handle, false, &TaskRegistry::new()).unwrap();
}

fn tool(root: &Path, name: &str, arguments: Value) -> Value {
    let response = call_tool(root, name, &arguments);
    assert!(!response.is_error, "{}", response.content[0].text);
    serde_json::from_str(&response.content[0].text).unwrap()
}

fn context(root: &Path) -> Value {
    tool(
        root,
        "repository_context",
        json!({"task":"auditprobe authenticate", "token_budget":8000}),
    )
}

#[test]
fn public_memory_writes_anchor_symbols_and_files_across_reindex_and_commits() {
    let dir = fixture();
    let root = dir.path();
    for (id, file, symbols) in [
        ("symbol", "src/auth.rs", vec!["authenticate"]),
        ("whole_file", "src/auth.rs", vec![]),
        ("document", "docs/rule.md", vec![]),
    ] {
        let reply = tool(
            root,
            "memory_write",
            json!({"id":id,"claim":"auditprobe accepts requests",
            "file":file,"symbols":symbols}),
        );
        assert!(reply["memory"]["evidence"][0]["fingerprint"]
            .as_str()
            .is_some());
    }
    // An unrelated declaration edit changes file evidence, but not selected-symbol evidence.
    fs::write(
        root.join("src/auth.rs"),
        "pub fn authenticate() -> bool { true }\npub fn unrelated() -> bool { false }\n",
    )
    .unwrap();
    reindex(root);
    let package = context(root);
    assert!(package["verified_memories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == "symbol"));
    assert!(package["diagnostic_memories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == "whole_file"));

    fs::write(
        root.join("src/auth.rs"),
        "pub fn authenticate() -> bool { false }\npub fn unrelated() -> bool { false }\n",
    )
    .unwrap();
    fs::write(root.join("docs/rule.md"), "auditprobe rejects requests\n").unwrap();
    // Detect before indexing, then exercise the original false-recovery path.
    assert!(context(root)["verified_memories"]
        .as_array()
        .unwrap()
        .is_empty());
    reindex(root);
    assert!(context(root)["verified_memories"]
        .as_array()
        .unwrap()
        .is_empty());
    commit(root);
    reindex(root);
    let clean = context(root);
    assert!(clean["verified_memories"].as_array().unwrap().is_empty());
    assert_eq!(clean["diagnostic_memories"].as_array().unwrap().len(), 3);
    assert!(clean["diagnostic_memories"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["status"] == "Degraded"));
}

#[test]
fn memory_claim_redaction_covers_public_replies_context_and_legacy_diagnostics() {
    let dir = fixture();
    let root = dir.path();
    let token = format!("ghp_{}", "a".repeat(36));
    let reply = tool(
        root,
        "memory_write",
        json!({"id":"rule","claim":format!("auditprobe {token}"),
        "file":"src/auth.rs","symbols":["authenticate"]}),
    );
    assert_eq!(reply["memory"]["claim"], "auditprobe [REDACTED]");
    for name in ["memory_get", "memory_status"] {
        let value = tool(root, name, json!({"id":"rule"}));
        assert!(!value.to_string().contains(&token));
        assert!(value.to_string().contains("[REDACTED]"));
    }
    assert!(!context(root).to_string().contains(&token));
    let info = RepoHandle::open(root).unwrap().info;
    let path = StoragePaths::repo_memories_path(&info.id).unwrap();
    assert!(!fs::read_to_string(&path).unwrap().contains(&token));
    let store = MemoryStore::for_repo(&info.id).unwrap();
    let mut legacy = store.get("rule").unwrap().unwrap();
    legacy.status = MemoryStatus::Invalidated;
    legacy.claim = format!("auditprobe legacy {token}");
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string(&legacy).unwrap()),
    )
    .unwrap();
    let package = context(root);
    assert!(package["verified_memories"].as_array().unwrap().is_empty());
    assert_eq!(
        package["diagnostic_memories"][0]["claim"],
        "auditprobe legacy [REDACTED]"
    );
    assert!(!package.to_string().contains(&token));
}

#[test]
fn memory_write_rejects_missing_symbols_in_the_cited_file() {
    let dir = fixture();
    let response = call_tool(
        dir.path(),
        "memory_write",
        &json!({"id":"missing", "claim":"auditprobe",
        "file":"src/auth.rs", "symbols":["does_not_exist"]}),
    );
    assert!(response.is_error);
}

#[test]
fn legacy_fresh_memories_compare_their_cited_commit_after_direct_reindex() {
    let dir = fixture();
    let root = dir.path();
    tool(
        root,
        "memory_write",
        json!({"id":"legacy", "claim":"auditprobe accepts requests",
        "file":"src/auth.rs", "symbols":["authenticate"]}),
    );
    let info = RepoHandle::open(root).unwrap().info;
    let store = MemoryStore::for_repo(&info.id).unwrap();
    let mut memory = store.get("legacy").unwrap().unwrap();
    memory.evidence[0].fingerprint = None;
    store.save(&memory).unwrap();
    // No context or staleness evaluation happens before the catalog replacement.
    fs::write(
        root.join("src/auth.rs"),
        "pub fn authenticate() -> bool { false }\n",
    )
    .unwrap();
    reindex(root);
    let package = context(root);
    assert!(package["verified_memories"].as_array().unwrap().is_empty());
    assert_eq!(package["diagnostic_memories"][0]["status"], "Degraded");
    assert!(store.get("legacy").unwrap().unwrap().evidence[0]
        .fingerprint
        .is_some());
    commit(root);
    reindex(root);
    assert!(context(root)["verified_memories"]
        .as_array()
        .unwrap()
        .is_empty());
    // A missing historic baseline fails closed instead of trusting the fresh catalog.
    memory.evidence[0].commit = "unavailable".into();
    store.save(&memory).unwrap();
    assert!(context(root)["verified_memories"]
        .as_array()
        .unwrap()
        .is_empty());
}
