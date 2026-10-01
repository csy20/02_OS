# archinstall 4.5 (packages.x86_64) loads Plugin() and calls
# on_install(installation) at the end of Installer.minimal_installation,
# before the desktop profile packages exist. on_genfstab(installation) runs
# after that profile, which is when the target schema directory is complete.
__archinstall__version__ = 4.5


class Plugin:
    def on_install(self, installation):
        self._provision(installation)

    def on_genfstab(self, installation):
        self._provision(installation)

    def _provision(self, installation):
        import subprocess

        target = str(installation.target)
        subprocess.check_call(["/usr/local/bin/02os-provision", target])
