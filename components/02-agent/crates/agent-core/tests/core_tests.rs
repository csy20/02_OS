use agent_core::{Language, RepoConfig, RepoId, SecretPattern};
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_repo_id_deterministic() {
    let id1 = RepoId::from_path("/home/user/project");
    let id2 = RepoId::from_path("/home/user/project");
    assert_eq!(id1, id2);
    assert_eq!(id1.as_str().len(), 16);
}

#[test]
fn test_language_detection() {
    assert_eq!(Language::from_extension("rs"), Language::Rust);
    assert_eq!(Language::from_extension("py"), Language::Python);
    assert_eq!(Language::from_extension("ts"), Language::TypeScript);
    assert_eq!(Language::from_extension("cpp"), Language::Cpp);
    assert_eq!(Language::from_extension("dart"), Language::Dart);
    assert_eq!(Language::from_extension("sh"), Language::Bash);
    assert_eq!(Language::from_extension("unknownext"), Language::Unknown);
    assert_eq!(Language::from_name("rust"), Language::Rust);
    assert_eq!(Language::from_name("python"), Language::Python);
    assert_eq!(Language::from_name("typescript"), Language::TypeScript);
    assert_eq!(Language::from_name("javascript"), Language::JavaScript);
    assert_eq!(Language::from_name("bash"), Language::Bash);
    assert_eq!(Language::from_name("cpp"), Language::Cpp);
    assert_eq!(Language::from_name("rs"), Language::Rust);
    assert_eq!(Language::from_name("not-a-language"), Language::Unknown);
}

#[test]
fn test_secret_detection() {
    assert!(SecretPattern::is_secret(Path::new(".env")));
    assert!(SecretPattern::is_secret(Path::new(".env.local")));
    assert!(SecretPattern::is_secret(Path::new("id_rsa")));
    assert!(SecretPattern::is_secret(Path::new("server.key")));
    assert!(SecretPattern::is_secret(Path::new("certificate.pem")));
    assert!(SecretPattern::is_secret(Path::new("credentials.json")));
    assert!(!SecretPattern::is_secret(Path::new("main.rs")));
    assert!(!SecretPattern::is_secret(Path::new("README.md")));
    assert!(!SecretPattern::is_secret(Path::new("secret_guide.md")));
}

#[test]
fn test_config_save_and_load() {
    let dir = tempdir().unwrap();
    let config = RepoConfig::default();
    config.save_to_repo(dir.path()).unwrap();

    let loaded = RepoConfig::load_or_default(dir.path());
    assert_eq!(loaded.max_file_size_kb, config.max_file_size_kb);
    assert!(loaded.exclude_secrets);
}
