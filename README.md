# 02_OS

`02_OS` is a custom Arch Linux live ISO: a **Hyprland** desktop that looks like a Mac, boots quickly, and uses **Windows-accurate Alt+F4** (close the focused window; on the empty desktop, a shutdown dialog).

Built with `archiso` in a privileged Docker container.

---

## Look and feel

| Piece | What you get |
| :--- | :--- |
| **Window manager** | Hyprland — rounded windows, light blur, GUI apps float |
| **Top bar** | Waybar — 02_OS menu, workspaces, clock, Wi‑Fi, volume, battery |
| **Dock** | `nwg-dock-hyprland` — Files, Firefox, Terminal, Sound, Installer |
| **Launcher** | Fuzzel as Spotlight (`Super+Space` or `Alt+Space`) |
| **Theme** | adw-gtk3-dark, Papirus, Capitaine cursors, Inter |
| **Audio / net** | PipeWire + NetworkManager (no duplicate network stacks) |

Live session user is **`live`** (autologin, password **`live`**). The desktop does **not** run as root. Use that password on the lock screen (Super+L or idle).

---

## Keybinds

| Shortcut | Action |
| :--- | :--- |
| **Alt+F4** | Close focused window; if none, shutdown / restart / sleep dialog |
| Super+Q / Super+W | Quit focused window |
| Super+Space / Alt+Space | Spotlight |
| Super+Tab | Window switcher |
| Super+T | Toggle floating |
| Super+Return | Terminal (Kitty) |
| Super+N | Files (Thunar) |
| Super+B | Firefox |
| Super+L | Lock |
| Super+Shift+3 | Screenshot (screen) |
| Super+Shift+4 / Print | Screenshot (region) |
| Super+Shift+V | Clipboard history |
| Super+A | 02_OS menu |
| Super+1..4 | Workspaces |

---

## Performance (vs the old rescue ISO)

- Rescue/hypervisor/cloud-init/iwd packages removed
- **zstd** squashfs + initramfs (faster boot than xz)
- **zram** swap, tuned swappiness
- One network stack: NetworkManager
- Sleep/lid handling enabled
- Mesa + Intel/AMD Vulkan only (no NVIDIA proprietary blob on the default ISO)

NVIDIA users: the live session uses nouveau/mesa. A proprietary-driver ISO is a later variant.

---

## Repository layout

```
/home/csy20/Documents/dev/02_OS/
├── README.md
├── build.sh
├── profile/
│   ├── profiledef.sh
│   ├── packages.x86_64
│   └── airootfs/          # overlay: session, Hyprland, helpers
└── out/                   # generated ISOs
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

Give QEMU a virtio GPU if you can (`-device virtio-vga-gl -display gtk,gl=on`) so Hyprland has a usable renderer.

---

## Install

From the live desktop: dock **Install 02_OS**, or `sudo archinstall`.

`archinstall` will ask for a username and bootloader. After install, boot from disk and log in (agreety, then Hyprland via uwsm).

---

## Sign and flash

```bash
gpg --detach-sign --armor out/02_OS-*.iso
sha256sum out/02_OS-*.iso > out/02_OS.sha256
sudo dd if=out/02_OS-*.iso of=/dev/sdX bs=4M status=progress oflag=sync
```

Identify `sdX` with `lsblk` first.
