"""Memory freshness failures fail closed through the shipped CLI and MCP."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("AGENT_BINARY", ROOT / "components/02-agent/target/debug/02")).resolve()


class MemoryRuntimeRegressionTests(unittest.TestCase):
    def test_changed_evidence_cannot_remain_verified_when_status_persistence_fails(self):
        with tempfile.TemporaryDirectory(prefix="02-memory-runtime-regression-") as directory:
            base = Path(directory)
            repo = base / "repo"
            repo.mkdir()
            env = os.environ.copy()
            for name, suffix in (("HOME", "home"), ("XDG_DATA_HOME", "data"),
                                 ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"),
                                 ("XDG_RUNTIME_DIR", "run")):
                path = base / suffix
                path.mkdir(mode=0o700)
                env[name] = str(path)

            def run(*args, check=True, input=None):
                result = subprocess.run(args, cwd=repo, env=env, input=input,
                                        text=True, capture_output=True, timeout=30)
                if check:
                    self.assertEqual(result.returncode, 0, result.stderr)
                return result

            def tool(name, arguments):
                request = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                           "params": {"name": name, "arguments": arguments}}
                reply = run(str(BINARY), "mcp", input=json.dumps(request) + "\n")
                return json.loads(reply.stdout)["result"]

            run("git", "init", "-q")
            run("git", "config", "user.name", "Memory regression fixture")
            run("git", "config", "user.email", "fixture@example.invalid")
            source = repo / "auth.rs"
            source.write_text("pub fn authenticate() -> bool { true }\n")
            run("git", "add", ".")
            run("git", "commit", "-qm", "fixture")
            run(str(BINARY), "--json", "index")
            claim = "auditprobe_live_baseline accepts requests"
            written = tool("memory_write", {"id": "rule", "claim": claim,
                                           "file": "auth.rs", "symbols": ["authenticate"]})
            self.assertFalse(written["isError"], written)
            memory = json.loads(written["content"][0]["text"])["memory"]
            self.assertTrue(memory["evidence"][0]["fingerprint"].startswith("symbols-v1:"))
            store = next((base / "data").rglob("memories.jsonl"))
            # Check the real human CLI as well as MCP/JSON with a synthetic
            # credential, including a legacy row written before redaction.
            token = "ghp_" + "a" * 36
            written = tool("memory_write", {"id": "rule", "claim": claim + " " + token,
                                           "file": "auth.rs", "symbols": ["authenticate"]})
            self.assertNotIn(token, json.dumps(written))
            self.assertNotIn(token, store.read_text())
            memory = json.loads(written["content"][0]["text"])["memory"]
            memory["claim"] = claim + " " + token
            store.write_text(json.dumps(memory) + "\n")
            human = run(str(BINARY), "context", "auditprobe_live_baseline")
            self.assertNotIn(token, human.stdout)
            self.assertIn("[REDACTED]", human.stdout)
            fetched = tool("memory_get", {"id": "rule"})
            self.assertNotIn(token, json.dumps(fetched))
            # A subsequent normal write also sanitizes the persisted legacy row.
            tool("memory_write", {"id": "rule", "claim": claim,
                                  "file": "auth.rs", "symbols": ["authenticate"]})
            original = store.read_text()
            source.write_text("pub fn authenticate() -> bool { false }\n")
            # Force the status write to fail for any UID while keeping the old
            # Fresh row readable. An ignored error would return it as verified.
            blocked_temp = store.with_suffix(".jsonl.tmp")
            blocked_temp.mkdir()
            failed_cli = run(str(BINARY), "--json", "context", "auditprobe_live_baseline",
                             check=False)
            self.assertNotEqual(failed_cli.returncode, 0)
            self.assertEqual(failed_cli.stdout, "")
            self.assertIn("Error:", failed_cli.stderr)
            failed_mcp = tool("repository_context", {"task": "auditprobe_live_baseline"})
            self.assertTrue(failed_mcp["isError"], failed_mcp)
            self.assertNotIn(claim, json.dumps(failed_mcp))
            self.assertEqual(store.read_text(), original)

            blocked_temp.rmdir()
            recovered = tool("repository_context", {"task": "auditprobe_live_baseline"})
            self.assertFalse(recovered["isError"], recovered)
            package = json.loads(recovered["content"][0]["text"])
            self.assertEqual(package["verified_memories"], [])
            self.assertEqual(package["diagnostic_memories"][0]["id"], "rule")
            self.assertEqual(package["diagnostic_memories"][0]["status"], "Degraded")


if __name__ == "__main__":
    unittest.main()
