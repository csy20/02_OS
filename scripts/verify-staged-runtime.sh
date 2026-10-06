#!/usr/bin/env bash
# Fail unless staged 02 and 02-agentd are the stripped release build of this
# tree and SOURCE_REVISION records those hashes.
set -euo pipefail

if [[ "${1:-}" == "--repo" ]]; then
  [[ $# -eq 2 ]] || { echo "Usage: verify-staged-runtime.sh [--repo ROOT]" >&2; exit 2; }
  repo_root="$(cd "$2" && pwd)"
elif [[ $# -eq 0 ]]; then
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  repo_root="$(cd "${script_dir}/.." && pwd)"
else
  echo "Usage: verify-staged-runtime.sh [--repo ROOT]" >&2
  exit 2
fi

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

# Same target directory as build-agent-runtime.sh. Trees without a manifest
# (unit fixtures) still compare the default target/release path.
resolve_release_dir() {
  local manifest="${repo_root}/components/02-agent/Cargo.toml"
  local meta target_dir
  if [[ ! -f "${manifest}" ]]; then
    printf '%s\n' "${repo_root}/components/02-agent/target/release"
    return 0
  fi
  command -v cargo >/dev/null 2>&1 || fail "cargo is required to resolve the target directory"
  meta="$(cargo metadata --format-version 1 --no-deps --manifest-path "${manifest}")" \
    || fail "cargo metadata failed for ${manifest}"
  if command -v python3 >/dev/null 2>&1; then
    target_dir="$(printf '%s' "${meta}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')" \
      || fail "could not parse cargo target_directory"
  else
    target_dir="$(printf '%s' "${meta}" | sed -n 's/.*"target_directory"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    target_dir="${target_dir//\\\//\/}"
  fi
  [[ -n "${target_dir}" ]] || fail "cargo metadata did not report target_directory"
  printf '%s\n' "${target_dir%/}/release"
}

if ! release="$(resolve_release_dir)"; then
  exit 1
fi
staged="${repo_root}/profile/airootfs/usr/bin"
prov="${repo_root}/profile/airootfs/usr/lib/02-agent/SOURCE_REVISION"

[[ -f "${release}/02" && ! -L "${release}/02" ]] || fail "source build is missing: ${release}/02"
[[ -f "${release}/02-agentd" && ! -L "${release}/02-agentd" ]] || fail "source build is missing: ${release}/02-agentd"
[[ -f "${staged}/02" && ! -L "${staged}/02" ]] || fail "staged 02 is missing: ${staged}/02"
[[ -f "${staged}/02-agentd" && ! -L "${staged}/02-agentd" ]] || fail "staged 02-agentd is missing: ${staged}/02-agentd"
if [[ ! -L "${staged}/02agent" ]] || [[ "$(readlink -- "${staged}/02agent")" != "02" ]]; then
  fail "staged 02agent must be a symlink to 02"
fi

cmp_stripped() {
  local src="$1" dest="$2" tmp
  tmp="$(mktemp)"
  cp -f -- "${src}" "${tmp}"
  if command -v strip >/dev/null 2>&1; then
    strip "${tmp}" || true
  fi
  if ! cmp -s -- "${tmp}" "${dest}"; then
    rm -f -- "${tmp}"
    fail "staged $(basename -- "${dest}") does not match the source build ${src}"
  fi
  rm -f -- "${tmp}"
}

cmp_stripped "${release}/02" "${staged}/02"
cmp_stripped "${release}/02-agentd" "${staged}/02-agentd"

[[ -f "${prov}" && ! -L "${prov}" ]] || fail "missing provenance ${prov}"

field() {
  local key="$1"
  awk -F= -v key="${key}" '$1 == key { print substr($0, length(key) + 2); exit }' "${prov}"
}

expect_02="$(field sha256_02)"
if [[ -z "${expect_02}" ]]; then
  expect_02="$(field sha256)"
fi
expect_daemon="$(field sha256_02_agentd)"
actual_02="$(sha256sum "${staged}/02" | awk 'NR==1 { print $1 }')"
actual_daemon="$(sha256sum "${staged}/02-agentd" | awk 'NR==1 { print $1 }')"
[[ -n "${expect_02}" && "${expect_02}" == "${actual_02}" ]] || fail "SOURCE_REVISION sha256 does not match staged 02"
[[ -n "${expect_daemon}" && "${expect_daemon}" == "${actual_daemon}" ]] || fail "SOURCE_REVISION sha256 does not match staged 02-agentd"

revision="$(field revision)"
if git_rev="$(git -C "${repo_root}" rev-parse HEAD 2>/dev/null)" && [[ -n "${git_rev}" ]]; then
  [[ "${revision}" == "${git_rev}" ]] || fail "SOURCE_REVISION revision ${revision:-empty} is not HEAD ${git_rev}"
fi
