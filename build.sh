#!/usr/bin/env bash
set -euo pipefail

# 02_OS ISO build script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROFILE_DIR="${SCRIPT_DIR}/profile"
OUT_DIR="${SCRIPT_DIR}/out"
WORK_DIR="/tmp/archiso-tmp"

mkdir -p "${OUT_DIR}"

# Clean previous workdir to avoid stale build artifacts
if [[ -d "${WORK_DIR}" ]]; then
  echo "Cleaning stale work directory ${WORK_DIR}..."
  rm -rf "${WORK_DIR}"
fi
mkdir -p "${WORK_DIR}"

echo "============================================================"
echo " Building 02_OS ISO via Docker (Arch Linux container)"
echo " Profile: ${PROFILE_DIR}"
echo " Output:  ${OUT_DIR}"
echo "============================================================"

docker run --rm --privileged \
  -v "${PROFILE_DIR}:/02_OS" \
  -v "${OUT_DIR}:/out" \
  -v "${WORK_DIR}:${WORK_DIR}" \
  archlinux:latest bash -c "
    pacman -Syu --noconfirm archiso && \
    mkarchiso -v -w ${WORK_DIR} -o /out /02_OS
  "

echo ""
echo "============================================================"
echo " ISO Build Complete!"
echo " Output ISO directory: ${OUT_DIR}"
echo "============================================================"
ls -lh "${OUT_DIR}"
