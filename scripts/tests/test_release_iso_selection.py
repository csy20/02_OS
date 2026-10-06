"""Fresh releases pick the ISO this run wrote; --reuse-iso stays strict."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
RELEASE = ROOT / "scripts/release.sh"


def git_head():
    return subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"],
        text=True,
        timeout=10,
    ).strip()


class ReleaseIsoSelectionTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory(prefix="02os-release-")
        self.root = Path(self._tmp.name)
        self.out = self.root / "out"
        self.out.mkdir()
        self.head = git_head()

    def tearDown(self):
        self._tmp.cleanup()

    def write_stub(self, body):
        stub = self.root / "build.sh"
        stub.write_text("#!/bin/bash\nset -euo pipefail\n" + body)
        stub.chmod(0o755)
        return stub

    def run_release(self, stub, *args):
        env = os.environ.copy()
        env["RELEASE_DRY_RUN"] = "1"
        env["RELEASE_OUT_DIR"] = str(self.out)
        env["RELEASE_BUILD_SCRIPT"] = str(stub)
        return subprocess.run(
            ["bash", str(RELEASE), *args],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_fresh_build_selects_new_iso_when_an_older_one_exists(self):
        old = self.out / "02_OS-2099.10.05-x86_64.iso"
        old.write_bytes(b"previous-image")
        (self.out / (old.name + ".commit")).write_text("not-this-commit\n")
        stub = self.write_stub(
            'printf "new-image\\n" > "${OUT_DIR}/02_OS-2099.10.06-x86_64.iso"\n'
        )
        result = self.run_release(stub)
        combined = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, combined)
        self.assertNotIn("found 2", combined)
        self.assertIn("Selected ISO:", result.stdout)
        selected = next(line for line in result.stdout.splitlines() if line.startswith("Selected ISO:"))
        self.assertIn("02_OS-2099.10.06-x86_64.iso", selected)
        self.assertNotIn("02_OS-2099.10.05-x86_64.iso", selected)
        self.assertIn("skipping split, tag, and GitHub publish", result.stdout)
        self.assertNotIn("Publishing GitHub", result.stdout)
        new = self.out / "02_OS-2099.10.06-x86_64.iso"
        self.assertEqual(new.read_text(), "new-image\n")
        self.assertEqual((self.out / (new.name + ".commit")).read_text().strip(), self.head)
        self.assertTrue(old.is_file(), "older ISO must be left in place")

    def test_fresh_build_selects_new_iso_among_two_older_ones(self):
        for name in ("02_OS-2099.10.04-x86_64.iso", "02_OS-2099.10.05-x86_64.iso"):
            (self.out / name).write_bytes(b"old")
        stub = self.write_stub(
            'printf "newest\\n" > "${OUT_DIR}/02_OS-2099.10.06-x86_64.iso"\n'
        )
        result = self.run_release(stub)
        combined = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, combined)
        self.assertNotIn("found 2", combined)
        self.assertNotIn("found 3", combined)
        self.assertIn("02_OS-2099.10.06-x86_64.iso", result.stdout)
        selected = next(line for line in result.stdout.splitlines() if line.startswith("Selected ISO:"))
        self.assertNotIn("2099.10.04", selected)
        self.assertNotIn("2099.10.05", selected)

    def test_same_name_rewrite_requires_head_stamp(self):
        iso = self.out / "02_OS-2099.10.06-x86_64.iso"
        iso.write_bytes(b"morning-build")
        os.utime(iso, (1_000_000_000, 1_000_000_000))
        (self.out / (iso.name + ".commit")).write_text("stale-stamp\n")
        stub = self.write_stub(
            'printf "evening-build\\n" > "${OUT_DIR}/02_OS-2099.10.06-x86_64.iso"\n'
            f'printf "%s\\n" "{self.head}" > "${{OUT_DIR}}/02_OS-2099.10.06-x86_64.iso.commit"\n'
        )
        result = self.run_release(stub)
        combined = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, combined)
        selected = next(line for line in result.stdout.splitlines() if line.startswith("Selected ISO:"))
        self.assertIn("02_OS-2099.10.06-x86_64.iso", selected)
        self.assertEqual(iso.read_bytes(), b"evening-build\n")

    def test_same_name_rewrite_with_wrong_stamp_is_rejected(self):
        iso = self.out / "02_OS-2099.10.06-x86_64.iso"
        iso.write_bytes(b"morning-build")
        os.utime(iso, (1_000_000_000, 1_000_000_000))
        (self.out / (iso.name + ".commit")).write_text(self.head + "\n")
        stub = self.write_stub(
            'printf "evening-build\\n" > "${OUT_DIR}/02_OS-2099.10.06-x86_64.iso"\n'
            'printf "not-head\\n" > "${OUT_DIR}/02_OS-2099.10.06-x86_64.iso.commit"\n'
        )
        result = self.run_release(stub)
        combined = result.stdout + result.stderr
        self.assertNotEqual(result.returncode, 0, combined)
        self.assertIn("does not match HEAD", result.stderr)
        self.assertNotIn("Selected ISO:", result.stdout)
        self.assertNotIn("skipping split", result.stdout)

    def test_untouched_iso_is_not_selected_even_if_stamp_matches_head(self):
        iso = self.out / "02_OS-2099.10.05-x86_64.iso"
        iso.write_bytes(b"untouched")
        os.utime(iso, (1_000_000_000, 1_000_000_000))
        (self.out / (iso.name + ".commit")).write_text(self.head + "\n")
        stub = self.write_stub("exit 0\n")
        result = self.run_release(stub)
        combined = result.stdout + result.stderr
        self.assertNotEqual(result.returncode, 0, combined)
        self.assertIn("could not identify", result.stderr)
        self.assertNotIn("Selected ISO:", result.stdout)
        self.assertEqual(iso.read_bytes(), b"untouched")

    def test_reuse_iso_fails_when_two_isos_exist(self):
        first = self.out / "02_OS-2099.10.05-x86_64.iso"
        second = self.out / "02_OS-2099.10.06-x86_64.iso"
        first.write_bytes(b"one")
        second.write_bytes(b"two")
        (self.out / (second.name + ".commit")).write_text(self.head + "\n")
        stub = self.write_stub('echo "build should not run" >&2\nexit 99\n')
        result = self.run_release(stub, "--reuse-iso")
        combined = result.stdout + result.stderr
        self.assertNotEqual(result.returncode, 0, combined)
        self.assertIn("exactly one", result.stderr)
        self.assertIn("found 2", result.stderr)
        self.assertNotIn("Selected ISO:", result.stdout)
        self.assertNotIn("build should not run", combined)

    def test_reuse_iso_still_requires_head_stamp(self):
        iso = self.out / "02_OS-2099.10.06-x86_64.iso"
        iso.write_bytes(b"only")
        (self.out / (iso.name + ".commit")).write_text("other-commit\n")
        stub = self.write_stub("exit 99\n")
        bad = self.run_release(stub, "--reuse-iso")
        self.assertNotEqual(bad.returncode, 0, bad.stderr)
        self.assertIn("does not match HEAD", bad.stderr)
        self.assertNotIn("Selected ISO:", bad.stdout)

        (self.out / (iso.name + ".commit")).write_text(self.head + "\n")
        good = self.run_release(stub, "--reuse-iso")
        combined = good.stdout + good.stderr
        self.assertEqual(good.returncode, 0, combined)
        self.assertIn("Selected ISO:", good.stdout)
        self.assertIn(iso.name, good.stdout)
        self.assertIn("Reusing ISO", good.stdout)
        self.assertNotIn("build should not run", combined)


if __name__ == "__main__":
    unittest.main()
