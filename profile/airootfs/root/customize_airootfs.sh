#!/usr/bin/env bash
set -e

# Build-time customization for 02_OS ISO
echo "==> Compiling GLib schemas in /usr/share/glib-2.0/schemas..."
glib-compile-schemas /usr/share/glib-2.0/schemas

echo "==> Updating dconf system database..."
dconf update

echo "==> Generating icon theme cache for 02-OS..."
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f /usr/share/icons/02-OS || true
fi
