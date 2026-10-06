use agent_core::{
    types::{FileKind, IndexedFile, Language},
    EdgeKind, Provenance, RepoId, RepoInfo, SourceKind,
};
use agent_index::{
    chunk_owner_path, IndexDatabase, NewDataPoint, PutPoint, RepoScanner, ScannedFile,
};
use chrono::Utc;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use tempfile::tempdir;

fn repo_info(root: &std::path::Path, repo_id: RepoId) -> RepoInfo {
    RepoInfo {
        id: repo_id,
        name: "regression".into(),
        root_path: root.to_path_buf(),
        head_commit: None,
        branch: None,
        is_clean: true,
        modified_count: 0,
        untracked_count: 0,
    }
}

fn sample_tree(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("src/auth.rs"),
        "pub fn rotate_refresh_token() {}\n",
    )
    .unwrap();
    fs::write(root.join("tests/auth_test.rs"), "fn test_rotate() {}\n").unwrap();
    fs::write(root.join("docs/architecture.md"), "# Architecture\n").unwrap();
    fs::write(root.join(".env"), "TOKEN=secret\n").unwrap();
}

#[test]
fn symlinks_are_not_indexed() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    sample_tree(root);
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("leak.txt"), "CANARY-EXTERNAL-BYTES\n").unwrap();
    symlink(
        outside.path().join("leak.txt"),
        root.join("src/absolute_link.rs"),
    )
    .unwrap();
    symlink("../leak-rel.txt", root.join("src/relative_link.rs")).unwrap();
    fs::write(
        outside.path().join("leak-rel.txt"),
        "CANARY-RELATIVE-BYTES\n",
    )
    .unwrap();
    symlink(root.join(".env"), root.join("src/env_link.rs")).unwrap();
    symlink("auth.rs", root.join("src/alias.rs")).unwrap();
    symlink(outside.path(), root.join("linked-dir")).unwrap();
    fs::write(outside.path().join("nested.rs"), "CANARY-DIR-BYTES\n").unwrap();
    symlink(root.join("missing.rs"), root.join("src/broken.rs")).unwrap();

    let scanned = RepoScanner::new(root, agent_core::RepoConfig::default()).scan();
    let blob = scanned
        .iter()
        .map(|item| {
            format!(
                "{}:{}",
                item.file.relative_path,
                item.content.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!blob.contains("CANARY-EXTERNAL-BYTES"));
    assert!(!blob.contains("CANARY-RELATIVE-BYTES"));
    assert!(!blob.contains("CANARY-DIR-BYTES"));
    assert!(!blob.contains("TOKEN=secret"));
    assert!(!scanned
        .iter()
        .any(|item| item.file.relative_path.contains("link")
            || item.file.relative_path.ends_with("alias.rs")
            || item.file.relative_path.ends_with("broken.rs")));
    assert!(scanned
        .iter()
        .any(|item| item.file.relative_path == "src/auth.rs"));
}

#[test]
fn directory_ignores_do_not_swallow_similar_file_names() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("build.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("targeting.py"), "x = 1\n").unwrap();
    fs::write(root.join("distill.rs"), "fn distill() {}\n").unwrap();
    fs::write(root.join("output.rs"), "fn output() {}\n").unwrap();
    fs::create_dir_all(root.join("build")).unwrap();
    fs::write(root.join("build/hidden.rs"), "fn hidden() {}\n").unwrap();
    fs::create_dir_all(root.join("src/build")).unwrap();
    fs::write(root.join("src/build/nested.rs"), "fn nested() {}\n").unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("target/skip.rs"), "fn skip() {}\n").unwrap();

    let scanned = RepoScanner::new(root, agent_core::RepoConfig::default()).scan();
    let paths: Vec<_> = scanned
        .iter()
        .map(|item| item.file.relative_path.as_str())
        .collect();
    assert!(paths.contains(&"build.rs"));
    assert!(paths.contains(&"targeting.py"));
    assert!(paths.contains(&"distill.rs"));
    assert!(paths.contains(&"output.rs"));
    assert!(!paths
        .iter()
        .any(|path| path.starts_with("build/") || path.contains("/build/")));
    assert!(!paths.iter().any(|path| path.starts_with("target/")));
}

#[test]
fn index_flags_drop_tests_and_docs_from_catalog() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    sample_tree(root);
    let repo_id = RepoId::from_path(root);
    let mut db = IndexDatabase::open_in_memory().unwrap();
    db.update_repo_info(&repo_info(root, repo_id.clone()))
        .unwrap();

    let full = RepoScanner::new(root, agent_core::RepoConfig::default()).scan();
    assert_eq!(full.len(), 3);
    db.save_scanned_files(&repo_id, &full).unwrap();

    let docs_off = agent_core::RepoConfig {
        index_docs: false,
        ..agent_core::RepoConfig::default()
    };
    let tests_off = agent_core::RepoConfig {
        index_tests: false,
        ..agent_core::RepoConfig::default()
    };
    let both_off = agent_core::RepoConfig {
        index_tests: false,
        index_docs: false,
        ..agent_core::RepoConfig::default()
    };

    let docs = RepoScanner::new(root, docs_off).scan();
    assert!(docs
        .iter()
        .all(|item| item.file.relative_path != "docs/architecture.md"));
    assert!(docs
        .iter()
        .any(|item| item.file.relative_path == "tests/auth_test.rs"));
    let tests = RepoScanner::new(root, tests_off).scan();
    assert!(tests
        .iter()
        .all(|item| item.file.relative_path != "tests/auth_test.rs"));
    assert!(tests
        .iter()
        .any(|item| item.file.relative_path == "docs/architecture.md"));

    let narrowed = RepoScanner::new(root, both_off).scan();
    assert_eq!(narrowed.len(), 1);
    db.save_scanned_files(&repo_id, &narrowed).unwrap();
    let listed = db.list_files(&repo_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].relative_path, "src/auth.rs");
    let hits = db.search_content("Architecture", 10).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn content_reverting_to_an_older_body_becomes_the_latest_version() {
    let mut db = IndexDatabase::open_in_memory().unwrap();
    let repo_id = RepoId::new("live");
    db.update_repo_info(&repo_info(
        std::path::Path::new("/tmp/live"),
        repo_id.clone(),
    ))
    .unwrap();
    let dataset = db.ensure_dataset(&repo_id, "default").unwrap();
    let point = |content: &str| NewDataPoint {
        repo_id: repo_id.clone(),
        dataset_id: dataset.id.clone(),
        kind: "file".into(),
        path: "src/auth.rs".into(),
        content: content.into(),
        provenance: Provenance {
            source_kind: SourceKind::Worktree,
            commit_id: None,
            path: "src/auth.rs".into(),
            start_byte: None,
            end_byte: None,
            symbol_fingerprint: None,
        },
        confidence: 1.0,
    };
    db.put_datapoint(point("alpha")).unwrap();
    let beta = db.put_datapoint(point("beta")).unwrap();
    let beta_id = match beta {
        PutPoint::Inserted(inserted) => inserted.id,
        PutPoint::Unchanged { id } => id,
    };
    let restored = db.put_datapoint(point("alpha")).unwrap();
    match restored {
        PutPoint::Inserted(inserted) => {
            assert_eq!(inserted.version, 3);
            assert_eq!(inserted.previous_id.as_deref(), Some(beta_id.as_str()));
        }
        PutPoint::Unchanged { .. } => panic!("reverted content was treated as unchanged"),
    }
    let points = db.list_datapoints(&dataset.id).unwrap();
    assert_eq!(points.len(), 2);
    let latest = points.iter().max_by_key(|point| point.version).unwrap();
    assert_eq!(latest.content, "alpha");
    assert_eq!(latest.version, 3);
}

#[test]
fn hash_in_filename_is_not_a_chunk_separator() {
    assert_eq!(chunk_owner_path("foo#bar.rs"), "foo#bar.rs");
    assert_eq!(chunk_owner_path("foo#bar.rs#12-40"), "foo#bar.rs");
    assert_eq!(chunk_owner_path("foo.rs#1-2"), "foo.rs");

    let mut db = IndexDatabase::open_in_memory().unwrap();
    let repo_id = RepoId::new("chunks");
    db.update_repo_info(&repo_info(
        std::path::Path::new("/tmp/chunks"),
        repo_id.clone(),
    ))
    .unwrap();
    let dataset = db.ensure_dataset(&repo_id, "default").unwrap();
    let scanned = ScannedFile {
        file: IndexedFile {
            relative_path: "foo#bar.rs".into(),
            file_hash: "abc".into(),
            git_blob_id: None,
            size_bytes: 3,
            language: Language::Rust,
            kind: FileKind::Source,
            is_test: false,
            is_doc: false,
            indexed_at: Utc::now(),
        },
        absolute_path: PathBuf::from("foo#bar.rs"),
        content: Some("fn live() {}".into()),
    };
    db.save_scanned_files(&repo_id, &[scanned]).unwrap();
    let make = |path: &str, kind: &str, content: &str| NewDataPoint {
        repo_id: repo_id.clone(),
        dataset_id: dataset.id.clone(),
        kind: kind.into(),
        path: path.into(),
        content: content.into(),
        provenance: Provenance {
            source_kind: SourceKind::Worktree,
            commit_id: None,
            path: path.into(),
            start_byte: None,
            end_byte: None,
            symbol_fingerprint: None,
        },
        confidence: 1.0,
    };
    db.put_datapoint(make("foo#bar.rs", "file", "fn live() {}"))
        .unwrap();
    db.put_datapoint(make("foo#bar.rs#1-2", "chunk", "live"))
        .unwrap();
    db.put_datapoint(make("foo.rs#1-2", "chunk", "other"))
        .unwrap();
    db.delete_chunks(&dataset.id, "foo").unwrap();
    let after_unrelated = db.list_datapoints(&dataset.id).unwrap();
    assert!(after_unrelated
        .iter()
        .any(|point| point.provenance.path == "foo#bar.rs#1-2"));
    db.delete_chunks(&dataset.id, "foo#bar.rs").unwrap();
    let after_owner = db.list_datapoints(&dataset.id).unwrap();
    assert!(after_owner
        .iter()
        .all(|point| point.provenance.path != "foo#bar.rs#1-2"));
    assert!(after_owner
        .iter()
        .any(|point| point.provenance.path == "foo#bar.rs"));

    let removed = db.purge_catalog_orphans(&dataset.id).unwrap();
    assert!(removed >= 1);
    let surviving = db.list_datapoints(&dataset.id).unwrap();
    assert!(surviving
        .iter()
        .any(|point| point.provenance.path == "foo#bar.rs"));
    assert!(surviving
        .iter()
        .all(|point| point.provenance.path != "foo.rs#1-2"));
}

#[test]
fn root_test_directories_follow_index_tests() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    for directory in ["src", "tests", "test", "spec", "specs", "docs", "src/tests"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("src/contest.rs"), "fn contest() {}\n").unwrap();
    fs::write(root.join("tests/common.rs"), "fn helper() {}\n").unwrap();
    fs::write(root.join("test/common.rs"), "fn helper() {}\n").unwrap();
    fs::write(root.join("spec/common.rs"), "fn helper() {}\n").unwrap();
    fs::write(root.join("specs/common.rs"), "fn helper() {}\n").unwrap();
    fs::write(root.join("src/tests/common.rs"), "fn helper() {}\n").unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();

    let enabled = RepoScanner::new(root, agent_core::RepoConfig::default()).scan();
    let kind_of = |scanned: &[ScannedFile], path: &str| {
        scanned
            .iter()
            .find(|item| item.file.relative_path == path)
            .map(|item| item.file.kind)
    };
    assert_eq!(kind_of(&enabled, "tests/common.rs"), Some(FileKind::Test));
    assert_eq!(kind_of(&enabled, "test/common.rs"), Some(FileKind::Test));
    assert_eq!(kind_of(&enabled, "spec/common.rs"), Some(FileKind::Test));
    assert_eq!(kind_of(&enabled, "specs/common.rs"), Some(FileKind::Test));
    assert_eq!(
        kind_of(&enabled, "src/tests/common.rs"),
        Some(FileKind::Test)
    );
    assert_eq!(kind_of(&enabled, "src/main.rs"), Some(FileKind::Source));
    assert_eq!(kind_of(&enabled, "src/contest.rs"), Some(FileKind::Source));
    assert_eq!(
        kind_of(&enabled, "docs/guide.md"),
        Some(FileKind::Documentation)
    );

    let tests_off = agent_core::RepoConfig {
        index_tests: false,
        ..agent_core::RepoConfig::default()
    };
    let hidden = RepoScanner::new(root, tests_off).scan();
    let hidden_paths: Vec<_> = hidden
        .iter()
        .map(|item| item.file.relative_path.as_str())
        .collect();
    assert!(!hidden_paths.contains(&"tests/common.rs"));
    assert!(!hidden_paths.iter().any(|path| path.ends_with("common.rs")));
    assert!(hidden_paths.contains(&"src/main.rs"));
    assert!(hidden_paths.contains(&"src/contest.rs"));
    assert!(hidden_paths.contains(&"docs/guide.md"));

    let docs_off = agent_core::RepoConfig {
        index_tests: false,
        index_docs: false,
        ..agent_core::RepoConfig::default()
    };
    let neither = RepoScanner::new(root, docs_off).scan();
    let neither_paths: Vec<_> = neither
        .iter()
        .map(|item| item.file.relative_path.as_str())
        .collect();
    assert!(!neither_paths.contains(&"docs/guide.md"));
    assert!(!neither_paths.contains(&"tests/common.rs"));
    assert!(neither_paths.contains(&"src/main.rs"));
}

#[test]
fn cyclic_traversal_returns_each_node_once_quickly() {
    let mut db = IndexDatabase::open_in_memory().unwrap();
    let repo_id = RepoId::new("cycle");
    db.update_repo_info(&repo_info(
        std::path::Path::new("/tmp/cycle"),
        repo_id.clone(),
    ))
    .unwrap();
    let dataset = db.ensure_dataset(&repo_id, "default").unwrap();
    for index in 0..8 {
        let id = format!("n{index}");
        db.upsert_node(repo_id.as_str(), &dataset.id, &id, "symbol", &id, None)
            .unwrap();
    }
    for src in 0..8 {
        for dst in 0..8 {
            if src == dst {
                continue;
            }
            db.upsert_edge(
                repo_id.as_str(),
                &dataset.id,
                &format!("n{src}"),
                &format!("n{dst}"),
                EdgeKind::Calls,
            )
            .unwrap();
        }
    }

    let started = std::time::Instant::now();
    let found = db.traverse(&dataset.id, "n0", 7).unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "traversal took {:?}",
        started.elapsed()
    );
    assert_eq!(found.len(), 8);
    assert_eq!(found.iter().find(|(id, _)| id == "n0").unwrap().1, 0);
    assert!(found.iter().all(|(id, depth)| id == "n0" || *depth == 1));
}
