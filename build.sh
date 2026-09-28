#!/usr/bin/env bash
set -euo pipefail

# 02_OS ISO build script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILE_DIR="${SCRIPT_DIR}/profile"
OUT_DIR="${SCRIPT_DIR}/out"
# /tmp is often a small tmpfs. Keep the rootfs work tree on disk.
WORK_DIR="${WORK_DIR:-/var/tmp/02_OS-build-work}"
PACMAN_CACHE_DIR="${PACMAN_CACHE_DIR:-/var/cache/pacman/pkg}"
MIN_FREE_GB="${MIN_FREE_GB:-20}"

HOST_UID="$(id -u)"
HOST_GID="$(id -g)"

mkdir -p "${OUT_DIR}"
mkdir -p "${WORK_DIR}"

avail_kb="$(df -Pk "${WORK_DIR}" | awk 'NR==2 {print $4}')"
min_kb=$((MIN_FREE_GB * 1024 * 1024))
if [[ -z "${avail_kb}" || "${avail_kb}" -lt "${min_kb}" ]]; then
  echo "ERROR: ${WORK_DIR} needs at least ${MIN_FREE_GB} GB free (available KB: ${avail_kb:-unknown})." >&2
  echo "Set WORK_DIR to a filesystem with enough space and retry." >&2
  exit 1
fi

if [[ ! -d "${PACMAN_CACHE_DIR}" ]]; then
  PACMAN_CACHE_DIR="${SCRIPT_DIR}/.cache/pacman"
  mkdir -p "${PACMAN_CACHE_DIR}"
fi

# A failed mkarchiso used to leave root-owned files behind, so the next rm failed.
if [[ -d "${WORK_DIR}" ]] && ! rm -rf "${WORK_DIR}"; then
  echo "Work directory is not writable; reclaiming ownership inside Docker..."
  docker run --rm --privileged \
    -e HOST_UID="${HOST_UID}" \
    -e HOST_GID="${HOST_GID}" \
    -v "${WORK_DIR}:${WORK_DIR}" \
    archlinux:latest \
    chown -R "${HOST_UID}:${HOST_GID}" "${WORK_DIR}"
  rm -rf "${WORK_DIR}"
fi
mkdir -p "${WORK_DIR}"

# Build and stage 02 Agent Runtime binaries into airootfs
if [[ -f "${SCRIPT_DIR}/scripts/build-agent-runtime.sh" ]]; then
  echo "Building and staging 02 Agent Runtime..."
  "${SCRIPT_DIR}/scripts/build-agent-runtime.sh"
fi

echo "============================================================"
echo " Building 02_OS ISO via Docker (Arch Linux container)"
echo " Profile: ${PROFILE_DIR}"
echo " Output:  ${OUT_DIR}"
echo " Work:    ${WORK_DIR}"
echo " Cache:   ${PACMAN_CACHE_DIR}"
echo "============================================================"

docker run --rm --privileged \
  -e HOST_UID="${HOST_UID}" \
  -e HOST_GID="${HOST_GID}" \
  -e WORK_DIR="${WORK_DIR}" \
  -v "${PROFILE_DIR}:/02_OS" \
  -v "${OUT_DIR}:/out" \
  -v "${WORK_DIR}:${WORK_DIR}" \
  -v "${PACMAN_CACHE_DIR}:/var/cache/pacman/pkg" \
  archlinux:latest bash -c '
    set -euo pipefail
    trap "chown -R \"${HOST_UID}:${HOST_GID}\" /out \"${WORK_DIR}\" 2>/dev/null || true" EXIT
    pacman -Syu --noconfirm archiso
    mkarchiso -v -w "${WORK_DIR}" -o /out /02_OS
  '

echo ""
echo "============================================================"
echo " ISO Build Complete!"
echo " Output ISO directory: ${OUT_DIR}"
echo "============================================================"
ls -lh "${OUT_DIR}"
