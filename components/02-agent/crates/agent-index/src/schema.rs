pub const CURRENT_SCHEMA_VERSION: i32 = 1;

pub const INIT_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS repositories (
    repo_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    head_commit TEXT,
    branch TEXT,
    indexed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    file_hash TEXT NOT NULL,
    git_blob_id TEXT,
    size_bytes INTEGER NOT NULL,
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    is_test INTEGER NOT NULL DEFAULT 0,
    is_doc INTEGER NOT NULL DEFAULT 0,
    indexed_at TEXT NOT NULL,
    FOREIGN KEY(repo_id) REFERENCES repositories(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_files_repo ON files (repo_id);
CREATE INDEX IF NOT EXISTS idx_files_path ON files (relative_path);
CREATE INDEX IF NOT EXISTS idx_files_kind ON files (kind);
CREATE INDEX IF NOT EXISTS idx_files_language ON files (language);

-- Full-text search index for files and source content
CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
    relative_path,
    content,
    tokenize = 'porter unicode61'
);

CREATE TABLE IF NOT EXISTS symbols (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    name TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    file_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    signature TEXT,
    doc_comment TEXT,
    fingerprint TEXT NOT NULL,
    FOREIGN KEY(repo_id) REFERENCES repositories(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols (name);
CREATE INDEX IF NOT EXISTS idx_symbols_repo ON symbols (repo_id);
CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols (file_path);

CREATE TABLE IF NOT EXISTS symbol_references (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    source_file TEXT NOT NULL,
    source_symbol_name TEXT,
    target_name TEXT NOT NULL,
    target_symbol_id TEXT,
    kind TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    FOREIGN KEY(repo_id) REFERENCES repositories(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refs_target ON symbol_references (target_name);
CREATE INDEX IF NOT EXISTS idx_refs_source ON symbol_references (source_file);
CREATE INDEX IF NOT EXISTS idx_refs_kind ON symbol_references (kind);
CREATE INDEX IF NOT EXISTS idx_refs_repo ON symbol_references (repo_id);
"#;
