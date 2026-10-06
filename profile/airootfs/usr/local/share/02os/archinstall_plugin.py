# archinstall 4.5 (packages.x86_64) loads Plugin() and calls
# on_install(installation) at the end of Installer.minimal_installation,
# before the desktop profile packages exist. on_genfstab(installation) runs
# after that profile, which is when the target schema directory is complete.
__archinstall__version__ = 4.5

# Beyond archinstall's gnome + gnome-tweaks profile. Read from the live image.
INSTALLED_PACKAGES = "/usr/local/share/02os/installed-packages.txt"


def _read_installed_packages(path):
    packages = []
    with open(path, encoding="utf-8") as handle:
        for raw in handle:
            line = raw.split("#", 1)[0].strip()
            if line:
                packages.append(line)
    return packages


class Plugin:
    def on_install(self, installation):
        self._provision(installation)

    def on_genfstab(self, installation):
        self._install_desktop_packages(installation)
        self._provision(installation)

    def _install_desktop_packages(self, installation):
        add = getattr(installation, "add_additional_packages", None)
        # Recording stubs have no archinstall Installer API.
        if add is None:
            return
        packages = _read_installed_packages(INSTALLED_PACKAGES)
        if not packages:
            raise RuntimeError("02_OS installed-system package list is empty")
        add(packages)

    def _provision(self, installation):
        import subprocess

        target = str(installation.target)
        subprocess.check_call(["/usr/local/bin/02os-provision", target])
