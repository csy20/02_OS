use agent_core::{
    config::RepoConfig,
    types::{FileKind, IndexedFile, Language, SecretPattern},
};
use chrono::Utc;
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub struct RepoScanner {
    root: PathBuf,
    config: RepoConfig,
}

#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub file: IndexedFile,
    pub absolute_path: PathBuf,
    pub content: Option<String>,
}

impl RepoScanner {
    pub fn new<P: AsRef<Path>>(root: P, config: RepoConfig) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            config,
        }
    }

    /// Walk repository and gather indexable files respecting gitignore, 02agentignore, and security rules.
    pub fn scan(&self) -> Vec<ScannedFile> {
        let mut builder = WalkBuilder::new(&self.root);
        builder.hidden(false); // allow inspecting dotfiles like .gitignore
        builder.git_ignore(true);
        builder.git_global(true);
        builder.git_exclude(true);

        // Add custom .02agentignore if it exists
        let agent_ignore = self.root.join(".02agentignore");
        if agent_ignore.exists() {
            builder.add_custom_ignore_filename(".02agentignore");
        }

        let max_bytes = self.config.max_file_size_kb * 1024;
        let mut scanned = Vec::new();

        for result in builder.build() {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Exclude anything inside .git
            if path.components().any(|c| c.as_os_str() == ".git") {
                continue;
            }

            // Exclude secrets
            if self.config.exclude_secrets && SecretPattern::is_secret(path) {
                continue;
            }

            // Relative path from repo root
            let rel_path = match path.strip_prefix(&self.root) {
                Ok(p) => match p.to_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                },
                Err(_) => continue,
            };

            // Check custom ignore list
            if self.config.custom_ignores.iter().any(|pattern| {
                let trimmed = pattern.trim_end_matches('/');
                rel_path.starts_with(pattern)
                    || rel_path.starts_with(trimmed)
                    || rel_path.contains(&format!("/{}/", trimmed))
            }) {
                continue;
            }

            let metadata = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = metadata.len();
            if size > max_bytes {
                continue;
            }

            // Read content and compute hash
            let (hash, content) = match fs::read(path) {
                Ok(bytes) => {
                    let mut hasher = Sha256::new();
                    hasher.update(&bytes);
                    let h = hex::encode(hasher.finalize());
                    // If UTF-8, store text content for FTS5 indexing
                    let text = std::str::from_utf8(&bytes).ok().map(|s| s.to_string());
                    (h, text)
                }
                Err(_) => continue,
            };

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let language = Language::from_extension(ext);
            let kind = Self::classify_kind(&rel_path, language);
            let is_test = kind == FileKind::Test;
            let is_doc = kind == FileKind::Documentation;

            let file = IndexedFile {
                relative_path: rel_path,
                file_hash: hash,
                git_blob_id: None, // Filled via Git if available
                size_bytes: size,
                language,
                kind,
                is_test,
                is_doc,
                indexed_at: Utc::now(),
            };

            scanned.push(ScannedFile {
                file,
                absolute_path: path.to_path_buf(),
                content,
            });
        }

        scanned
    }

    fn classify_kind(rel_path: &str, language: Language) -> FileKind {
        let lower = rel_path.to_lowercase();
        let file_name = Path::new(rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Tests
        if lower.contains("/tests/")
            || lower.contains("/test/")
            || lower.contains("/spec/")
            || lower.contains("/specs/")
            || file_name.starts_with("test_")
            || file_name.ends_with("_test.rs")
            || file_name.ends_with("_test.py")
            || file_name.ends_with("_test.go")
            || file_name.ends_with(".test.ts")
            || file_name.ends_with(".test.js")
            || file_name.ends_with(".spec.ts")
            || file_name.ends_with(".spec.js")
        {
            return FileKind::Test;
        }

        // Documentation
        if lower.starts_with("docs/")
            || lower.contains("/docs/")
            || file_name.ends_with(".md")
            || file_name.ends_with(".markdown")
            || file_name.ends_with(".rst")
            || file_name.ends_with(".adoc")
            || file_name == "readme"
            || file_name.starts_with("readme.")
            || file_name == "license"
            || file_name.starts_with("license.")
        {
            return FileKind::Documentation;
        }

        // Package manifests
        let manifests = [
            "cargo.toml",
            "package.json",
            "requirements.txt",
            "pyproject.toml",
            "go.mod",
            "pom.xml",
            "cmakelists.txt",
            "packages.x86_64",
            "pubspec.yaml",
            "gemfile",
        ];
        if manifests.contains(&file_name.as_str()) {
            return FileKind::Manifest;
        }

        // Configuration
        if file_name.ends_with(".conf")
            || file_name.ends_with(".ini")
            || file_name.ends_with(".desktop")
            || file_name.ends_with(".service")
            || file_name.ends_with(".socket")
            || file_name == ".editorconfig"
            || file_name == ".gitignore"
            || (matches!(language, Language::Toml | Language::Yaml | Language::Json)
                && !manifests.contains(&file_name.as_str()))
        {
            return FileKind::Configuration;
        }

        // Source
        if matches!(
            language,
            Language::Rust
                | Language::Python
                | Language::TypeScript
                | Language::JavaScript
                | Language::C
                | Language::Cpp
                | Language::Bash
                | Language::Dart
                | Language::Go
                | Language::Sql
        ) {
            return FileKind::Source;
        }

        FileKind::Unknown
    }
}
