"""Staging and verify follow cargo's target directory, not a stale default."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "scripts/build-agent-runtime.sh"
VERIFY = ROOT / "scripts/verify-staged-runtime.sh"

STALE_02 = b"stale-source-build-02\n"
STALE_DAEMON = b"stale-source-build-daemon\n"
FRESH_02 = b"fresh-cli-02\n"
FRESH_DAEMON = b"fresh-daemon\n"

FAKE_CARGO = r'''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path

log = Path(os.environ["FAKE_CARGO_LOG"])
argv = sys.argv[1:]
target = os.environ.get("CARGO_TARGET_DIR", "")
if "--target-dir" in argv:
    target = argv[argv.index("--target-dir") + 1]
log.parent.mkdir(parents=True, exist_ok=True)
with log.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps({"argv": argv, "target": target}) + "\n")
if not argv:
    sys.stderr.write("missing cargo subcommand\n")
    sys.exit(1)
cmd = argv[0]
if cmd == "metadata":
    if not target:
        sys.stderr.write("metadata without a target directory\n")
        sys.exit(1)
    json.dump({"target_directory": os.path.abspath(target)}, sys.stdout)
    sys.stdout.write("\n")
    sys.exit(0)
if cmd in ("build", "test"):
    if not target:
        sys.stderr.write("build without a target directory\n")
        sys.exit(1)
    dest = Path(target) / "release"
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "02").write_bytes(b"fresh-cli-02\n")
    (dest / "02-agentd").write_bytes(b"fresh-daemon\n")
    sys.exit(0)
sys.stderr.write("unexpected cargo command: %s\n" % argv)
sys.exit(1)
'''


class RuntimeStagingTargetDirTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory(prefix="02os-cargo-target-")
        self.root = Path(self._tmp.name)
        scripts = self.root / "scripts"
        scripts.mkdir()
        shutil.copy2(BUILD, scripts / "build-agent-runtime.sh")
        agent = self.root / "components/02-agent"
        agent.mkdir(parents=True)
        # Present so verify resolves metadata. Not a real crate: the fake cargo
        # never reads it, and a mistaken real cargo fails instead of building.
        (agent / "Cargo.toml").write_text("not a valid manifest\n")
        self.bin_dir = self.root / "bin"
        self.bin_dir.mkdir()
        cargo = self.bin_dir / "cargo"
        cargo.write_text(FAKE_CARGO)
        cargo.chmod(0o755)
        self.log = self.root / "cargo-log.jsonl"
        self.configured = self.root / "configured-target"
        self.env = os.environ.copy()
        self.env["PATH"] = str(self.bin_dir) + os.pathsep + self.env.get("PATH", "")
        self.env["CARGO_TARGET_DIR"] = str(self.configured)
        self.env["FAKE_CARGO_LOG"] = str(self.log)

    def tearDown(self):
        self._tmp.cleanup()

    def stage(self):
        return subprocess.run(
            ["bash", str(self.root / "scripts/build-agent-runtime.sh")],
            cwd=self.root,
            env=self.env,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def verify(self):
        return subprocess.run(
            ["bash", str(VERIFY), "--repo", str(self.root)],
            cwd=self.root,
            env=self.env,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def entries(self):
        self.assertTrue(self.log.is_file(), "fake cargo was not invoked")
        return [json.loads(line) for line in self.log.read_text().splitlines() if line.strip()]

    def assert_recorded_target(self):
        commands = []
        for entry in self.entries():
            commands.append(entry["argv"][0])
            self.assertEqual(Path(entry["target"]).resolve(), self.configured.resolve(), entry)
        self.assertIn("metadata", commands)
        self.assertIn("build", commands)
        self.assertIn("test", commands)
        self.assertLess(commands.index("metadata"), commands.index("build"))

    def test_staged_binary_is_fresh_not_the_stale_default(self):
        default_release = self.root / "components/02-agent/target/release"
        default_release.mkdir(parents=True)
        (default_release / "02").write_bytes(STALE_02)
        (default_release / "02-agentd").write_bytes(STALE_DAEMON)

        staged = self.stage()
        self.assertEqual(staged.returncode, 0, staged.stderr + staged.stdout)
        self.assert_recorded_target()
        self.assertEqual((default_release / "02").read_bytes(), STALE_02)
        self.assertEqual((default_release / "02-agentd").read_bytes(), STALE_DAEMON)
        self.assertEqual((self.configured / "release/02").read_bytes(), FRESH_02)
        self.assertEqual((self.configured / "release/02-agentd").read_bytes(), FRESH_DAEMON)

        dest = self.root / "profile/airootfs/usr/bin"
        staged_02 = (dest / "02").read_bytes()
        staged_daemon = (dest / "02-agentd").read_bytes()
        self.assertNotEqual(staged_02, STALE_02)
        self.assertNotEqual(staged_daemon, STALE_DAEMON)
        self.assertTrue(staged_02 == FRESH_02 or staged_02.startswith(b"fresh-cli-02"))
        self.assertTrue((dest / "02agent").is_symlink())
        self.assertEqual((dest / "02agent").readlink(), Path("02"))

        checked = self.verify()
        self.assertEqual(checked.returncode, 0, checked.stderr + checked.stdout)

    def test_missing_default_target_dir_does_not_fail(self):
        default_target = self.root / "components/02-agent/target"
        self.assertFalse(default_target.exists())
        staged = self.stage()
        self.assertEqual(staged.returncode, 0, staged.stderr + staged.stdout)
        self.assert_recorded_target()
        self.assertFalse(default_target.exists())
        self.assertEqual((self.configured / "release/02").read_bytes(), FRESH_02)
        dest = self.root / "profile/airootfs/usr/bin/02"
        self.assertTrue(dest.is_file())
        self.assertNotEqual(dest.read_bytes(), STALE_02)
        checked = self.verify()
        self.assertEqual(checked.returncode, 0, checked.stderr + checked.stdout)
        self.assertFalse(default_target.exists())


if __name__ == "__main__":
    unittest.main()
