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

echo "==> Enforcing polkit rules directory permissions..."
if [[ -d /etc/polkit-1/rules.d ]]; then
  chmod 750 /etc/polkit-1/rules.d
  chmod 644 /etc/polkit-1/rules.d/* 2>/dev/null || true
fi

echo "==> Locking the root account for the live environment..."
passwd -l root 2>/dev/null || true

echo "==> Uncommenting HTTPS mirrors in /etc/pacman.d/mirrorlist..."
if [[ -f /etc/pacman.d/mirrorlist ]]; then
  sed -i -E 's/^#(Server = https:)/\1/' /etc/pacman.d/mirrorlist 2>/dev/null || true
  if ! grep -q '^Server = https' /etc/pacman.d/mirrorlist 2>/dev/null; then
    sed -i -E 's/^#(Server =)/\1/' /etc/pacman.d/mirrorlist 2>/dev/null || true
  fi
fi

