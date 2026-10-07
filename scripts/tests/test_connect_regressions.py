"""Real connect CLI preserves TOML semantics as parsed independently by Python."""
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("AGENT_BINARY", ROOT / "components/02-agent/target/debug/02")).resolve()


class ConnectRegressionTests(unittest.TestCase):
    def test_codex_write_preserves_values_with_independent_toml_parser(self):
        with tempfile.TemporaryDirectory(prefix="02-connect-regression-") as directory:
            home = Path(directory) / "home"
            config = home / ".codex/config.toml"
            config.parent.mkdir(parents=True)
            original = r'''
model = "keep-me"
custom_instructions = """Follow \u0061ll checks. \U0001F642\b\f
Trim \
    this continuation."""
quoted = "quote:\" backslash:\\ tab:\t carriage:\r newline:\n"
integers = [0x10, 0o10, 0b10, -1_000]
date = 2026-10-08
inline = { "quoted.key" = { enabled = true, weights = [1.5, 2.25] } }

[[skills.config]]
path = "/tmp/fixture/one/SKILL.md"
enabled = false

[[skills.config]]
path = "/tmp/fixture/two/SKILL.md"
enabled = true

[projects."/tmp/fixture/a.b"]
trust_level = "trusted"

[mcp_servers.other]
command = "keep-other"
args = ["--keep"]

[mcp_servers.02]
command = "old-command"
args = ["old-argument"]
startup_timeout_sec = 30

[mcp_servers.02.env]
FIXTURE_VALUE = "keep-me"
'''
            # A multiline literal string has different escape rules from the
            # multiline basic string, so include both in the actual document.
            original = "literal = '''Keep literal \\u0061 and \"quotes\".'''\n" + original
            expected = tomllib.loads(original)
            self.assertTrue(expected["custom_instructions"].startswith("Follow all checks. 🙂"))
            config.write_text(original)
            os.chmod(config, 0o640)
            with config.open() as old_reader:
                result = subprocess.run([str(BINARY), "connect", "codex", "--write"],
                                        cwd=directory, env={**os.environ, "HOME": str(home)},
                                        text=True, capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Successfully configured", result.stdout)
                actual = tomllib.loads(config.read_text())
                expected["mcp_servers"]["02"].update(command="02", args=["mcp"])
                self.assertEqual(actual, expected)
                self.assertEqual(old_reader.read(), original)
                self.assertEqual(config.stat().st_mode & 0o777, 0o640)
                self.assertEqual(list(config.parent.iterdir()), [config])


if __name__ == "__main__":
    unittest.main()
