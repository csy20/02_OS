# archinstall 4.5 (packages.x86_64) loads Plugin() and calls
# on_install(installation) at the end of Installer.minimal_installation,
# before bootloader generation and desktop profile installation. Set kernel
# flags at that point so every generated bootloader/UKI retains them.
# on_genfstab runs after the desktop profile and after add_bootloader.
# Select our theme again in case the installer selected another Plymouth
# theme, then rename boot-menu titles that still name another OS.
__archinstall__version__ = 4.5
PLYMOUTH_THEME = "02-zero-portal"
OS_TITLE = "02_OS"


class Plugin:
    def on_install(self, installation):
        self._provision(installation)
        self._prepare_boot(installation)
        installation.add_additional_packages(["plymouth"])
        self._activate_theme(installation)

    def on_genfstab(self, installation):
        self._provision(installation)
        self._activate_theme(installation)
        self._rename_os_titles(installation)

    def _prepare_boot(self, installation):
        # archinstall's own Plymouth integration uses these installer-owned
        # lists. Keep the installed target's autodetect, keyboard, encryption,
        # LVM, and resume hooks; never copy the live archiso hook configuration.
        hooks = getattr(installation, "_hooks", None)
        parameters = getattr(installation, "_kernel_params", None)
        if not isinstance(hooks, list) or not isinstance(parameters, list):
            raise RuntimeError("Unsupported archinstall boot configuration")
        backend = "systemd" if "systemd" in hooks else "udev"
        if backend not in hooks:
            raise RuntimeError("Plymouth requires a systemd or udev initramfs")

        hooks = [hook for hook in hooks if hook != "plymouth"]
        insert_at = hooks.index(backend) + 1
        if "kms" in hooks:
            insert_at = max(insert_at, hooks.index("kms") + 1)
        for encryption in ("encrypt", "sd-encrypt"):
            if encryption in hooks and hooks.index(encryption) < insert_at:
                raise RuntimeError("Unsupported initramfs encryption hook ordering")
        hooks.insert(insert_at, "plymouth")
        installation._hooks = hooks
        for parameter in ("quiet", "splash"):
            if parameter not in parameters:
                parameters.append(parameter)

    def _activate_theme(self, installation):
        installation.arch_chroot(f"plymouth-set-default-theme {PLYMOUTH_THEME}")
        if not installation.mkinitcpio(["-P"]):
            raise RuntimeError("Could not generate the installed Plymouth initramfs")

    def _rename_os_titles(self, installation):
        from pathlib import Path

        root = Path(str(installation.target))
        seen = set()
        for base in (root / "boot", root / "efi"):
            if not base.is_dir():
                continue
            entries = base / "loader" / "entries"
            candidates = []
            if entries.is_dir():
                candidates.extend(entries.glob("*.conf"))
            candidates.extend(base.rglob("limine.conf"))
            candidates.append(base / "grub" / "grub.cfg")
            candidates.append(base / "refind_linux.conf")
            for path in candidates:
                if path in seen or not path.is_file() or path.is_symlink():
                    continue
                seen.add(path)
                original = path.read_text()
                updated = retitle_boot_text(original)
                if updated != original:
                    path.write_text(updated)

    def _provision(self, installation):
        import subprocess

        target = str(installation.target)
        subprocess.check_call(["/usr/local/bin/02os-provision", target])


def retitle_boot_text(text):
    """Replace an Arch Linux product title. Leave paths, options, and comments."""
    lines = []
    for line in text.splitlines(keepends=True):
        body = line.lstrip(" \t")
        title = body.startswith("title ") or body.startswith("title\t")
        menu = body.startswith("menuentry ") or body.startswith("submenu ")
        if (title or menu or body.startswith("/Arch Linux") or body.startswith('"Arch Linux')) and "Arch Linux" in line:
            line = line.replace("Arch Linux", OS_TITLE, 1)
        lines.append(line)
    return "".join(lines)
