# 02_OS - Custom Arch Linux Distribution

`02_OS` is a customized, reproducible Arch Linux live ISO built using `archiso` inside a privileged Docker container. It comes pre-configured with Sway (Wayland window manager), Neovim, Firefox, PipeWire audio, NetworkManager, and custom user dotfiles.

---

## 📁 Repository Structure

```
/home/csy20/Documents/dev/02_OS/
├── README.md                 # Complete documentation & workflow guide
├── build.sh                  # One-command ISO build script (Docker-based)
├── profile/                  # Customized archiso profile configuration
│   ├── profiledef.sh         # Distro branding, iso_name, iso_label, & versioning
│   ├── packages.x86_64       # Package manifest included in live environment & install
│   ├── pacman.conf           # Pacman configuration & optional custom repos
│   ├── airootfs/             # Root filesystem overlay
│   │   ├── etc/
│   │   │   ├── hostname      # Default hostname set to "02_OS"
│   │   │   ├── skel/.config/ # Baked dotfiles & user configs
│   │   │   ├── calamares/    # Calamares graphical installer configuration
│   │   │   └── systemd/system/multi-user.target.wants/
│   │   │       └── NetworkManager.service  # NetworkManager enabled by default
│   └── efiboot/ & grub/      # Bootloader configurations
└── out/                      # Output directory for generated ISOs and checksums
```

---

## ⚙️ Specifications & Included Software

| Category | Component / Software |
| :--- | :--- |
| **Base System** | Arch Linux (Kernel + Systemd + Base Tools) |
| **Window Manager** | Sway (Wayland tile-based WM) + XWayland (`xorg-server`) |
| **Terminal & Editor** | Neovim |
| **Web Browser** | Mozilla Firefox |
| **Audio Stack** | PipeWire + Sound Open Firmware (`sof-firmware`) |
| **Networking** | NetworkManager (auto-enabled) |
| **Development** | `base-devel`, `git` |
| **Installer** | Calamares ready (`airootfs/etc/calamares`) |

---

## 🚀 Building the ISO

You can build the ISO on any Linux host (including Ubuntu/Zorin OS) using Docker.

### Automated Build (Recommended)
Run the automated build script:
```bash
./build.sh
```

### Manual Docker Build
If you prefer running step-by-step commands inside Docker:
```bash
docker run --rm -it --privileged \
  -v $(pwd)/profile:/02_OS \
  -v $(pwd)/out:/out \
  -v /tmp/archiso-tmp:/tmp/archiso-tmp \
  archlinux:latest bash
```

Inside the interactive container shell:
```bash
pacman -Sy --noconfirm archiso
mkarchiso -v -w /tmp/archiso-tmp -o /out /02_OS
```

---

## 🧪 Testing in QEMU Virtual Machine

To test your compiled ISO before writing to physical hardware:

```bash
# Install QEMU on host (Zorin/Ubuntu):
sudo apt update && sudo apt install -y qemu-system-x86

# Run ISO in QEMU:
qemu-system-x86_64 -enable-kvm -m 4G -cdrom out/02_OS-*.iso -boot d
```

---

## ✍️ Signing & Verification

Generate cryptographic signatures and SHA-256 hashes for publishing:

```bash
# Generate detached OpenPGP ASCII signature
gpg --detach-sign --armor out/02_OS-*.iso

# Create SHA256 checksum file
sha256sum out/02_OS-*.iso > out/02_OS.sha256
```

---

## 💾 Writing to USB Flash Drive

> [!WARNING]
> Ensure you identify the correct device node (`/dev/sdX`) using `lsblk` before running `dd`. Overwriting the wrong partition will cause data loss.

1. **List storage devices:**
   ```bash
   lsblk
   ```
2. **Flash ISO to USB drive:**
   ```bash
   sudo dd if=out/02_OS-*.iso of=/dev/sdX bs=4M status=progress oflag=sync
   ```
