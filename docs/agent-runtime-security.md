# 02 Agent Runtime Security & Isolation Model

## 1. Security Philosophy & Threat Model

The **02 Agent Runtime** is an OS-level developer service designed under strict **Local-First, Zero-Trust, and Least-Privilege** principles.

Because coding agents interact with proprietary source code and sensitive repository environments, the runtime guarantees:

1. **Zero External Data Exfiltration**: The runtime never initiates external network requests, transmits telemetry, or contacts cloud endpoints.
2. **Zero Provider Lock-In**: The runtime does not handle or store proprietary LLM API keys. It functions strictly as a local repository intelligence engine over standard MCP.
3. **Secret Isolation**: Sensitive files, private keys, and API tokens are detected and stripped before entering search indices or evidence packages.
4. **Non-Root Sandboxing**: The daemon enforces non-root execution and operates inside a hardened systemd user environment.
5. **Multi-User IPC Isolation**: Inter-process communication occurs over Unix domain sockets restricted exclusively to the owning user (`0600`).

---

## 2. Secret Scanning & Exclusion Pipeline

The indexing engine (`agent-index`) integrates pre-indexing pattern matching (`agent-core::SecretPattern`) to prevent sensitive credentials from ever being written into SQLite WAL databases or FTS5 search indices.

### Filtered Patterns

| Category | Detection Regex / Rule | Action |
| :--- | :--- | :--- |
| **Private Keys** | `-----BEGIN (RSA\|OPENSSH\|EC\|PGP) PRIVATE KEY-----` | Exclude file / redact content |
| **AWS Credentials** | `AKIA[0-9A-Z]{16}`, AWS secret access key patterns | Redact match |
| **GitHub Tokens** | `ghp_[A-Za-z0-9_]{36}`, `github_pat_[A-Za-z0-9_]{82}` | Redact match |
| **Slack Tokens** | `xox[baprs]-[0-9A-Za-z]{10,48}` | Redact match |
| **Generic API Keys** | High-entropy strings matching `(api[_-]?key\|secret\|token)\s*[:=]\s*["'][A-Za-z0-9_\-]{20,}["']` | Redact match |
| **Environment Files** | `.env`, `.env.local`, `.env.*.local` | Excluded from text indexing |

Developers can audit their repository at any time:

```bash
02 security
```

---

## 3. Privilege Separation & Daemon Hardening

### Non-Root Enforcement

The `02-agentd` background service refuses to run as UID 0 (`root`). If invoked with root privileges, it immediately terminates with an error:

```text
ERROR: 02-agentd must run as a regular user, not as root (UID 0).
```

### Systemd User Sandboxing

In **02_OS**, `02-agentd` is managed as an unprivileged user service (`02-agentd.service`) under `systemd --user`. The service profile applies defensive Linux sandboxing flags:

```ini
[Unit]
Description=02 Agent Runtime Daemon
Documentation=man:02agent(1)
After=default.target

[Service]
Type=simple
ExecStart=/usr/bin/02-agentd
Restart=on-failure
RestartSec=3s

# Security Hardening
ProtectSystem=strict
ReadWritePaths=%h/.local/share/02-agent %t
ProtectHome=read-only
PrivateTmp=true
NoNewPrivileges=true

[Install]
WantedBy=default.target
```

- **`ProtectSystem=strict`**: Mounts `/usr`, `/boot`, `/etc`, and system directories read-only.
- **`ProtectHome=read-only`**: Prevents arbitrary modification of user home directories, with write access scoped strictly to `%h/.local/share/02-agent` and `%t` (`$XDG_RUNTIME_DIR`).
- **`PrivateTmp=true`**: Provides an isolated `/tmp` namespace distinct from other user sessions.
- **`NoNewPrivileges=true`**: Disallows gaining new privileges via `setuid` binaries.

---

## 4. Socket Security & IPC Isolation

Communications between coding agents, `02` CLI, and `02-agentd` take place over a local Unix domain socket:

```
Primary:   $XDG_RUNTIME_DIR/02agent.sock
Fallback:  ~/.local/share/02-agent/02agent.sock
```

### Access Restrictions

- **File Permissions**: The socket is created with `0600` permissions (`-rw-------`). Only the user who spawned the daemon has read and write access.
- **Directory Permissions**: The storage directory `~/.local/share/02-agent` is initialized with `0700` (`drwx------`).
- **Peer Credential Checking**: Unix peer credentials (`SO_PEERCRED`) verify the connecting client PID and UID match the daemon owner.

---

## 5. Live-User & OverlayFS Storage Safety

When running in the 02_OS live ISO environment:

- The default live user is `live` (UID 1000).
- The root filesystem is a memory-backed OverlayFS (`cowspace`).
- **RAM Preservation**:
  - SQLite indexes operate with `PRAGMA synchronous = NORMAL;` and `PRAGMA cache_size = -64000;` (64 MB max memory cache).
  - Write-Ahead Logging (WAL) automatically checkpoints to prevent unbounded file growth in RAM.
  - Ignored directories (`target/`, `node_modules/`, `.git/objects/`, `build/`, `dist/`) are pruned prior to indexing, keeping RAM footprint minimal (typically < 30 MB per indexed repository).

---

## 6. Audit & Health Verification

Developers and security teams can verify the operational integrity of the agent runtime using the built-in diagnostic suite:

```bash
# Verify system environment, storage permissions, and live-user state
02 doctor

# Scan repository for credentials or insecure configuration
02 security
```
