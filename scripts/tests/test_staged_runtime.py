"""The image build must package the runtime that this tree just compiled."""
import hashlib
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VERIFY = ROOT / "scripts/verify-staged-runtime.sh"
CHECK = ROOT / "profile/airootfs/usr/local/bin/02os-check-runtime"


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class StagedRuntimeTests(unittest.TestCase):
    def test_build_sh_is_the_only_supported_image_build(self):
        build = (ROOT / "build.sh").read_text()
        verify = build.index('"${SCRIPT_DIR}/scripts/verify-staged-runtime.sh"')
        stamp = build.index("printf 'built_by=build.sh")
        iso = build.index("mkarchiso -v")
        self.assertLess(verify, stamp)
        self.assertLess(stamp, iso)
        self.assertIn("cleanup_staged_runtime", build)
        self.assertNotIn('if [[ -f "${SCRIPT_DIR}/scripts/build-agent-runtime.sh" ]]', build)
        readme = (ROOT / "README.md").read_text()
        self.assertNotIn("mkarchiso -v", readme)
        self.assertIn("only supported", readme)
        customize = (ROOT / "profile/airootfs/root/customize_airootfs.sh").read_text()
        self.assertIn("/usr/local/bin/02os-check-runtime\n", customize)
        ignored = subprocess.run(
            ["git", "check-ignore", "-q", "profile/airootfs/usr/bin/02"],
            cwd=ROOT,
            check=False,
        )
        self.assertEqual(ignored.returncode, 0)

    def test_verify_accepts_a_matching_source_build(self):
        with tempfile.TemporaryDirectory(prefix="02os-stage-") as tmp:
            root = Path(tmp)
            self._stage(root, match=True)
            self._run(VERIFY, "--repo", str(root), expect=0)

    def test_verify_rejects_a_stale_staged_binary(self):
        with tempfile.TemporaryDirectory(prefix="02os-stale-") as tmp:
            root = Path(tmp)
            self._stage(root, match=False)
            result = self._run(VERIFY, "--repo", str(root), expect=1)
            self.assertIn("does not match the source build", result.stderr)

    def test_image_check_rejects_a_manual_tree(self):
        with tempfile.TemporaryDirectory(prefix="02os-manual-") as tmp:
            root = Path(tmp)
            self._image_root(root, stamped=False, match=True)
            result = self._run(CHECK, str(root), expect=1)
            self.assertIn("build stamp", result.stderr)
            self.assertIn("./build.sh", result.stderr)

    def test_image_check_accepts_a_build_sh_stamp(self):
        with tempfile.TemporaryDirectory(prefix="02os-stamped-") as tmp:
            root = Path(tmp)
            self._image_root(root, stamped=True, match=True)
            self._run(CHECK, str(root), expect=0)

    def test_image_check_rejects_a_hash_mismatch(self):
        with tempfile.TemporaryDirectory(prefix="02os-hash-") as tmp:
            root = Path(tmp)
            self._image_root(root, stamped=True, match=False)
            result = self._run(CHECK, str(root), expect=1)
            self.assertIn("does not match SOURCE_REVISION", result.stderr)

    def _run(self, *args, expect):
        result = subprocess.run(args, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, expect, result.stderr + result.stdout)
        return result

    def _stage(self, root, match):
        release = root / "components/02-agent/target/release"
        staged = root / "profile/airootfs/usr/bin"
        prov_dir = root / "profile/airootfs/usr/lib/02-agent"
        release.mkdir(parents=True)
        staged.mkdir(parents=True)
        prov_dir.mkdir(parents=True)
        (release / "02").write_bytes(b"source-02")
        (release / "02-agentd").write_bytes(b"source-daemon")
        staged_02 = b"source-02" if match else b"stale-02"
        staged_daemon = b"source-daemon" if match else b"stale-daemon"
        (staged / "02").write_bytes(staged_02)
        (staged / "02-agentd").write_bytes(staged_daemon)
        (staged / "02agent").symlink_to("02")
        # Provenance records the staged bytes. A stale binary still has to fail
        # the comparison with target/release, even when the hash file agrees.
        (prov_dir / "SOURCE_REVISION").write_text(
            "revision=unknown\n"
            f"sha256_02={sha256(staged / '02')}\n"
            f"sha256_02_agentd={sha256(staged / '02-agentd')}\n"
        )

    def _image_root(self, root, stamped, match):
        bindir = root / "usr/bin"
        prov_dir = root / "usr/lib/02-agent"
        bindir.mkdir(parents=True)
        prov_dir.mkdir(parents=True)
        daemon = b"live-daemon"
        (bindir / "02").write_bytes(b"live-02")
        (bindir / "02-agentd").write_bytes(daemon)
        (bindir / "02agent").symlink_to("02")
        recorded = sha256(bindir / "02") if match else "0" * 64
        (prov_dir / "SOURCE_REVISION").write_text(
            "revision=abc\n"
            f"sha256_02={recorded}\n"
            f"sha256_02_agentd={sha256(bindir / '02-agentd')}\n"
        )
        if stamped:
            (prov_dir / "BUILD_STAMP").write_text("built_by=build.sh\nrevision=abc\n")


if __name__ == "__main__":
    unittest.main()
