"""Daemon watch-state errors and startup reconciliation, through real processes."""
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("AGENT_BINARY", ROOT / "components/02-agent/target/debug/02")).resolve()
DAEMON_BINARY = Path(os.environ.get("AGENT_DAEMON_BINARY", BINARY.with_name("02-agentd"))).resolve()


class DaemonRegressionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="02-daemon-regression-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"
        self.repo.mkdir()
        self.env = os.environ.copy()
        for name, directory in (("HOME", "home"), ("XDG_DATA_HOME", "data"),
                                ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"),
                                ("XDG_RUNTIME_DIR", "run")):
            path = self.base / directory
            path.mkdir(mode=0o700)
            self.env[name] = str(path)
        self.state = self.base / "data/02-agent/watched_repos.json"
        self.state.parent.mkdir(mode=0o700)
        self.git("init", "-q")
        self.git("config", "user.name", "Daemon regression fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        (self.repo / ".gitignore").write_text(".02agent/\n")
        (self.repo / "main.rs").write_text("pub fn committed_definition() {}\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.repo, env=self.env,
                              check=True, text=True, capture_output=True, timeout=30).stdout

    def cli(self, *args):
        result = subprocess.run([str(BINARY), "--json", *args], cwd=self.repo,
                                env=self.env, text=True, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def start_daemon(self):
        socket_path = self.base / "run/daemon.sock"
        log_path = self.base / "daemon.stderr"
        log = log_path.open("w")
        self.addCleanup(log.close)
        process = subprocess.Popen([str(DAEMON_BINARY), "--socket", str(socket_path), "--interval", "1"],
                                   cwd=self.base / "home", env=self.env,
                                   stdout=subprocess.DEVNULL, stderr=log)

        def stop():
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        self.addCleanup(stop)
        self.wait_until(lambda: socket_path.exists() or process.poll() is not None)
        self.assertIsNone(process.poll(), log_path.read_text())
        client = socket.socket(socket.AF_UNIX)
        client.settimeout(5)
        client.connect(str(socket_path))
        self.addCleanup(client.close)
        reader = client.makefile("r")
        self.addCleanup(reader.close)

        def request(method, params=None, notification=False):
            value = {"jsonrpc": "2.0", "method": method, "params": params or {}}
            if not notification:
                value["id"] = "fixture-request"
            client.sendall((json.dumps(value) + "\n").encode())
            if notification:
                return None
            response = json.loads(reader.readline())
            self.assertEqual(response["id"], "fixture-request")
            return response
        # Read a complete reply to establish that the socket handler is ready.
        self.assertEqual(request("ping")["result"], {})
        return process, request, log_path, stop

    def wait_until(self, predicate, timeout=6):
        deadline = time.monotonic() + timeout
        while not predicate():
            self.assertLess(time.monotonic(), deadline, "daemon did not reach expected state")
            time.sleep(0.05)

    def assert_server_error(self, response):
        self.assertNotIn("result", response)
        self.assertEqual(response["error"]["code"], -32000)
        self.assertTrue(response["error"]["message"])

    def test_watch_errors_are_returned_and_notifications_are_silent(self):
        corrupt = '{"repositories":['
        self.state.write_text(corrupt)
        _, request, log_path, _ = self.start_daemon()
        for method in ("daemon/watch", "daemon/unwatch"):
            self.assert_server_error(request(method, {"path": str(self.repo)}))
            request(method, {"path": str(self.repo)}, notification=True)
        # The next response must belong to ping, with no notification response
        # lines preceding it. Failures must still be recorded locally.
        self.assertEqual(request("ping")["result"], {})
        self.assertEqual(self.state.read_text(), corrupt)
        log = log_path.read_text()
        self.assertIn("daemon/watch", log)
        self.assertIn("daemon/unwatch", log)
        self.assertIn("malformed", log)

        self.state.unlink()
        self.state.mkdir()
        for method in ("daemon/watch", "daemon/unwatch"):
            self.assert_server_error(request(method, {"path": str(self.repo)}))
        self.assertTrue(self.state.is_dir())

    def test_watch_write_failures_are_not_reported_as_no_ops(self):
        self.state.write_text('{"repositories": []}')
        process, request, _, _ = self.start_daemon()
        for sequence, (method, repositories) in enumerate((
                ("daemon/watch", []), ("daemon/unwatch", [str(self.repo)]))):
            original = json.dumps({"repositories": repositories})
            self.state.write_text(original)
            # Force File::create to fail deterministically without depending on
            # UID/permission behavior: the daemon temporary path is a directory.
            blocked_temp = self.state.parent / f".watched_repos.json.{process.pid}.{sequence}.tmp"
            blocked_temp.mkdir()
            self.assert_server_error(request(method, {"path": str(self.repo)}))
            self.assertEqual(self.state.read_text(), original)

    def test_duplicate_watch_and_absent_unwatch_remain_successful(self):
        _, request, _, _ = self.start_daemon()
        path = {"path": str(self.repo)}
        self.assertEqual(request("daemon/watch", path)["result"], {"added": True})
        self.assertEqual(request("daemon/watch", path)["result"], {"added": False})
        self.assertEqual(request("daemon/unwatch", path)["result"], {"removed": True})
        self.assertEqual(request("daemon/unwatch", path)["result"], {"removed": False})

    def prepare_deleted_untracked_file(self):
        self.cli("index")
        (self.repo / "untracked.rs").write_text("pub fn removed_untracked_definition() {}\n")
        self.cli("index")
        (self.repo / "untracked.rs").unlink()
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.state.write_text(json.dumps({"repositories": [str(self.repo)]}))
        return next((self.base / "data").rglob("index.sqlite"))

    def test_nested_repo_override_keeps_full_catalog_and_context_snippets(self):
        nested = self.repo / "src"
        nested.mkdir()
        (nested / "module.rs").write_text("pub fn nested_definition() {}\n")
        self.git("add", "src")
        self.git("commit", "-qm", "nested fixture")
        self.cli("index")
        database = next((self.base / "data").rglob("index.sqlite"))

        def catalog():
            with sqlite3.connect(database) as conn:
                return conn.execute("SELECT relative_path FROM files ORDER BY relative_path").fetchall()

        before = catalog()
        _, request, _, _ = self.start_daemon()

        def tool(name, arguments):
            response = request("tools/call", {"name": name, "arguments": arguments})
            self.assertNotIn("error", response)
            self.assertFalse(response["result"]["isError"], response)
            return json.loads(response["result"]["content"][0]["text"])

        package = tool("repository_context", {
            "repo_path": str(nested), "task": "committed_definition nested_definition",
        })
        snippets = {item["relative_path"]: item["snippet"] for item in package["relevant_files"]}
        self.assertIn("committed_definition", snippets["main.rs"])
        self.assertIn("nested_definition", snippets["src/module.rs"])
        tool("add", {"repo_path": str(nested)})
        # The session now also names the nested directory; an omitted override
        # must resolve to the same parent worktree before scanning.
        tool("cognify", {})
        self.assertEqual(catalog(), before)
        self.assertEqual(self.cli("symbol", "committed_definition")["count"], 1)
        self.assertEqual(self.cli("symbol", "nested_definition")["count"], 1)

    def test_restart_removes_deleted_untracked_catalog_and_search_results(self):
        self.prepare_deleted_untracked_file()
        self.start_daemon()
        self.wait_until(lambda: self.cli("symbol", "removed_untracked_definition")["count"] == 0)
        self.assertEqual(self.cli("search", "removed_untracked_definition")["count"], 0)
        database = next((self.base / "data").rglob("index.sqlite"))
        with sqlite3.connect(database) as conn:
            self.assertEqual(conn.execute("SELECT count(*) FROM files WHERE relative_path='untracked.rs'").fetchone()[0], 0)
        # Once reconciliation succeeds, unchanged ticks must not keep indexing.
        indexed_at = self.cli("status")["last_indexed_at"]
        time.sleep(1.2)
        self.assertEqual(self.cli("status")["last_indexed_at"], indexed_at)

    def test_restart_reconciles_tracked_content_reverted_to_clean_head(self):
        self.cli("index")
        (self.repo / "main.rs").write_text("pub fn transient_dirty_definition() {}\n")
        self.cli("index")
        self.git("checkout", "--", "main.rs")
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.state.write_text(json.dumps({"repositories": [str(self.repo)]}))
        self.start_daemon()
        self.wait_until(lambda: self.cli("symbol", "transient_dirty_definition")["count"] == 0)
        self.assertEqual(self.cli("symbol", "committed_definition")["count"], 1)

    def test_failed_startup_reconciliation_retries_without_a_git_change(self):
        database = self.prepare_deleted_untracked_file()
        with sqlite3.connect(database) as conn:
            conn.execute("""CREATE TRIGGER fixture_sync_failure BEFORE DELETE ON files
                            BEGIN SELECT RAISE(ABORT, 'fixture sync failure'); END""")
        _, _, log_path, _ = self.start_daemon()
        self.wait_until(lambda: "fixture sync failure" in log_path.read_text())
        self.assertEqual(self.cli("symbol", "removed_untracked_definition")["count"], 1)
        with sqlite3.connect(database) as conn:
            conn.execute("DROP TRIGGER fixture_sync_failure")
        self.wait_until(lambda: self.cli("symbol", "removed_untracked_definition")["count"] == 0)
        self.assertEqual(self.git("status", "--porcelain"), "")


if __name__ == "__main__":
    unittest.main()
