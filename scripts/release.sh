#!/usr/bin/env bash
# ==============================================================================
# 02_OS Release Orchestrator
# Builds the ISO, splits into upload chunks, generates checksums, and publishes
# release to GitHub using gh CLI.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PROFILE_DIR="${REPO_ROOT}/profile"
OUT_DIR="${REPO_ROOT}/out"

EXISTING_ISO="$(find "${OUT_DIR}" -maxdepth 1 -name "02_OS-*.iso" 2>/dev/null | head -n 1 || true)"

if [[ -n "${EXISTING_ISO}" && -f "${EXISTING_ISO}" && "${1:-}" != "--force-rebuild" ]]; then
    echo "Found existing ISO at ${EXISTING_ISO}. Skipping build step."
    ISO_PATH="${EXISTING_ISO}"
else
    echo "=== [Step 1/5] Building 02_OS Live ISO ==="
    "${REPO_ROOT}/build.sh"
    ISO_PATH="$(find "${OUT_DIR}" -maxdepth 1 -name "02_OS-*.iso" | head -n 1)"
fi

if [[ -z "${ISO_PATH:-}" || ! -f "${ISO_PATH}" ]]; then
    echo "ERROR: ISO file not found in ${OUT_DIR}" >&2
    exit 1
fi

ISO_NAME="$(basename "${ISO_PATH}")"
ISO_VERSION="$(echo "${ISO_NAME}" | sed -E 's/^02_OS-(.*)-x86_64\.iso$/\1/')"
TAG_NAME="v${ISO_VERSION}"
RELEASE_TITLE="02_OS v${ISO_VERSION}"

echo "============================================================"
echo " Starting 02_OS Release: ${RELEASE_TITLE} (${TAG_NAME})"
echo " Repo: ${REPO_ROOT}"
echo " ISO:  ${ISO_PATH} ($(du -h "${ISO_PATH}" | cut -f1))"
echo "============================================================"

# Step 2: Generate Checksums & Split ISO
echo "=== [Step 2/5] Splitting ISO into 1.4GB Parts & Computing Checksums ==="
cd "${OUT_DIR}"

# Full ISO checksum
echo "Calculating sha256 for ${ISO_NAME}..."
sha256sum "${ISO_NAME}" > "02_OS.sha256"

# Remove any old split parts
rm -f "${ISO_NAME}.part"* "02_OS-parts.sha256"

# Split into 1400M chunks (partaa, partab, ...)
echo "Splitting ${ISO_NAME} into 1400MB parts..."
split -b 1400M "${ISO_NAME}" "${ISO_NAME}.part"

# Parts checksum
echo "Calculating sha256 for split parts..."
sha256sum "${ISO_NAME}.part"* > "02_OS-parts.sha256"

ls -lh "${OUT_DIR}"

# Step 3: Git Tag
echo "=== [Step 3/5] Creating and Pushing Git Tag ${TAG_NAME} ==="
cd "${REPO_ROOT}"
if git rev-parse "${TAG_NAME}" >/dev/null 2>&1; then
    echo "Tag ${TAG_NAME} already exists locally, replacing..."
    git tag -d "${TAG_NAME}"
fi

git tag -a "${TAG_NAME}" -m "Release ${RELEASE_TITLE}"
git push origin "${TAG_NAME}" --force

# Step 4: Prepare Release Notes
echo "=== [Step 4/5] Preparing Release Notes ==="
NOTES_FILE="${OUT_DIR}/RELEASE_NOTES.md"
cat > "${NOTES_FILE}" <<EOF
# 02_OS v${ISO_VERSION} — Agent-Native Developer Operating System

Welcome to the **02_OS v${ISO_VERSION}** release!

This release introduces the **02 Agent Runtime** — an operating-system-level repository intelligence service and Model Context Protocol (MCP) server built directly into 02_OS for autonomous coding agents (Codex, Claude Code, OpenCode, Gemini CLI, Cursor).

### What's New

- **02 Agent Runtime (\`02\` & \`02-agentd\`)**:
  - **100% Local-First**: Zero telemetry, no cloud DBs, no external LLM dependencies.
  - **Git-Aware Truth**: Real-time diff tracking and symbol-level staleness detection.
  - **Tree-sitter AST Evidence Graph**: Multi-language symbol parsing (Rust, Python, C/C++, Bash, JS/TS) with callers, callees, imports, and tests.
  - **Architectural Memory**: Git-anchored verified facts with automated confidence degradation.
  - **Token Budget Context Compiler**: Compiles high-density evidence packages tailored to LLM context windows (\`02 context "<task>" --budget 8000\`).
  - **Standard MCP Server**: Vendor-neutral JSON-RPC 2.0 interface supporting 12 agent tools (\`02 mcp\`, \`02 connect\`).
  - **Pre-installed Background User Daemon**: \`02-agentd\` enabled via \`systemd --user\`.
- **Refined GNOME Desktop**:
  - GNOME Shell 50 + Pop Shell window tiling (\`Super+Y\`).
  - Centered floating Dash-to-Dock with pinned terminal and installer.
  - Custom handcrafted **02-OS Glassmorphic Vector Icon Theme** (70+ scalable SVGs).
  - Precompiled GLib schemas and dconf database for faster boot and minimal RAM footprint.
- **archinstall**:
  - Live session user \`live\` (password \`live\`) with an unprivileged desktop session.
  - Install from GNOME Console via the Install 02_OS launcher, or run \`sudo archinstall\`.
- **Modern Linux Audio & Networking**:
  - PipeWire audio stack with automatic live unmuting.
  - Clean NetworkManager stack.

---

### Joining Split Parts

To reassemble the full ISO image after downloading all parts:

\`\`\`bash
cat ${ISO_NAME}.part* > ${ISO_NAME}
\`\`\`

---

### Verification

Verify the reassembled ISO image integrity:

\`\`\`bash
sha256sum -c 02_OS.sha256
\`\`\`

Or verify the split parts individually:

\`\`\`bash
sha256sum -c 02_OS-parts.sha256
\`\`\`
EOF

# Step 5: Publish Release via gh CLI
echo "=== [Step 5/5] Publishing GitHub Release with Assets ==="
ASSETS=(
    "${OUT_DIR}/02_OS.sha256"
    "${OUT_DIR}/02_OS-parts.sha256"
)

for part in "${OUT_DIR}/${ISO_NAME}.part"*; do
    if [[ -f "${part}" ]]; then
        ASSETS+=("${part}")
    fi
done

echo "Uploading assets to release ${TAG_NAME}:"
printf '  %s\n' "${ASSETS[@]}"

if gh release view "${TAG_NAME}" >/dev/null 2>&1; then
    echo "Release ${TAG_NAME} already exists on GitHub, updating..."
    gh release upload "${TAG_NAME}" "${ASSETS[@]}" --clobber
    gh release edit "${TAG_NAME}" --title "${RELEASE_TITLE}" --notes-file "${NOTES_FILE}"
else
    echo "Creating new GitHub release ${TAG_NAME}..."
    gh release create "${TAG_NAME}" "${ASSETS[@]}" \
        --title "${RELEASE_TITLE}" \
        --notes-file "${NOTES_FILE}" \
        --latest
fi

echo "============================================================"
echo " 02_OS Release ${TAG_NAME} Published Successfully!"
echo " URL: $(gh release view "${TAG_NAME}" --json url -q .url)"
echo "============================================================"
