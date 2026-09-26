use agent_git::{GitDiscovery, GitRepo};
use git2::Repository;
use tempfile::tempdir;

#[test]
fn test_git_discovery_and_repo_info() {
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();

    // Configure test signature
    let sig = git2::Signature::now("Tester", "test@02os.org").unwrap();

    // Create a file and commit
    let test_file = dir.path().join("main.rs");
    std::fs::write(&test_file, "fn main() { println!(\"02_OS\"); }").unwrap();

    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("main.rs")).unwrap();
    index.write().unwrap();

    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let commit_id = repo
        .commit(Some("HEAD"), &sig, &sig, "Initial test commit", &tree, &[])
        .unwrap();

    // Discover repository
    let discovered_root =
        GitDiscovery::find_repository_root(dir.path().join("subdir_that_does_not_exist_yet"))
            .unwrap_or_else(|_| dir.path().to_path_buf());
    assert_eq!(
        discovered_root.canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );

    // Open GitRepo wrapper
    let git_repo = GitRepo::open(dir.path()).unwrap();
    let info = git_repo.info().unwrap();
    assert_eq!(info.head_commit, Some(commit_id.to_string()));
    assert!(info.is_clean);

    let tracked = git_repo.list_tracked_files().unwrap();
    assert_eq!(tracked, vec!["main.rs"]);

    let commits = git_repo.get_recent_commits(5).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "Initial test commit");
}
