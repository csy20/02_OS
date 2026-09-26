# 02_OS

`02_OS` is a custom Arch Linux live ISO featuring a streamlined **GNOME** desktop with Pop Shell tiling capabilities, a floating Dash-to-Dock, and a custom **02-OS Glassmorphic Vector Icon Theme**.

Built with `archiso` in a privileged Docker container.

---

## Look and feel

| Piece | What you get |
| :--- | :--- |
| **Desktop Environment** | GNOME Shell 50 + Pop Shell (optional tiling toggle with `Super+Y`) |
| **Dock** | Dash-to-Dock — Centered, dynamic floating dock with pinned favorites and trash |
| **Theme** | adw-gtk3-dark, Capitaine cursors, Inter 11 |
| **Icons** | **02-OS** — Custom handcrafted glassmorphic vector icon theme (70+ scalable SVGs) |
| **Terminal** | GNOME Console (`org.gnome.Console`) |
| **File Manager** | Nautilus (`org.gnome.Nautilus`) |
| **Audio / Net** | PipeWire + NetworkManager (no duplicate network stacks) |

Live session user is **`live`** (autologin, password **`live`**). The desktop does **not** run as root. Use that password on the lock screen or for `sudo` commands.

---

## Keybinds

| Shortcut | Action |
| :--- | :--- |
| **Alt+F4** / **Super+Q** | Close focused window |
| **Super+Y** | Toggle Pop Shell window tiling (floating by default) |
| **Super+M** | Toggle maximize window |
| **Super+,** | Minimize window |
| **Super+V** | Toggle notification / message tray |
| **Super** | Overview / App Launcher |
| **Super+1..4** | Switch workspaces |

---

## Performance & Optimization

- **Single DE Architecture**: Pruned conflicting Hyprland / XFCE packages, leaving a clean, lean GNOME stack with no portal conflicts.
- **Build-Time Schema Compilation**: GLib schemas and dconf databases are precompiled during ISO build, saving boot time and RAM overlayfs space.
- **Clean User Management**: System users (`greeter`, `polkitd`, `dbus`) preserved via Arch packages and `sysusers.d`.
- **Deduplicated Skeleton**: Unified dotfiles in `/etc/skel` as the single source of truth.
- **zstd** squashfs + initramfs (faster decompression and boot).
- **zram** swap with tuned swappiness.

---

## Repository layout

```
02_OS/
├── README.md
├── build.sh
├── 02-OS-icons-preview.html   # Visual preview gallery for 02-OS icon pack
├── profile/
│   ├── profiledef.sh          # ISO build configuration and file permissions
│   ├── packages.x86_64        # Curated package manifest
│   ├── pacman.conf
│   └── airootfs/              # Live filesystem overlay:
│       ├── etc/dconf/         # Desktop defaults and keybindings (Pop-shell & Dash-to-Dock)
│       ├── etc/greetd/        # Greetd autologin configuration
│       ├── etc/skel/          # User default profile and GTK settings
│       ├── etc/sysusers.d/    # Declarative live user creation
│       ├── root/              # Build-time customization script (schema & dconf compiler)
│       └── usr/share/icons/02-OS/  # Glassmorphic vector icon theme
├── scripts/
│   └── icon-generator/        # Python generator suite for the 02-OS icon pack
└── out/                       # Generated ISO images
```

---

## Building

```bash
./build.sh
```

Manual Docker build:

```bash
docker run --rm -it --privileged \
  -v $(pwd)/profile:/02_OS \
  -v $(pwd)/out:/out \
  -v /tmp/archiso-tmp:/tmp/archiso-tmp \
  archlinux:latest bash
```

Inside the container:

```bash
pacman -Sy --noconfirm archiso
mkarchiso -v -w /tmp/archiso-tmp -o /out /02_OS
```

---

## Test in QEMU

```bash
sudo apt update && sudo apt install -y qemu-system-x86
qemu-system-x86_64 -enable-kvm -m 4G -cdrom out/02_OS-*.iso -boot d
```

Accelerated rendering in QEMU:
`-device virtio-vga-gl -display gtk,gl=on`

---

## Install

From the live desktop: run `sudo archinstall` from terminal or Settings.

`archinstall` will guide you through partitioning, username creation, and bootloader configuration.

---

## Sign and flash

```bash
gpg --detach-sign --armor out/02_OS-*.iso
sha256sum out/02_OS-*.iso > out/02_OS.sha256
sudo dd if=out/02_OS-*.iso of=/dev/sdX bs=4M status=progress oflag=sync
```
