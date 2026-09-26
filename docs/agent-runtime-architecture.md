# 02 Agent Runtime Architecture Specification

## 1. Vision & Core Philosophy

The **02 Agent Runtime** transforms **02_OS** into an *agent-native developer operating system*. Rather than implementing a traditional AI chat application or relying on generic RAG with cloud vector databases, 02_OS embeds a native, deterministic repository intelligence subsystem directly into the OS user session.

```
                 Coding Agents
     (Codex / Claude Code / OpenCode / Gemini CLI)
                         |
                         |  Model Context Protocol (JSON-RPC 2.0)
                         v
             +-----------------------+
             |   02 Agent Runtime    |
             |  (02agent / 02-agentd)|
             +-----------------------+
                |        |        |
         symbols|        |deps    |memories
                v        v        v
             +-----------------------+
             |    Evidence Graph     |
             |   (SQLite WAL + FTS5) |
             +-----------------------+
                         |
           +-------------+-------------+
           |             |             |
      Git History    Tree-sitter     Tests
     (libgit2 HEAD) (C/Rust/Py/etc)  & Docs
           |             |             |
           +-------------+-------------+
                         |
                         v
                 Git Working Diff
                         |
                         v
                 Staleness Engine
              (Symbol Fingerprints)
                         |
                         v
                 Context Compiler
            (Multi-Signal Scoring)
                         |
                         v
               Token Budget Allocator
               (Dynamic Knapsack)
                         |
                         v
                  Evidence Package
            (Structured JSON / Markdown)
                         |
                         v
                    Coding Agent
```

### Key Principles

1. **Local-First & Sovereign**: Runs 100% locally on the developer machine. No external databases, no mandatory LLM APIs, no telemetry.
2. **Git-Aware Truth**: Code facts are not freeform notes; every memory is tied to Git commits, blob OIDs, AST symbol fingerprints, and file paths.
3. **Deterministic AST Intelligence**: Employs Tree-sitter parsers to build symbol hierarchies, call graphs, import graphs, and test references.
4. **Automated Staleness & Invalidation**: Tracks working tree diffs in real time and automatically invalidates or degrades stale knowledge when underlying symbols change.
5. **Budgeted Context Compilation**: Gathers multi-signal evidence and compiles compact, high-density context packages tailored to coding agent token budgets.
6. **Vendor-Neutral MCP Service**: Exposes all services via the Model Context Protocol (MCP 2024-11-05) over stdio and Unix domain sockets.

---

## 2. System Architecture & Crates

The 02 Agent Runtime is built in Rust as a clean workspace located in `components/02-agent/`.

```
components/02-agent/
├── Cargo.toml
├── crates/
│   ├── agent-core/       # Types, RepoId, config, secrets, XDG paths
│   ├── agent-git/        # libgit2 repository discovery, status, diff hunks
│   ├── agent-parser/     # Tree-sitter AST extraction & fingerprinting
│   ├── agent-index/      # SQLite WAL, FTS5 full-text search, storage
│   ├── agent-memory/     # Evidence-based architectural memory & staleness
│   ├── agent-context/    # Context compiler & token budgeting engine
│   ├── agent-mcp/        # Model Context Protocol server & connect tool
│   ├── agent-daemon/     # 02-agentd user service & IPC socket listener
│   └── agent-cli/        # 02agent unified CLI command
├── benchmarks/           # Performance test suite
└── tests/                # Workspace integration tests
```

### 2.1 `agent-core`
- **`RepoId`**: Deterministic 64-bit hexadecimal identifier computed from the SHA-256 hash of the canonical repository root path.
- **`RepoInfo`**: Metadata describing the repository root, storage directory, branch, HEAD commit, and live environment.
- **`IndexedFile` & `FileKind`**: Categorizes files as `Source`, `Test`, `Doc`, `Config`, `Manifest`, or `Other`.
- **`Language`**: Language detection across Rust, Python, Dart, JavaScript, TypeScript, C, C++, Bash, and fallback.
- **`SecretPattern`**: Pre-indexing regex filters for AWS keys, GitHub tokens, Slack tokens, private keys, and high-entropy secrets.
- **XDG Base Directories**: Stores runtime indices under `$XDG_DATA_HOME/02-agent/repos/<repo-id>/` (`~/.local/share/02-agent/`).

### 2.2 `agent-git`
- Wraps `libgit2` (via `git2` crate) for zero-overhead, in-process Git operations.
- **Discovery**: Finds the enclosing `.git` directory and resolves worktrees.
- **HEAD & Branch**: Retrieves active branch name, detached HEAD status, and full 40-character commit hashes.
- **Working Tree Diffs**: Computes modified, added, deleted, and untracked files without spawning `git` subshell commands.
- **Diff Hunk Extraction**: Emits `DiffHunk` records with old/new line ranges and diff content for precise AST mapping.

### 2.3 `agent-parser`
- Implements Tree-sitter parsers for:
  - Rust (`tree-sitter-rust`)
  - Python (`tree-sitter-python`)
  - C & C++ (`tree-sitter-c`)
  - Bash (`tree-sitter-bash`)
  - JavaScript & TypeScript
  - Graceful regex fallback for unsupported languages.
- **Extracted Entities**:
  - `Symbol`: Function, Method, Class, Struct, Enum, Interface, Module, Variable.
  - `CallReference`: Caller-to-callee symbol relationships.
  - `ImportReference`: Module and symbol import declarations.
  - `TestReference`: Unit tests and test functions linked to source symbols.
- **Symbol Fingerprinting**: Computes SHA-256 hash of the normalized symbol AST subtree for micro-invalidation.

### 2.4 `agent-index`
- **Database Engine**: Embedded SQLite with WAL mode (`PRAGMA journal_mode=WAL;`), synchronous normal, and busy timeout.
- **FTS5 Virtual Table**: BM25-ranked full-text indexing over file paths and content with porter stemming.
- **Fast Traversal**: Parallel ignore-aware filesystem scanner (respecting `.gitignore`, `.git`, binary files, and secret patterns).
- **Incremental Updater**: Computes changed file mtimes and Git blob OIDs, updating only modified files in sub-100ms.

### 2.5 `agent-memory`
- **Evidence-Based Knowledge**: Memory records contain a claim, kind, valid commit, confidence score (0.0 to 1.0), status, and an array of evidence items.
- **`EvidenceItem`**: Specifies file path, referenced symbols, commit OID, and symbol SHA-256 fingerprint.
- **`MemoryStatus`**:
  - `Fresh`: Verified against current HEAD and working tree.
  - `Degraded`: Code touched or surrounding context changed, confidence lowered.
  - `Stale`: Linked symbol modified or deleted, requires verification.
  - `Invalidated`: Explicitly marked obsolete.
- **Storage**: Append-friendly JSONL stored at `~/.local/share/02-agent/repos/<repo-id>/memories.jsonl`.

### 2.6 `agent-context`
- **Task Intent Parsing**: Extracts identifiers, keywords, file paths, and operations from natural language prompts.
- **Multi-Signal Relevance Scoring**:
  1. Exact symbol name match (Score: +10.0)
  2. Modified in working tree / git diff (Score: +8.0)
  3. Partial / case-insensitive symbol match (Score: +5.0)
  4. Related test suites (Score: +4.5)
  5. Dependency / call-graph proximity (Score: +3.0)
  6. BM25 full-text search score (Normalized 0.0 - 5.0)
  7. Fresh architectural memories (Score: +2.5)
  8. Recent commit activity (Score: +1.5)
- **Token Budget Allocator**: Dynamic knapsack packing algorithm ensuring output remains strictly under the `--budget` limit (default 8,000 tokens).
- **Dual Format Output**: Emits structured JSON (`--json`) or human/agent-readable Markdown.

### 2.7 `agent-mcp`
- Standard JSON-RPC 2.0 implementation adhering to Model Context Protocol specification (version `2024-11-05`).
- **Registered Tools**:
  - `repository_status`: Branch, commit, file counts, memory counts, freshness.
  - `repository_search`: BM25 full-text search with snippet highlights.
  - `find_symbol`: Look up symbol declarations and line numbers.
  - `symbol_references`: Find callers, callees, and usages.
  - `symbol_dependencies`: Traverse dependency graph for a symbol.
  - `git_history`: Recent commit log with author, date, and message.
  - `git_diff_context`: Working tree diff mapped to modified symbols.
  - `related_tests`: Discover tests covering specified symbols.
  - `repository_context`: Main context compiler endpoint.
  - `memory_get`: Retrieve architectural memories by ID or tag.
  - `memory_write`: Record new verified evidence-based memory.
  - `memory_status`: Freshness audit of all repository memories.
- **Agent Connect**: `02agent connect <agent>` automatically configures Codex, Claude Code, and OpenCode.

### 2.8 `agent-daemon`
- **Binary**: `02-agentd`, packaged as a systemd user service (`02-agentd.service`).
- **IPC Transport**: Unix domain socket located at `$XDG_RUNTIME_DIR/02agent.sock` with restrictive `0600` permissions.
- **Security Check**: Enforces running as an unprivileged user (refuses root execution).
- **Repo Watcher**: Continuously monitors registered repositories (`watched_repos.json`) and triggers incremental indexing in the background.

---

## 3. SQLite Storage Schema

All index metadata is stored in `~/.local/share/02-agent/repos/<repo-id>/index.sqlite`:

```sql
-- File catalog
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    kind TEXT NOT NULL,
    language TEXT,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    blob_id TEXT,
    indexed_at INTEGER NOT NULL
);

-- Full-text search index (FTS5)
CREATE VIRTUAL TABLE IF NOT EXISTS files_fts USING fts5(
    path,
    content,
    tokenize = 'porter unicode61'
);

-- AST Symbol index
CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_col INTEGER NOT NULL,
    end_col INTEGER NOT NULL,
    signature TEXT,
    fingerprint TEXT NOT NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Cross-symbol relationships (calls, imports, tests)
CREATE TABLE IF NOT EXISTS symbol_references (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_symbol_id INTEGER,
    file_id INTEGER NOT NULL,
    target_name TEXT NOT NULL,
    kind TEXT NOT NULL, -- 'call', 'import', 'test_target', 'inheritance'
    line INTEGER NOT NULL,
    FOREIGN KEY(source_symbol_id) REFERENCES symbols(id) ON DELETE CASCADE,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Indices for rapid query response (<15ms)
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_files_kind ON files(kind);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_id);
CREATE INDEX IF NOT EXISTS idx_references_target ON symbol_references(target_name);
CREATE INDEX IF NOT EXISTS idx_references_source ON symbol_references(source_symbol_id);
```

---

## 4. Staleness & Invalidation State Machine

```
              Code Edit / Git Diff
                       |
                       v
         Identify Changed File Paths
                       |
                       v
           Extract Modified Line Hunks
                       |
                       v
       Map Hunks to Indexed AST Symbols
                       |
        +--------------+--------------+
        |                             |
  No Symbol Touched            Symbol Intersected
        |                             |
        v                             v
  Retain Confidence          Recompute Fingerprint
        |                             |
        |              +--------------+--------------+
        |              |                             |
        |       Fingerprint Identical      Fingerprint Changed
        |              |                             |
        |              v                             v
        |        Slight Decay                Mark Memory STALE
        |       (Context Touched)             (Confidence = 0.0)
        |              |                             |
        +--------------+-----------------------------+
                       |
                       v
            Emit Updated Repository Status
```

---

## 5. Token Budget Allocation Algorithm

The context compiler maximizes evidence density while staying strictly within the requested token budget:

1. **Budget Partitioning**:
   - Manifest & Environment Anchor: ~5% of budget.
   - Working Tree Diffs: ~15% of budget.
   - Top Ranked Symbols & Definitions: ~40% of budget.
   - Call Graphs & Dependency Chains: ~15% of budget.
   - Related Test Suites: ~15% of budget.
   - Architectural Memories & Commits: ~10% of budget.
2. **Greedy Knapsack Packing**: Each candidate chunk is estimated for tokens (4 characters per token heuristic). Items are packed in decreasing order of multi-signal relevance score.
3. **Smart Truncation**: If a symbol definition or test file exceeds the remaining chunk quota, the compiler preserves the signature and docstring while collapsing the body.
