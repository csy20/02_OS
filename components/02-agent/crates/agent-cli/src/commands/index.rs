use agent_core::{config::RepoConfig, paths::StoragePaths, Result};
use agent_git::{GitDiscovery, GitRepo};
use agent_index::{IndexDatabase, RepoScanner};
use serde_json::json;
use std::env;
use std::time::Instant;

pub fn execute(full: bool, json_output: bool) -> Result<()> {
    let start = Instant::now();
    let current_dir = env::current_dir()?;
    let repo_root = GitDiscovery::find_repository_root(&current_dir)?;
    let git_repo = GitRepo::open(&repo_root)?;
    let repo_info = git_repo.info()?;

    let db_path = StoragePaths::repo_db_path(&repo_info.id)?;
    if !db_path.exists() {
        println!("Repository not initialized yet. Initializing...");
        crate::commands::init::execute(false)?;
    }

    let mut db = IndexDatabase::open(&db_path)?;
    db.update_repo_info(&repo_info)?;

    let config = RepoConfig::load_or_default(&repo_root);
    let scanner = RepoScanner::new(&repo_root, config);

    let mut scanned = scanner.scan();

    // Populate Git blob IDs where available
    for item in &mut scanned {
        if let Ok(Some(blob_id)) = git_repo.get_blob_id(&item.file.relative_path) {
            item.file.git_blob_id = Some(blob_id);
        }
    }

    let files_updated = if full {
        let count = db.save_scanned_files(&repo_info.id, &scanned)?;

        // Extract symbols and references for source & test files
        let mut all_symbols = Vec::new();
        let mut all_references = Vec::new();

        for item in &scanned {
            if item.file.kind == agent_core::types::FileKind::Source || item.file.is_test {
                if let Some(ref text) = item.content {
                    if let Ok(extracted) = agent_parser::CodeExtractor::extract(
                        &item.file.relative_path,
                        text,
                        item.file.language,
                    ) {
                        all_symbols.extend(extracted.symbols);
                        all_references.extend(extracted.references);
                    }
                }
            }
        }

        db.save_symbols_and_references(&repo_info.id, &all_symbols, &all_references)?;
        count
    } else {
        // Incremental indexing: compare scanned files with database
        let existing_files = db.list_files(&repo_info.id)?;
        let mut existing_map = std::collections::HashMap::new();
        for f in existing_files {
            existing_map.insert(f.relative_path, f.file_hash);
        }

        let mut current_paths = std::collections::HashSet::new();
        let mut files_to_update = Vec::new();

        for item in scanned {
            current_paths.insert(item.file.relative_path.clone());
            let needs_update = match existing_map.get(&item.file.relative_path) {
                Some(old_hash) => old_hash != &item.file.file_hash,
                None => true,
            };

            if needs_update {
                files_to_update.push(item);
            }
        }

        let mut files_to_delete = Vec::new();
        for old_path in existing_map.keys() {
            if !current_paths.contains(old_path) {
                files_to_delete.push(old_path.clone());
            }
        }

        let mut new_symbols = Vec::new();
        let mut new_references = Vec::new();

        for item in &files_to_update {
            if item.file.kind == agent_core::types::FileKind::Source || item.file.is_test {
                if let Some(ref text) = item.content {
                    if let Ok(extracted) = agent_parser::CodeExtractor::extract(
                        &item.file.relative_path,
                        text,
                        item.file.language,
                    ) {
                        new_symbols.extend(extracted.symbols);
                        new_references.extend(extracted.references);
                    }
                }
            }
        }

        let (updated, _, _) = db.update_incremental(
            &repo_info.id,
            &files_to_update,
            &files_to_delete,
            &new_symbols,
            &new_references,
            repo_info.head_commit.as_deref(),
        )?;

        updated
    };

    // Evaluate staleness and update memories
    let store = agent_memory::MemoryStore::for_repo(&repo_info.id)?;
    let staleness_report =
        agent_memory::StalenessEngine::evaluate(&repo_root, &repo_info.id, &git_repo, &db, &store)
            .ok();

    let elapsed = start.elapsed();
    let stats = db.get_stats(&repo_info.id)?;

    if json_output {
        let out = json!({
            "status": "success",
            "mode": if full { "full" } else { "incremental" },
            "files_updated": files_updated,
            "total_files": stats.total_files,
            "source_files": stats.source_files,
            "test_files": stats.test_files,
            "doc_files": stats.doc_files,
            "total_symbols": stats.total_symbols,
            "total_references": stats.total_references,
            "duration_ms": elapsed.as_millis(),
            "head_commit": repo_info.head_commit,
            "staleness": staleness_report,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!(
            "02 Agent Runtime: Indexing Complete ({})",
            if full { "full" } else { "incremental" }
        );
        println!("  Files Updated:   {}", files_updated);
        println!("  Total Indexed:   {}", stats.total_files);
        println!("  Source Files:    {}", stats.source_files);
        println!("  Test Files:      {}", stats.test_files);
        println!("  Docs / Markdown: {}", stats.doc_files);
        println!("  Manifests:       {}", stats.manifest_files);
        println!("  Symbols Parsed:  {}", stats.total_symbols);
        println!("  References:      {}", stats.total_references);
        if let Some(ref report) = staleness_report {
            println!(
                "  Memory Status:   {} fresh, {} degraded, {} stale",
                report.fresh_count, report.degraded_count, report.stale_count
            );
            if !report.changed_symbols.is_empty() {
                println!("  Changed Symbols: {}", report.changed_symbols.join(", "));
            }
        }
        println!("  Time Taken:      {:.2?}", elapsed);
        println!(
            "  Target Commit:   {}",
            repo_info.head_commit.as_deref().unwrap_or("none")
        );
    }

    Ok(())
}
