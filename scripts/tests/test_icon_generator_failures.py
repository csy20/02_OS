"""Required icon theme writes must fail the process instead of printing success."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "scripts/icon-generator"
COMPLETE_ALL = "Theme generation complete for all target paths"


def copy_generator(root):
    dest = root / "scripts/icon-generator"
    shutil.copytree(GENERATOR, dest)
    return dest


class IconGeneratorFailureTests(unittest.TestCase):
    def test_regular_file_output_root_is_a_failed_required_write(self):
        with tempfile.TemporaryDirectory(prefix="02os-icons-fail-") as tmp:
            root = Path(tmp)
            scripts = copy_generator(root)
            # The generator's profile directory cannot be created under a file.
            (root / "profile").write_text("not a directory\n")
            for entry in ("generate_full_icon_pack.py", "build_and_install_theme.py"):
                with self.subTest(entry=entry):
                    result = subprocess.run(
                        [sys.executable, str(scripts / entry)],
                        cwd=root,
                        capture_output=True,
                        text=True,
                        timeout=60,
                    )
                    combined = result.stdout + result.stderr
                    self.assertNotEqual(result.returncode, 0, combined)
                    self.assertIn("Error generating", combined)
                    self.assertIn("Not a directory", combined)
                    self.assertNotIn(COMPLETE_ALL, combined)
                    self.assertNotIn("Theme generation complete for", combined)
                    self.assertFalse((root / "profile/airootfs").exists())

    def test_missing_icon_cache_tool_does_not_fail_a_written_theme(self):
        with tempfile.TemporaryDirectory(prefix="02os-icons-ok-") as tmp:
            root = Path(tmp)
            scripts = copy_generator(root)
            empty_path = root / "empty-path"
            empty_path.mkdir()
            env = os.environ.copy()
            env["PATH"] = str(empty_path)
            result = subprocess.run(
                [sys.executable, str(scripts / "build_and_install_theme.py")],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
                timeout=120,
            )
            combined = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, combined)
            theme = root / "profile/airootfs/usr/share/icons/02-OS"
            self.assertTrue((theme / "index.theme").is_file())
            self.assertTrue(any(theme.rglob("*.svg")))
            self.assertIn(f"Theme generation complete for {theme}", result.stdout)
            self.assertNotIn(COMPLETE_ALL, combined)
            self.assertIn("gtk-update-icon-cache", combined)


if __name__ == "__main__":
    unittest.main()
