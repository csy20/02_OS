"""Installed Plymouth configuration without installing packages or an OS."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock


ROOT = Path(__file__).resolve().parents[2]
PLUGIN_PATH = ROOT / "profile/airootfs/usr/local/share/02os/archinstall_plugin.py"


class InstalledBootTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location("02os_archinstall_test", PLUGIN_PATH)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.plugin = module.Plugin()

    def installation(self, hooks, parameters=None):
        return SimpleNamespace(
            target="/mnt/disposable-target",
            _hooks=list(hooks),
            _kernel_params=list(parameters or []),
            add_additional_packages=Mock(),
            arch_chroot=Mock(),
            mkinitcpio=Mock(return_value=True),
        )

    def test_keeps_installed_encryption_lvm_resume_and_kernel_parameters(self):
        cases = [
            ["base", "udev", "autodetect", "microcode", "kms", "keyboard", "keymap consolefont", "block", "encrypt", "lvm2", "resume", "filesystems", "fsck"],
            ["base", "systemd", "autodetect", "microcode", "kms", "keyboard", "sd-vconsole", "block", "sd-encrypt", "filesystems", "fsck"],
        ]
        for hooks in cases:
            with self.subTest(backend=hooks[1]):
                parameters = ["root=UUID=target", "cryptdevice=UUID=encrypted:root", "resume=UUID=swap", "quiet"]
                target = self.installation(hooks, parameters)
                self.plugin._prepare_boot(target)
                self.plugin._prepare_boot(target)
                self.assertEqual([hook for hook in target._hooks if hook != "plymouth"], hooks)
                self.assertEqual(target._hooks.count("plymouth"), 1)
                self.assertEqual(target._hooks.index("plymouth"), target._hooks.index("kms") + 1)
                encryption = "encrypt" if "encrypt" in hooks else "sd-encrypt"
                self.assertLess(target._hooks.index("plymouth"), target._hooks.index(encryption))
                self.assertEqual(target._kernel_params, parameters + ["splash"])
                self.assertNotIn("archiso", target._hooks)

    def test_early_setup_precedes_image_generation_and_final_phase_reselects_theme(self):
        target = self.installation(["base", "udev", "kms", "filesystems"], ["root=UUID=target"])
        events = []
        self.plugin._provision = Mock(side_effect=lambda _target: events.append("copy-theme"))
        target.add_additional_packages.side_effect = lambda _packages: events.append("install-package")
        target.arch_chroot.side_effect = lambda _command: events.append("select-theme")

        def generate(flags):
            self.assertEqual(flags, ["-P"])
            self.assertIn("splash", target._kernel_params)
            self.assertIn("plymouth", target._hooks)
            events.append("regenerate-image")
            return True

        target.mkinitcpio.side_effect = generate
        self.plugin.on_install(target)
        # archinstall generates the bootloader after on_install returns.
        self.assertEqual(events, ["copy-theme", "install-package", "select-theme", "regenerate-image"])
        target.add_additional_packages.assert_called_once_with(["plymouth"])
        self.plugin.on_genfstab(target)
        self.assertEqual(events[-3:], ["copy-theme", "select-theme", "regenerate-image"])
        self.assertEqual(target.arch_chroot.call_args.args, ("plymouth-set-default-theme 02-zero-portal",))

    def test_repositions_existing_plymouth_before_encryption(self):
        target = self.installation(["base", "udev", "kms", "encrypt", "plymouth", "filesystems"])
        self.plugin._prepare_boot(target)
        self.assertEqual(target._hooks, ["base", "udev", "kms", "plymouth", "encrypt", "filesystems"])

    def test_rejects_unsupported_target_without_mutating_it(self):
        for hooks in (["base", "kms", "filesystems"], ["base", "udev", "encrypt", "kms", "filesystems"]):
            with self.subTest(hooks=hooks):
                target = self.installation(hooks, ["root=UUID=target"])
                with self.assertRaisesRegex(RuntimeError, "(requires|ordering)"):
                    self.plugin._prepare_boot(target)
                self.assertEqual(target._hooks, hooks)
                self.assertEqual(target._kernel_params, ["root=UUID=target"])

    def test_initramfs_failure_aborts_provisioning(self):
        target = self.installation(["base", "udev", "kms", "filesystems"])
        self.plugin._provision = Mock()
        target.mkinitcpio.return_value = False
        with self.assertRaisesRegex(RuntimeError, "initramfs"):
            self.plugin.on_install(target)


if __name__ == "__main__":
    unittest.main()
