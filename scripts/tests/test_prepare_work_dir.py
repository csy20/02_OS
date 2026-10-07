"""prepare_work_dir must reject critical paths and only delete a marked work dir."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/prepare-work-dir.sh"


class PrepareWorkDirTests(unittest.TestCase):
    def setUp(self):
        self._temps = []

    def tearDown(self):
        for path in self._temps:
            shutil.rmtree(path, ignore_errors=True)
            marker = Path(str(path) + ".02os-marker")
            if marker.exists() or marker.is_symlink():
                marker.unlink()

    def make_temp(self, prefix, directory=None):
        path = Path(tempfile.mkdtemp(prefix=prefix, dir=directory))
        self._temps.append(path)
        return path

    def run_prepare(self, work_dir, extra=None, cwd=None):
        env = os.environ.copy()
        for key in ("SCRIPT_DIR", "PROFILE_DIR", "OUT_DIR", "PACMAN_CACHE_DIR", "WORK_DIR"):
            env.pop(key, None)
        env["MIN_FREE_GB"] = "0"
        env["WORK_DIR"] = str(work_dir)
        if extra:
            env.update(extra)
        return subprocess.run(
            ["bash", "-c", 'source "$1" && prepare_work_dir', "bash", str(SCRIPT)],
            cwd=cwd,
            env=env,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def marker(self, work_dir):
        return Path(str(Path(work_dir)) + ".02os-marker")

    def test_allows_fresh_temp_dir(self):
        parent = self.make_temp("02os-wd-")
        work = parent / "work"
        result = self.run_prepare(work)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(work.is_dir())
        self.assertTrue(self.marker(work).is_file())
        self.assertEqual(list(work.iterdir()), [])

    def test_allows_child_of_var_tmp(self):
        parent = self.make_temp("02os-vartmp-", directory="/var/tmp")
        work = parent / "02_OS-build-work"
        result = self.run_prepare(work)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(work.is_dir())
        self.assertNotEqual(work, Path("/var/tmp"))
        self.assertTrue(str(work).startswith("/var/tmp/"))

    def test_relative_path_is_resolved(self):
        parent = self.make_temp("02os-rel-")
        env = os.environ.copy()
        for key in ("SCRIPT_DIR", "PROFILE_DIR", "OUT_DIR", "PACMAN_CACHE_DIR", "WORK_DIR"):
            env.pop(key, None)
        env["MIN_FREE_GB"] = "0"
        env["WORK_DIR"] = "work"
        result = subprocess.run(
            ["bash", "-c", 'cd "$2" && source "$1" && prepare_work_dir', "bash", str(SCRIPT), str(parent)],
            env=env,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((parent / "work").is_dir())
        self.assertTrue((parent / "work.02os-marker").is_file())

    def test_refuses_unmarked_directory(self):
        parent = self.make_temp("02os-unmarked-")
        work = parent / "work"
        work.mkdir()
        sentinel = work / "sentinel"
        sentinel.write_text("keep")
        result = self.run_prepare(work)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sentinel.read_text(), "keep")
        self.assertFalse(self.marker(work).exists())

    def test_deletes_marked_work_dir(self):
        parent = self.make_temp("02os-marked-")
        work = parent / "work"
        first = self.run_prepare(work)
        self.assertEqual(first.returncode, 0, first.stderr)
        sentinel = work / "sentinel"
        sentinel.write_text("gone")
        second = self.run_prepare(work)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertFalse(sentinel.exists())
        self.assertTrue(work.is_dir())
        self.assertTrue(self.marker(work).is_file())

    def test_reclaims_read_only_marked_directories_without_following_symlinks(self):
        parent = self.make_temp("02os-readonly-")
        work, outside = parent / "work", parent / "outside"
        self.assertEqual(self.run_prepare(work).returncode, 0)
        nested = work / "airootfs"
        nested.mkdir()
        (nested / "sentinel").write_text("remove")
        outside.mkdir()
        (outside / "sentinel").write_text("keep")
        (nested / "outside-link").symlink_to(outside, target_is_directory=True)
        nested.chmod(0o555)
        outside.chmod(0o555)
        fake_bin = parent / "bin"
        fake_bin.mkdir()
        docker = fake_bin / "docker"
        docker.write_text("#!/bin/sh\nexit 73\n")
        docker.chmod(0o755)
        try:
            result = self.run_prepare(work, {"PATH": str(fake_bin) + ":" + os.environ["PATH"]})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(list(work.iterdir()), [])
            self.assertEqual((outside / "sentinel").read_text(), "keep")
            self.assertEqual(outside.stat().st_mode & 0o777, 0o555)
        finally:
            outside.chmod(0o755)
            if nested.exists():
                nested.chmod(0o755)

    def test_df_failure_does_not_create_work_dir(self):
        parent = self.make_temp("02os-df-")
        work = parent / "work"
        result = self.run_prepare(work, extra={"MIN_FREE_GB": "1000000"})
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertFalse(work.exists())
        self.assertFalse(self.marker(work).exists())

    def test_missing_parent_is_not_created(self):
        parent = self.make_temp("02os-parent-")
        work = parent / "missing" / "work"
        result = self.run_prepare(work)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertFalse((parent / "missing").exists())

    def test_rejects_root(self):
        marker = Path("/.02os-marker")
        existed = marker.exists()
        result = self.run_prepare("/")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(marker.exists(), existed)
        self.assertTrue(Path("/").is_dir())

    def test_rejects_home(self):
        home = Path(os.environ["HOME"])
        marker = Path(str(home) + ".02os-marker")
        existed = marker.exists()
        result = self.run_prepare(home)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(marker.exists(), existed)
        self.assertTrue(home.is_dir())

    def test_rejects_repo(self):
        marker = Path(str(ROOT) + ".02os-marker")
        existed = marker.exists()
        result = self.run_prepare(ROOT)
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("overlaps", result.stderr)
        self.assertTrue((ROOT / "build.sh").is_file())
        self.assertEqual(marker.exists(), existed)

    def test_rejects_work_dir_that_contains_profile(self):
        parent = self.make_temp("02os-contain-")
        outside = self.make_temp("02os-outside-")
        profile = parent / "profile"
        profile.mkdir()
        sentinel = profile / "sentinel"
        sentinel.write_text("keep")
        (outside / "home").mkdir()
        result = self.run_prepare(parent, extra={
            "SCRIPT_DIR": str(outside / "repo"),
            "PROFILE_DIR": str(profile),
            "OUT_DIR": str(outside / "out"),
            "PACMAN_CACHE_DIR": str(outside / "cache"),
            "HOME": str(outside / "home"),
        })
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertIn("overlaps", result.stderr)
        self.assertEqual(sentinel.read_text(), "keep")
        self.assertFalse(self.marker(parent).exists())


if __name__ == "__main__":
    unittest.main()
