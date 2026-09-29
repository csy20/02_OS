use crate::db::IndexDatabase;
use crate::schema::{CURRENT_SCHEMA_VERSION, SCHEMA_V1_SQL};
use rusqlite::Connection;
use std::fs;

#[test]
fn v1_database_upgrades_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("index.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(SCHEMA_V1_SQL).unwrap();
    conn.execute("INSERT INTO schema_version (version) VALUES (1)", [])
        .unwrap();
    conn.execute(
        "INSERT INTO repositories (repo_id, name, root_path, head_commit, branch, indexed_at)
         VALUES ('repo', 'demo', '/tmp/demo', NULL, 'main', '2020-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO files (
            repo_id, relative_path, file_hash, git_blob_id, size_bytes, language, kind,
            is_test, is_doc, indexed_at
         ) VALUES ('repo', 'src/main.rs', 'abc', NULL, 4, 'rust', 'source', 0, 0, '2020-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    drop(conn);

    let db = IndexDatabase::open(&db_path).unwrap();
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    let files = db.list_files(&agent_core::RepoId::new("repo")).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "src/main.rs");
    assert_eq!(files[0].language, agent_core::Language::Rust);

    let marker: i64 = {
        let conn = Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'graph_nodes'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(marker, 1);
    let _ = fs::metadata(&db_path).unwrap();
}

#[test]
fn architecture_doc_matches_schema() {
    let doc = include_str!("../../../../../docs/agent-runtime-architecture.md");
    assert!(doc.contains("relative_path"));
    assert!(doc.contains("graph_nodes"));
}
