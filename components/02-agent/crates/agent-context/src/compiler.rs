use crate::{
    budget::{estimate_tokens, BudgetManager},
    ranking::TaskKeywords,
    types::{
        BudgetReport, DependencyNode, EvidencePackage, GitContextSummary, RecentCommit, ScoredFile,
        ScoredSymbol, ScoredTest,
    },
};
use agent_core::{paths::StoragePaths, AgentError, Result};
use agent_git::GitRepo;
use agent_index::IndexDatabase;
use agent_memory::{MemoryStore, StalenessEngine};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub struct ContextCompiler;

impl ContextCompiler {
    /// Compile a task-focused, token-budgeted evidence package.
    pub fn compile<P: AsRef<Path>>(
        repo_root: P,
        task: &str,
        token_budget: usize,
    ) -> Result<EvidencePackage> {
        let root = repo_root.as_ref();
        let git_repo = GitRepo::open(root)?;
        let repo_info = git_repo.info()?;
        let repo_id = &repo_info.id;

        let db_path = StoragePaths::repo_db_path(repo_id)?;
        if !db_path.exists() {
            return Err(AgentError::General(
                "Repository is not indexed yet. Run '02 init' or '02 index' first.".to_string(),
            ));
        }

        let db = IndexDatabase::open(&db_path)?;
        let store = MemoryStore::for_repo(repo_id)?;

        // 1. Git staleness & diff detection
        let diff_files = git_repo.get_diff_files().unwrap_or_default();
        let _staleness_report =
            StalenessEngine::evaluate(root, repo_id, &git_repo, &db, &store).ok();

        let head_commit_str = repo_info
            .head_commit
            .clone()
            .unwrap_or_else(|| "none".to_string());
        let short_head = if head_commit_str.len() >= 7 {
            &head_commit_str[..7]
        } else {
            &head_commit_str
        };

        let staleness_str = if diff_files.is_empty() {
            format!("fresh against HEAD ({})", short_head)
        } else {
            format!(
                "working tree dirty against HEAD ({}) [{} modified files]",
                short_head,
                diff_files.len()
            )
        };

        // 2. Parse task keywords
        let keywords = TaskKeywords::parse(task);

        // 3. Multi-signal Candidate Collection & Scoring
        let mut file_scores: HashMap<String, (f64, Vec<String>)> = HashMap::new();
        let mut candidate_symbols: Vec<ScoredSymbol> = Vec::new();
        let mut candidate_deps: Vec<DependencyNode> = Vec::new();
        let mut candidate_tests: Vec<ScoredTest> = Vec::new();
        let mut seen_symbols: HashSet<String> = HashSet::new();

        // Signal A: Working tree diff
        let diff_set: HashSet<&str> = diff_files.iter().map(|s| s.as_str()).collect();
        for df in &diff_files {
            let entry = file_scores.entry(df.clone()).or_insert((0.0, Vec::new()));
            entry.0 += 8.0;
            entry.1.push("modified in working tree".to_string());
        }

        // Signal B: Exact & partial symbol match
        for term in &keywords.terms {
            let mut matched_syms = Vec::new();
            if let Ok(syms) = db.find_symbols_by_name(repo_id, term) {
                for s in syms {
                    matched_syms.push((s, 10.0, "exact symbol match in task".to_string()));
                }
            }
            if let Ok(syms) = db.search_symbols(repo_id, term, 25) {
                for s in syms {
                    matched_syms.push((s, 7.0, format!("partial symbol match with '{}'", term)));
                }
            }

            for (sym, base_score, reason) in matched_syms {
                let mut sym_score = base_score;
                let mut reasons = vec![reason];

                if diff_set.contains(sym.file_path.as_str()) {
                    sym_score += 8.0;
                    reasons.push("symbol in dirty working-tree file".to_string());
                }

                // Boost file containing this symbol
                let entry = file_scores
                    .entry(sym.file_path.clone())
                    .or_insert((0.0, Vec::new()));
                entry.0 += sym_score;
                entry.1.push(format!("defines symbol '{}'", sym.name));

                if seen_symbols.insert(format!("{}:{}", sym.file_path, sym.name)) {
                    candidate_symbols.push(ScoredSymbol {
                        symbol: sym.clone(),
                        score: sym_score,
                        reasons,
                    });
                }

                // Signal C: Dependencies & callers
                if let Ok(deps) = db.find_symbol_deps(repo_id, &sym.name) {
                    for r in deps.callers {
                        let caller_file = r.source_file.clone();
                        let caller_name = r
                            .source_symbol_name
                            .clone()
                            .unwrap_or_else(|| "<top-level>".to_string());

                        let entry = file_scores
                            .entry(caller_file.clone())
                            .or_insert((0.0, Vec::new()));
                        entry.0 += 5.0;
                        entry
                            .1
                            .push(format!("calls '{}' from '{}'", sym.name, caller_name));

                        candidate_deps.push(DependencyNode {
                            caller: caller_name,
                            callee: sym.name.clone(),
                            relationship: r.kind.to_string(),
                            file_path: caller_file,
                            line: r.line_number,
                        });
                    }

                    for r in deps.callees {
                        let entry = file_scores
                            .entry(r.source_file.clone())
                            .or_insert((0.0, Vec::new()));
                        entry.0 += 4.0;
                        entry.1.push(format!("called by '{}'", sym.name));

                        candidate_deps.push(DependencyNode {
                            caller: sym.name.clone(),
                            callee: r.target_name,
                            relationship: r.kind.to_string(),
                            file_path: r.source_file,
                            line: r.line_number,
                        });
                    }
                }

                // Signal D: Relevant tests
                if let Ok(tests) = db.find_tests_for_symbol(repo_id, &sym.name) {
                    for t in tests {
                        let test_file = t.source_file.clone();
                        let entry = file_scores
                            .entry(test_file.clone())
                            .or_insert((0.0, Vec::new()));
                        entry.0 += 6.0;
                        entry.1.push(format!("tests symbol '{}'", sym.name));

                        candidate_tests.push(ScoredTest {
                            test_file,
                            test_name: t.source_symbol_name,
                            target_symbol: sym.name.clone(),
                            score: 6.0,
                        });
                    }
                }
            }
        }

        // Signal E: BM25 Full-text search
        if let Ok(search_hits) = db.search_content(task, 15) {
            for (idx, hit) in search_hits.into_iter().enumerate() {
                let bm25_boost = 6.0 - (idx as f64 * 0.3).min(4.0);
                let entry = file_scores
                    .entry(hit.relative_path.clone())
                    .or_insert((0.0, Vec::new()));
                entry.0 += bm25_boost;
                entry
                    .1
                    .push(format!("BM25 search match (rank #{})", idx + 1));
            }
        }

        // Signal F: Prior verified memories
        let memories = store.load_all().unwrap_or_default();
        let mut relevant_memories = Vec::new();
        for mem in memories {
            let mut matched = false;
            let claim_lower = mem.claim.to_lowercase();
            for term in &keywords.terms {
                if claim_lower.contains(term) {
                    matched = true;
                    break;
                }
            }
            if !matched {
                for ev in &mem.evidence {
                    if file_scores.contains_key(&ev.file)
                        || ev
                            .symbols
                            .iter()
                            .any(|s| keywords.terms.contains(&s.to_lowercase()))
                    {
                        matched = true;
                        break;
                    }
                }
            }
            if matched {
                // Boost files mentioned in evidence
                for ev in &mem.evidence {
                    let entry = file_scores
                        .entry(ev.file.clone())
                        .or_insert((0.0, Vec::new()));
                    entry.0 += 4.0;
                    entry.1.push(format!("supported by memory '{}'", mem.id));
                }
                relevant_memories.push(mem);
            }
        }

        // Signal G: Recent commits touching relevant files
        let all_recent = git_repo.get_recent_commits(15).unwrap_or_default();
        let mut relevant_commits = Vec::new();
        for c in all_recent {
            relevant_commits.push(RecentCommit {
                commit_id: c.id,
                author: c.author,
                summary: c.summary,
                timestamp: c.timestamp,
            });
        }

        // Sort files by score descending
        let mut sorted_files: Vec<(String, f64, Vec<String>)> = file_scores
            .into_iter()
            .map(|(path, (score, reasons))| (path, score, reasons))
            .collect();
        sorted_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Sort symbols by score descending
        candidate_symbols.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 4. Token Budget Allocation & Snippet Generation
        let mut budget_manager = BudgetManager::new(token_budget);

        // Estimate header, git context, memory tokens
        let mut base_metadata = format!("# {}\n{}\n{}", task, staleness_str, head_commit_str);
        for m in &relevant_memories {
            base_metadata.push_str(&m.claim);
        }
        for d in &candidate_deps {
            base_metadata.push_str(&d.caller);
            base_metadata.push_str(&d.callee);
        }
        for t in &candidate_tests {
            base_metadata.push_str(&t.test_file);
        }
        let base_tokens = estimate_tokens(&base_metadata);
        budget_manager.consume(base_tokens.min(token_budget / 3));

        let candidate_files_count = sorted_files.len();
        // Read and budget only the files that can be returned.
        sorted_files.truncate(15);
        let mut returned_files = Vec::new();
        let mut total_candidate_tokens = base_tokens;

        for (path, score, reasons) in sorted_files {
            if budget_manager.remaining() <= 100 {
                break;
            }

            let full_path = root.join(&path);
            let content_opt = fs::read_to_string(&full_path).ok();

            if let Some(ref content) = content_opt {
                let file_tokens = estimate_tokens(content);
                total_candidate_tokens += file_tokens;

                let snippet =
                    if budget_manager.can_fit(file_tokens) && content.lines().count() <= 80 {
                        budget_manager.consume(file_tokens);
                        Some(content.clone())
                    } else if budget_manager.remaining() > 100 {
                        // Extract focused excerpt around matched symbols
                        let target_lines: Vec<usize> = candidate_symbols
                            .iter()
                            .filter(|s| s.symbol.file_path == path)
                            .map(|s| s.symbol.start_line)
                            .collect();

                        let excerpt =
                            extract_excerpt(content, &target_lines, budget_manager.remaining());
                        let excerpt_tokens = estimate_tokens(&excerpt);
                        budget_manager.consume(excerpt_tokens);
                        Some(excerpt)
                    } else {
                        None
                    };

                returned_files.push(ScoredFile {
                    relative_path: path,
                    score,
                    reasons,
                    snippet,
                });
            } else {
                returned_files.push(ScoredFile {
                    relative_path: path,
                    score,
                    reasons,
                    snippet: None,
                });
            }
        }

        candidate_symbols.truncate(12);
        candidate_deps.truncate(10);
        candidate_tests.truncate(8);

        let git_summary = GitContextSummary {
            head_commit: repo_info.head_commit,
            branch: repo_info.branch,
            modified_files: diff_files,
            recent_commits: relevant_commits,
        };

        let budget_report = BudgetReport {
            candidate_tokens: total_candidate_tokens,
            returned_tokens: budget_manager.used,
            budget_limit: token_budget,
            candidate_files: candidate_files_count,
            returned_files: returned_files.len(),
        };

        Ok(EvidencePackage {
            task: task.to_string(),
            repo_id: repo_id.to_string(),
            staleness: staleness_str,
            relevant_files: returned_files,
            relevant_symbols: candidate_symbols,
            relevant_tests: candidate_tests,
            dependencies: candidate_deps,
            verified_memories: relevant_memories,
            git_context: git_summary,
            budget: budget_report,
        })
    }
}

/// Extract focused window around line numbers, respecting remaining token allowance.
fn extract_excerpt(content: &str, target_lines: &[usize], max_tokens: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    let mut included_lines = HashSet::new();

    if target_lines.is_empty() {
        // Take the top lines up to token limit
        let take_count = (max_tokens * 3).min(lines.len());
        for i in 0..take_count {
            included_lines.insert(i + 1);
        }
    } else {
        // Window of 5 lines before and 10 lines after each target symbol
        for &t in target_lines {
            let start = t.saturating_sub(5).max(1);
            let end = (t + 10).min(lines.len());
            for l in start..=end {
                included_lines.insert(l);
            }
        }
    }

    let mut line_nums: Vec<usize> = included_lines.into_iter().collect();
    line_nums.sort();

    let mut result = String::new();
    let mut last_line = 0;

    for &num in &line_nums {
        if last_line != 0 && num > last_line + 1 {
            result.push_str("    ...\n");
        }
        if num <= lines.len() {
            result.push_str(&format!("{:4}: {}\n", num, lines[num - 1]));
        }
        last_line = num;
        if estimate_tokens(&result) >= max_tokens {
            result.push_str("    ... [budget limit reached]\n");
            break;
        }
    }

    result
}
