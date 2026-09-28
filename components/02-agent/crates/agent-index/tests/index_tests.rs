use agent_core::{config::RepoConfig, types::RepoId, RepoInfo};
use agent_index::{IndexDatabase, RepoScanner};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_scanner_and_sqlite_fts5() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create a mock repository structure
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();

    fs::write(
        root.join("src/auth.rs"),
        "pub fn rotate_refresh_token() { /* token rotation logic */ }",
    )
    .unwrap();
    fs::write(
        root.join("tests/auth_test.rs"),
        "#[test] fn test_rotate() { rotate_refresh_token(); }",
    )
    .unwrap();
    fs::write(
        root.join("docs/architecture.md"),
        "# Authentication Architecture\nRefresh tokens are single use.",
    )
    .unwrap();
    fs::write(root.join(".env"), "DATABASE_URL=postgres://secret").unwrap();

    let config = RepoConfig::default();
    let scanner = RepoScanner::new(root, config);
    let scanned = scanner.scan();

    // Verify .env was excluded
    assert!(!scanned.iter().any(|f| f.file.relative_path == ".env"));

    // Verify files were found and classified correctly
    let auth_file = scanned
        .iter()
        .find(|f| f.file.relative_path == "src/auth.rs")
        .expect("src/auth.rs should be found");
    assert_eq!(auth_file.file.kind, agent_core::types::FileKind::Source);
    assert_eq!(auth_file.file.language, agent_core::types::Language::Rust);

    let test_file = scanned
        .iter()
        .find(|f| f.file.relative_path == "tests/auth_test.rs")
        .expect("tests/auth_test.rs should be found");
    assert!(test_file.file.is_test);

    // Save to database
    let mut db = IndexDatabase::open_in_memory().unwrap();
    let repo_id = RepoId::from_path(root);
    let repo_info = RepoInfo {
        id: repo_id.clone(),
        name: "test_repo".into(),
        root_path: root.to_path_buf(),
        head_commit: Some("c0ffee".into()),
        branch: Some("main".into()),
        is_clean: true,
        modified_count: 0,
        untracked_count: 0,
    };
    db.update_repo_info(&repo_info).unwrap();
    let saved_count = db.save_scanned_files(&repo_id, &scanned).unwrap();
    assert_eq!(saved_count, 3);

    // Test stats
    let stats = db.get_stats(&repo_id).unwrap();
    assert_eq!(stats.total_files, 3);
    assert_eq!(stats.source_files, 1);
    assert_eq!(stats.test_files, 1);
    assert_eq!(stats.doc_files, 1);

    // Test FTS5 search
    let results = db.search_content("refresh token", 10).unwrap();
    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.relative_path.contains("auth")));

    let listed = db.list_files(&repo_id).unwrap();
    let listed_auth = listed
        .iter()
        .find(|f| f.relative_path == "src/auth.rs")
        .unwrap();
    assert_eq!(
        listed_auth.language,
        agent_core::types::Language::Rust,
        "language names stored in sqlite must round-trip"
    );
}

#[test]
fn test_find_tests_ignores_unrelated_calls_inside_tests() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(
        root.join("src/parse.rs"),
        "pub fn parse_header() -> bool { true }\n",
    )
    .unwrap();
    fs::write(
        root.join("tests/parse_test.rs"),
        "#[test]\nfn test_parse_header() {\n    assert!(parse_header());\n    let _ = String::new();\n}\n",
    )
    .unwrap();

    let scanner = RepoScanner::new(root, RepoConfig::default());
    let scanned = scanner.scan();
    let mut db = IndexDatabase::open_in_memory().unwrap();
    let repo_id = RepoId::from_path(root);
    db.update_repo_info(&RepoInfo {
        id: repo_id.clone(),
        name: "parse_repo".into(),
        root_path: root.to_path_buf(),
        head_commit: None,
        branch: None,
        is_clean: true,
        modified_count: 0,
        untracked_count: 0,
    })
    .unwrap();
    db.save_scanned_files(&repo_id, &scanned).unwrap();

    let mut symbols = Vec::new();
    let mut references = Vec::new();
    for item in &scanned {
        if let Some(text) = &item.content {
            if let Ok(extracted) = agent_parser::CodeExtractor::extract(
                &item.file.relative_path,
                text,
                item.file.language,
            ) {
                symbols.extend(extracted.symbols);
                references.extend(extracted.references);
            }
        }
    }
    db.save_symbols_and_references(&repo_id, &symbols, &references)
        .unwrap();

    let in_file = db.find_symbols_by_file(&repo_id, "src/parse.rs").unwrap();
    assert!(in_file.iter().any(|s| s.name == "parse_header"));
    assert!(db.find_symbols_by_name(&repo_id, "").unwrap().is_empty());

    let tests = db.find_tests_for_symbol(&repo_id, "parse_header").unwrap();
    assert!(tests.iter().any(|t| t.source_file == "tests/parse_test.rs"));
    assert!(
        tests.iter().all(|t| {
            t.target_name != "assert" && t.target_name != "new" && t.target_name != "String::new"
        }),
        "unexpected tests: {:?}",
        tests
            .iter()
            .map(|t| (
                t.kind,
                t.source_symbol_name.clone(),
                t.target_name.clone(),
                t.line_number
            ))
            .collect::<Vec<_>>()
    );

    let broad = db.find_tests_for_symbol(&repo_id, "parse").unwrap();
    assert!(broad
        .iter()
        .any(|t| t.source_symbol_name.as_deref() == Some("test_parse_header")));
    assert!(broad
        .iter()
        .all(|t| t.target_name != "assert" && t.target_name != "new"));
}
