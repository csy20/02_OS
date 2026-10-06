"""02-agentd sandbox paths are created before ProtectSystem=strict."""

import os
import stat
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
UNIT = ROOT / "profile/airootfs/usr/lib/systemd/user/02-agentd.service"
PREPARE = ROOT / "profile/airootfs/usr/lib/02-agent/prepare-storage.sh"
GENERATOR = ROOT / "profile/airootfs/usr/lib/systemd/user-generators/02-agent-storage"

READ_WRITE = (
    "ReadWritePaths=%h/.local/share/02-agent "
    "%h/.cache/02-agent %h/.config/02-agent %t %h"
)


def _mode(path: Path) -> int:
    return stat.S_IMODE(path.lstat().st_mode)


def _assert_private_dir(test: unittest.TestCase, path: Path) -> None:
    test.assertTrue(path.is_dir(), path)
    test.assertFalse(path.is_symlink(), path)
    test.assertEqual(_mode(path), 0o700, path)


class AgentdSandboxTests(unittest.TestCase):
    def test_unit_prepares_storage_without_optional_path_prefix(self):
        text = UNIT.read_text()
        self.assertIn("ProtectSystem=strict", text)
        self.assertIn("ProtectHome=no", text)
        self.assertIn("ExecStartPre=+/usr/lib/02-agent/prepare-storage.sh", text)
        self.assertIn(READ_WRITE, text)
        self.assertIn("%h/.config/02-agent", text)
        self.assertNotIn("-%h/.local/share/02-agent", text)
        self.assertNotIn("-%h/.cache/02-agent", text)
        self.assertNotIn("-%h/.config/02-agent", text)
        self.assertNotIn("- %h/", text)
        analyze = Path("/usr/bin/systemd-analyze")
        if analyze.is_file():
            result = subprocess.run(
                [str(analyze), "--user", "verify", str(UNIT)],
                capture_output=True,
                text=True,
                timeout=30,
            )
            combined = result.stdout + result.stderr
            self.assertNotIn("Failed to parse", combined, combined)

    def test_prepare_storage_default_home(self):
        with self._isolated_env() as (home, env):
            self._run_prepare(env)
            for rel in (
                ".local/share/02-agent",
                ".cache/02-agent",
                ".config/02-agent",
            ):
                _assert_private_dir(self, home / rel)

    def test_prepare_storage_custom_xdg_and_symlink(self):
        with self._isolated_env() as (home, env):
            base = home.parent
            data = base / "xdg-data"
            cache = base / "xdg-cache"
            config = base / "xdg-config"
            outside = base / "outside-target"
            outside.mkdir()
            os.chmod(outside, 0o755)
            data.mkdir()
            (data / "02-agent").symlink_to(outside, target_is_directory=True)
            env["XDG_DATA_HOME"] = str(data)
            env["XDG_CACHE_HOME"] = str(cache)
            env["XDG_CONFIG_HOME"] = str(config)
            self._run_prepare(env)
            created = data / "02-agent"
            self.assertFalse(created.is_symlink(), created)
            _assert_private_dir(self, created)
            _assert_private_dir(self, cache / "02-agent")
            _assert_private_dir(self, config / "02-agent")
            self.assertEqual(_mode(outside), 0o755)
            self.assertTrue((home / ".config/02-agent").is_dir())

    def test_generator_writes_dropin_and_does_not_create_storage(self):
        with self._isolated_env() as (home, env):
            base = home.parent
            out = base / "gen-out"
            out.mkdir()
            data = base / "gen-data"
            cache = base / "gen-cache"
            config = base / "gen-config"
            env["XDG_DATA_HOME"] = str(data)
            env["XDG_CACHE_HOME"] = str(cache)
            env["XDG_CONFIG_HOME"] = str(config)
            result = subprocess.run(
                [str(GENERATOR), str(out)],
                capture_output=True,
                text=True,
                env=env,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            dropin = (out / "02-agentd.service.d" / "10-xdg-storage.conf").read_text()
            self.assertIn(f"ReadWritePaths={data}/02-agent", dropin)
            self.assertIn(f"ReadWritePaths={cache}/02-agent", dropin)
            self.assertIn(f"ReadWritePaths={config}/02-agent", dropin)
            self.assertIn("ReadWritePaths=%t", dropin)
            self.assertFalse(data.exists())
            self.assertFalse(cache.exists())
            self.assertFalse(config.exists())

    def _isolated_env(self):
        import tempfile

        return _TempHome(tempfile.TemporaryDirectory(prefix="02-agentd-sandbox-"))

    def _run_prepare(self, env):
        result = subprocess.run(
            [str(PREPARE)],
            capture_output=True,
            text=True,
            env=env,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)


class _TempHome:
    def __init__(self, temp):
        self.temp = temp

    def __enter__(self):
        root = Path(self.temp.name)
        home = root / "home"
        home.mkdir()
        env = os.environ.copy()
        env["HOME"] = str(home)
        for key in ("XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_CONFIG_HOME"):
            env.pop(key, None)
        return home, env

    def __exit__(self, exc_type, exc, tb):
        self.temp.cleanup()
        return False


if __name__ == "__main__":
    unittest.main()
