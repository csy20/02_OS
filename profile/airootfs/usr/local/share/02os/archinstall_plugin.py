__archinstall__version__ = 3.0

class Plugin:
    def on_install(self, installation):
        import subprocess
        target = str(installation.target)
        subprocess.check_call(["/usr/local/bin/02os-provision", target])
