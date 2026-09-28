use agent_core::Result;
use agent_core::{EdgeKind, SourceKind};
use agent_index::{file_node_id, symbol_node_id, GraphExportFormat, IndexDatabase};
use agent_memory::{
    add_pipeline, cognify_pipeline, export_graph, AddRequest, CognifyReport, RepoHandle, Task,
    TaskContext, TaskRegistry,
};
use git2::Repository;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

fn git_repo(root: &std::path::Path) {
    let repo = Repository::init(root).unwrap();
    let sig = git2::Signature::now("Tester", "test@02os.org").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub fn target() {}\npub fn caller() { target(); }\n",
    )
    .unwrap();
    fs::write(
        root.join("docs/note.md"),
        "# Note\nA documented decision.\n",
    )
    .unwrap();
    fs::write(root.join(".env"), "TOKEN=secret\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("src/lib.rs")).unwrap();
    index
        .add_path(std::path::Path::new("docs/note.md"))
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();
}

struct CountingTask {
    hits: &'static AtomicUsize,
}

impl Task for CountingTask {
    fn name(&self) -> &'static str {
        "counting"
    }

    fn run(&self, _ctx: &mut TaskContext) -> Result<()> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn add_cognify_export_and_supersede() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git_repo(root);
    let db = IndexDatabase::open(root.join("index.sqlite")).unwrap();
    let mut handle = RepoHandle::open_with_db(root, db).unwrap();

    static HITS: AtomicUsize = AtomicUsize::new(0);
    let mut extra = TaskRegistry::new();
    extra.register(CountingTask { hits: &HITS });

    let first = add_pipeline(
        &mut handle,
        &AddRequest {
            sources: vec![
                SourceKind::Worktree,
                SourceKind::Document,
                SourceKind::GitHistory,
            ],
            dataset_name: None,
            text: None,
            full: false,
        },
        &extra,
    )
    .unwrap();
    assert!(first.files_updated >= 2);
    assert!(first.datapoints_inserted >= 2);
    assert_eq!(HITS.load(Ordering::SeqCst), 1);

    let again = add_pipeline(
        &mut handle,
        &AddRequest {
            sources: vec![SourceKind::Worktree, SourceKind::Document],
            dataset_name: None,
            text: None,
            full: false,
        },
        &TaskRegistry::new(),
    )
    .unwrap();
    assert_eq!(again.datapoints_inserted, 0);
    assert!(again.datapoints_unchanged >= 2);

    let report: CognifyReport =
        cognify_pipeline(&mut handle, false, None, &TaskRegistry::new()).unwrap();
    assert!(report.chunks >= 1, "chunks {}", report.chunks);
    assert!(report.edges >= 1, "edges {}", report.edges);
    let symbols = handle.db.list_symbols(handle.info.id.as_str()).unwrap();
    assert!(symbols.len() >= 2, "symbols {}", symbols.len());

    let points = handle.db.list_datapoints(&first.dataset_id).unwrap();
    assert!(points.iter().all(|point| point.provenance.path != ".env"));
    assert!(points.iter().any(|point| point.kind == "commit"));

    let edges = handle.db.list_edges(&first.dataset_id).unwrap();
    assert!(edges
        .iter()
        .any(|edge| edge.kind == EdgeKind::Calls || edge.kind == EdgeKind::Owns));
    assert!(edges
        .iter()
        .any(|edge| edge.kind == EdgeKind::TouchedInCommit));

    let caller = symbols
        .iter()
        .find(|symbol| symbol.name == "caller")
        .unwrap();
    let start = symbol_node_id(&first.dataset_id, &caller.id);
    let reached = handle.db.traverse(&first.dataset_id, &start, 2).unwrap();
    assert!(reached
        .iter()
        .any(|(id, depth)| id != &start && *depth >= 1));

    let file = file_node_id(&first.dataset_id, "src/lib.rs");
    let from_file = handle.db.traverse(&first.dataset_id, &file, 2).unwrap();
    assert!(from_file.len() > 1);

    for format in ["json", "dot", "mermaid"] {
        let document =
            export_graph(&handle, None, GraphExportFormat::parse(format).unwrap()).unwrap();
        assert!(document.contains("caller"), "{format}: {document}");
    }

    let pasted = add_pipeline(
        &mut handle,
        &AddRequest {
            sources: vec![SourceKind::PastedText],
            dataset_name: None,
            text: Some("first note".into()),
            full: false,
        },
        &TaskRegistry::new(),
    )
    .unwrap();
    assert_eq!(pasted.datapoints_inserted, 1);
    add_pipeline(
        &mut handle,
        &AddRequest {
            sources: vec![SourceKind::PastedText],
            dataset_name: None,
            text: Some("second note".into()),
            full: false,
        },
        &TaskRegistry::new(),
    )
    .unwrap();
    let edges = handle.db.list_edges(&first.dataset_id).unwrap();
    assert!(edges.iter().any(|edge| edge.kind == EdgeKind::Supersedes));
}

#[test]
fn ontology_can_reject_call_edges() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git_repo(root);
    fs::create_dir_all(root.join(".02agent")).unwrap();
    fs::write(
        root.join(".02agent/ontology.toml"),
        "edge_kinds = [\"owns\"]\n",
    )
    .unwrap();
    let db = IndexDatabase::open(root.join("index.sqlite")).unwrap();
    let mut handle = RepoHandle::open_with_db(root, db).unwrap();
    add_pipeline(
        &mut handle,
        &AddRequest {
            sources: vec![SourceKind::Worktree],
            dataset_name: None,
            text: None,
            full: true,
        },
        &TaskRegistry::new(),
    )
    .unwrap();
    let report = cognify_pipeline(&mut handle, true, None, &TaskRegistry::new()).unwrap();
    let edges = handle.db.list_edges(&report.dataset_id).unwrap();
    assert!(edges.iter().all(|edge| edge.kind != EdgeKind::Calls));
    assert!(edges.iter().any(|edge| edge.kind == EdgeKind::Owns));
}

#[test]
fn traversal_refuses_excessive_depth() {
    let dir = tempfile::tempdir().unwrap();
    git_repo(dir.path());
    let db = IndexDatabase::open(dir.path().join("index.sqlite")).unwrap();
    let handle = RepoHandle::open_with_db(dir.path(), db).unwrap();
    let error = handle
        .db
        .traverse("dataset:missing:default", "start", 9)
        .unwrap_err();
    assert!(error.to_string().contains("depth"));
}
