#!/usr/bin/env bash
# ==============================================================================
# 02 Agent Runtime Build Script
# Compiles 02 Agent Runtime (02agent, 02-agentd) and stages into ArchISO airootfs.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
AGENT_DIR="${REPO_ROOT}/components/02-agent"
DEST_DIR="${REPO_ROOT}/profile/airootfs/usr/bin"

echo "=== [02 Agent Runtime] Compiling Release Binaries ==="

if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: 'cargo' is required to build the 02 Agent Runtime." >&2
    echo "Install Rust via 'pacman -S rust' or 'rustup'." >&2
    exit 1
fi

# Build release binaries in components/02-agent
cargo build --release --manifest-path "${AGENT_DIR}/Cargo.toml"

echo "=== [02 Agent Runtime] Running Verification Tests ==="
cargo test --manifest-path "${AGENT_DIR}/Cargo.toml"

echo "=== [02 Agent Runtime] Staging Binaries into airootfs ==="
mkdir -p "${DEST_DIR}"

cp -f "${AGENT_DIR}/target/release/02agent" "${DEST_DIR}/02agent"
cp -f "${AGENT_DIR}/target/release/02-agentd" "${DEST_DIR}/02-agentd"

chmod 755 "${DEST_DIR}/02agent" "${DEST_DIR}/02-agentd"

# Strip debug symbols if strip is present to conserve squashfs space
if command -v strip >/dev/null 2>&1; then
    echo "Stripping binaries..."
    strip "${DEST_DIR}/02agent" "${DEST_DIR}/02-agentd" || true
fi

echo "Staged binaries:"
ls -lh "${DEST_DIR}/02agent" "${DEST_DIR}/02-agentd"

echo "=== [02 Agent Runtime] Build & Staging Complete ==="
