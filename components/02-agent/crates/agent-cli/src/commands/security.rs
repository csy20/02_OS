use agent_core::{types::SecretPattern, Result};
use agent_git::{GitDiscovery, GitRepo};
use serde_json::json;
use std::env;
use std::fs;
use walkdir::WalkDir;

pub fn execute(json_output: bool) -> Result<()> {
    let current_dir = env::current_dir()?;
    let repo_root = GitDiscovery::find_repository_root(&current_dir)?;
    let git_repo = GitRepo::open(&repo_root)?;
    let tracked_files = git_repo.list_tracked_files().unwrap_or_default();

    let mut secrets_detected = Vec::new();
    let mut tracked_secrets = Vec::new();
    let mut loose_permissions = Vec::new();

    // Check .gitignore existence
    let has_gitignore = repo_root.join(".gitignore").exists();

    for entry in WalkDir::new(&repo_root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Skip .git internal directory
        if path.components().any(|c| c.as_os_str() == ".git") {
            continue;
        }

        let rel_path = match path.strip_prefix(&repo_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if SecretPattern::is_secret(path) {
            secrets_detected.push(rel_path.clone());
            if tracked_files.contains(&rel_path) {
                tracked_secrets.push(rel_path.clone());
            }
        }

        // Check for overly permissive files (e.g. world-writable)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(path) {
                let mode = metadata.permissions().mode();
                if mode & 0o002 != 0 {
                    loose_permissions.push(rel_path);
                }
            }
        }
    }

    let is_secure = tracked_secrets.is_empty() && loose_permissions.is_empty();

    if json_output {
        let out = json!({
            "status": if is_secure { "passed" } else { "warnings_found" },
            "has_gitignore": has_gitignore,
            "detected_sensitive_files": secrets_detected,
            "tracked_secrets_critical": tracked_secrets,
            "world_writable_files": loose_permissions,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("02 Agent Runtime: Security Audit (02agent security)");
        println!("==================================================");
        println!("Repository Root: {}", repo_root.display());
        println!(
            "  • .gitignore file: {}",
            if has_gitignore {
                "Present (Good)"
            } else {
                "MISSING (Warning)"
            }
        );

        if !tracked_secrets.is_empty() {
            println!();
            println!("  [CRITICAL] Tracked Secret Files Found in Git Index:");
            for f in &tracked_secrets {
                println!("    - {}", f);
            }
            println!("    Remedy: Remove with 'git rm --cached <file>' and add to .gitignore immediately.");
        } else {
            println!("  • Tracked Secrets in Git: None (Clean)");
        }

        if !secrets_detected.is_empty() {
            println!();
            println!("  • Untracked Local Sensitive Files Excluded from Index:");
            for f in &secrets_detected {
                println!("    - {} (Excluded by 02 Agent Runtime filter)", f);
            }
        }

        if !loose_permissions.is_empty() {
            println!();
            println!("  [WARN] World-Writable Files:");
            for f in &loose_permissions {
                println!("    - {}", f);
            }
        }

        println!();
        if is_secure {
            println!("Summary: Security audit PASSED. No sensitive data exposed.");
        } else {
            println!("Summary: Security audit completed with WARNINGS. See above.");
        }
        println!("==================================================");
    }

    Ok(())
}
