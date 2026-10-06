#!/bin/sh
# Create the 02-agent data, cache, and config directories before the
# ProtectSystem=strict sandbox is applied. ExecStartPre=+ runs this
# outside the mount namespace. Do not chmod through a symlink: a final
# symlink is replaced with a real directory, then mode 0700 is set on
# that directory.
set -eu

umask 077

create_private_dir() {
  dir="$1"
  parent=$(dirname -- "$dir")
  if [ "$parent" != "/" ] && [ "$parent" != "." ]; then
    mkdir -p -- "$parent"
  fi
  if [ -L "$dir" ]; then
    rm -- "$dir"
  fi
  if [ ! -d "$dir" ]; then
    mkdir -- "$dir"
  fi
  if [ -L "$dir" ] || [ ! -d "$dir" ]; then
    echo "prepare-storage: refusing to chmod $dir (not a real directory)" >&2
    exit 1
  fi
  chmod 0700 -- "$dir"
  if [ -L "$dir" ]; then
    echo "prepare-storage: $dir is a symlink; refusing to follow it" >&2
    exit 1
  fi
}

base_dir() {
  xdg="$1"
  fallback="$2"
  label="$3"
  if [ -n "$xdg" ]; then
    printf '%s' "${xdg%/}"
    return
  fi
  if [ -z "${HOME:-}" ]; then
    echo "prepare-storage: HOME is unset and $label is unset" >&2
    exit 1
  fi
  printf '%s/%s' "$HOME" "$fallback"
}

data_base=$(base_dir "${XDG_DATA_HOME:-}" ".local/share" "XDG_DATA_HOME")
cache_base=$(base_dir "${XDG_CACHE_HOME:-}" ".cache" "XDG_CACHE_HOME")
config_base=$(base_dir "${XDG_CONFIG_HOME:-}" ".config" "XDG_CONFIG_HOME")

create_private_dir "$data_base/02-agent"
create_private_dir "$cache_base/02-agent"
create_private_dir "$config_base/02-agent"

# The packaged unit names the default XDG paths with no '-' prefix.
# Create them even when XDG_* points somewhere else, or namespace setup
# fails before the daemon starts.
if [ -n "${HOME:-}" ]; then
  create_private_dir "$HOME/.local/share/02-agent"
  create_private_dir "$HOME/.cache/02-agent"
  create_private_dir "$HOME/.config/02-agent"
fi
