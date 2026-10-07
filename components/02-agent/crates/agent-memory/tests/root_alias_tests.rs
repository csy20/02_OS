use agent_core::RepoConfig;
use agent_git::GitRepo;
use agent_index::IndexDatabase;
use agent_memory::{index_pipeline, RepoHandle, TaskRegistry};
use std::fs;

#[test]
fn subdirectory_and_symlink_aliases_use_the_same_complete_catalog() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("repo");
    fs::create_dir_all(root.join("src")).unwrap();
    git2::Repository::init(&root).unwrap();
    fs::write(root.join("root.rs"), "fn root_entry() {}\n").unwrap();
    fs::write(root.join("src/lib.rs"), "fn nested_entry() {}\n").unwrap();
    let config = RepoConfig {
        max_file_size_kb: 123,
        ..RepoConfig::default()
    };
    config.save_to_repo(&root).unwrap();
    let database = directory.path().join("index.sqlite");
    let db = IndexDatabase::open(&database).unwrap();
    let mut handle = RepoHandle::open_with_db(&root.join("src"), db).unwrap();
    assert_eq!(handle.root, root.canonicalize().unwrap());
    assert_eq!(handle.config.max_file_size_kb, 123);
    index_pipeline(&mut handle, true, &TaskRegistry::new()).unwrap();
    assert_eq!(
        handle
            .db
            .find_symbols_by_name(&handle.info.id, "root_entry")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        handle
            .db
            .find_symbols_by_name(&handle.info.id, "nested_entry")
            .unwrap()[0]
            .file_path,
        "src/lib.rs"
    );
    #[cfg(unix)]
    {
        let alias = directory.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        let git = GitRepo::open(alias.join("src")).unwrap();
        assert_eq!(git.root_path(), handle.root);
        let db = IndexDatabase::open(&database).unwrap();
        let mut alias_handle = RepoHandle::open_with_db(&alias.join("src"), db).unwrap();
        index_pipeline(&mut alias_handle, false, &TaskRegistry::new()).unwrap();
        assert_eq!(alias_handle.info.id, handle.info.id);
        assert_eq!(
            alias_handle
                .db
                .find_symbols_by_name(&alias_handle.info.id, "root_entry")
                .unwrap()
                .len(),
            1
        );
    }
}
