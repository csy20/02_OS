"""Validate the ISO overlay without running root customization or installing an OS."""
import ast
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[2]
OVERLAY = ROOT / "profile/airootfs"


class OSProfileTests(unittest.TestCase):
    def checked(self, *command):
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, f"{command}: {result.stderr}{result.stdout}")

    def test_package_manifest_has_unique_entries(self):
        packages = [line.strip() for line in (ROOT / "profile/packages.x86_64").read_text().splitlines()
                    if line.strip() and not line.lstrip().startswith("#")]
        self.assertEqual(len(packages), len(set(packages)), "Duplicate packages in ISO manifest")
        self.assertTrue({"linux", "gnome-shell", "gnome-session", "networkmanager", "archinstall"} <= set(packages))

    def test_bash_scripts_parse_without_execution(self):
        scripts = [ROOT / "build.sh", ROOT / "profile/profiledef.sh", *sorted((ROOT / "scripts").glob("*.sh")),
                   OVERLAY / "root/customize_airootfs.sh", *sorted((OVERLAY / "usr/local/bin").iterdir())]
        checked = 0
        required = [ROOT / "build.sh", ROOT / "profile/profiledef.sh", ROOT / "scripts/build-agent-runtime.sh",
                    ROOT / "scripts/release.sh", OVERLAY / "root/customize_airootfs.sh",
                    OVERLAY / "usr/local/bin/02os-gnome-session", OVERLAY / "usr/local/bin/02os-ensure-live-user"]
        for path in required:
            with self.subTest(required=str(path.relative_to(ROOT))):
                self.assertTrue(path.is_file())
                self.checked("bash", "-n", str(path))
        for path in scripts:
            if path.is_file() and path.read_bytes().split(b"\n", 1)[0] in (b"#!/bin/bash", b"#!/usr/bin/bash", b"#!/usr/bin/env bash"):
                with self.subTest(path=str(path.relative_to(ROOT))):
                    self.checked("bash", "-n", str(path))
                    checked += 1
        self.assertGreater(checked, 5)

    def test_python_generators_parse_without_execution(self):
        scripts = sorted((ROOT / "scripts/icon-generator").glob("*.py"))
        self.assertGreater(len(scripts), 0)
        for path in scripts:
            with self.subTest(path=path.name):
                ast.parse(path.read_text(), filename=str(path))

    def test_svg_and_schema_xml_is_well_formed(self):
        assets = sorted(OVERLAY.rglob("*.svg")) + sorted(OVERLAY.rglob("*.gschema.xml"))
        self.assertGreater(len(assets), 0)
        for path in assets:
            with self.subTest(path=str(path.relative_to(OVERLAY))):
                ET.parse(path)

    def test_desktop_entries_and_glib_schemas(self):
        for command in ("desktop-file-validate", "glib-compile-schemas"):
            self.assertIsNotNone(shutil.which(command), f"Install required CI dependency: {command}")
        for path in sorted((OVERLAY / "usr/share/applications").glob("*.desktop")):
            with self.subTest(path=path.name):
                self.checked("desktop-file-validate", str(path))
        self.checked("glib-compile-schemas", "--strict", "--dry-run", str(OVERLAY / "usr/share/glib-2.0/schemas"))

    def test_profile_permissions_and_runtime_alias(self):
        text = (ROOT / "profile/profiledef.sh").read_text()
        permissions = dict(re.findall(r'\["([^"]+)"\]="([^"]+)"', text))
        optional_runtime = {"/usr/bin/02", "/usr/bin/02agent", "/usr/bin/02-agentd"}
        for relative, encoded in permissions.items():
            with self.subTest(path=relative):
                path = OVERLAY / relative.lstrip("/")
                if relative not in optional_runtime:
                    self.assertTrue(path.exists() or path.is_symlink(), f"Permission mapping has no overlay path: {relative}")
                uid, gid, mode = encoded.split(":")
                self.assertEqual((uid, gid), ("0", "0"))
                self.assertEqual(int(mode, 8) & 0o022, 0, f"Group/other writable privileged overlay file: {relative}")
        self.assertEqual(permissions["/etc/sudoers.d/01-live"], "0:0:440")
        for binary in optional_runtime:
            self.assertEqual(permissions[binary], "0:0:755")
        alias = OVERLAY / "usr/bin/02agent"
        if alias.exists() or alias.is_symlink():
            self.assertTrue(alias.is_symlink(), "usr/bin/02agent must be a symlink to 02 when it is staged")
            self.assertEqual(alias.readlink(), Path("02"))
        for script in ("02os-gnome-session", "02os-ensure-live-user", "pop-shell-shortcuts",
                       "02os-install", "02os-provision", "pop-launcher"):
            self.assertEqual(permissions[f"/usr/local/bin/{script}"], "0:0:755")

    def test_install_desktop_launches_provisioner_entry(self):
        desktop_path = OVERLAY / "usr/share/applications/02os-install.desktop"
        desktop = desktop_path.read_text()
        self.assertIn("Name=Install 02_OS", desktop)
        exec_lines = [line for line in desktop.splitlines() if line.startswith("Exec=")]
        self.assertTrue(exec_lines)
        for line in exec_lines:
            self.assertIn("02os-install", line)
            self.assertNotIn("sudo archinstall", line)
        plugin = (OVERLAY / "usr/local/share/02os/archinstall_plugin.py").read_text()
        self.assertIn("__archinstall__version__ = 3.0", plugin)
        self.assertIn("def on_install", plugin)
        installer = (OVERLAY / "usr/local/bin/02os-install").read_text()
        self.assertIn("GNOME desktop profile", installer)
        self.assertIn("exec archinstall --plugin /usr/local/share/02os/archinstall_plugin.py --skip-version-check", installer)

    def test_agentd_user_unit_is_not_ignored(self):
        unit = OVERLAY / "usr/lib/systemd/user/02-agentd.service"
        text = unit.read_text()
        self.assertIn("ProtectHome=no", text)
        self.assertNotIn("ProtectHome=read-write", text)
        self.assertIn("ReadWritePaths=-%h/.local/share/02-agent -%h/.cache/02-agent -%t", text)
        binary = "/usr/bin/systemd-analyze"
        self.assertTrue(Path(binary).is_file(), binary)
        result = subprocess.run([binary, "--user", "verify", str(unit)], capture_output=True, text=True, timeout=30)
        combined = result.stdout + result.stderr
        self.assertNotIn("ignoring", combined.lower(), combined)

    def test_provisioner_copies_allowlist_and_skips_live_paths(self):
        provision = OVERLAY / "usr/local/bin/02os-provision"
        with tempfile.TemporaryDirectory(prefix="02os-prov-") as tmp:
            source = Path(tmp) / "source"
            target = Path(tmp) / "target"
            self._write_provision_fixture(source, include_runtime=True)
            forbidden = self._write_forbidden_live_paths(source)
            result = subprocess.run(
                [str(provision), "--source", str(source), "--target", str(target)],
                capture_output=True, text=True, timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((target / "usr/bin/02").read_text(), "runtime")
            self.assertTrue((target / "usr/bin/02agent").is_symlink())
            self.assertEqual((target / "usr/bin/02agent").readlink(), Path("02"))
            self.assertEqual((target / "usr/share/applications/02os-install.desktop").read_text(), "desktop")
            self.assertEqual((target / "usr/lib/02-agent/SOURCE_REVISION").read_text(), "rev\n")
            schema_dir = target / "usr/share/glib-2.0/schemas"
            self.assertEqual((schema_dir / "org.gnome.shell.extensions.dash-to-dock.gschema.xml").read_text(), "dock")
            self.assertEqual((schema_dir / "org.gnome.shell.extensions.pop-shell.gschema.xml").read_text(), "pop")
            self.assertFalse((schema_dir / "org.example.gschema.xml").exists())
            icon = target / "usr/share/icons/02-OS/scalable/apps/a.svg"
            self.assertTrue(icon.is_symlink())
            self.assertEqual(icon.readlink(), Path("b.svg"))
            self.assertFalse((target / "usr/bin/02-agentd").exists())
            self.assertFalse((target / "usr/share/backgrounds/02os").exists())
            for rel in forbidden:
                self.assertFalse((target / rel).exists(), rel)
            self.assertFalse((target / "etc/hostname").exists())

            bare = Path(tmp) / "bare-source"
            bare_target = Path(tmp) / "bare-target"
            self._write_provision_fixture(bare, include_runtime=False)
            env = os.environ.copy()
            env["02OS_OVERLAY"] = str(bare)
            result = subprocess.run(
                [str(provision), str(bare_target)],
                capture_output=True, text=True, timeout=30, env=env,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse((bare_target / "usr/bin/02").exists())
            self.assertEqual((bare_target / "usr/share/applications/02os-install.desktop").read_text(), "desktop")

    def test_provisioner_refuses_symlink_outside_source(self):
        provision = OVERLAY / "usr/local/bin/02os-provision"
        with tempfile.TemporaryDirectory(prefix="02os-escape-") as tmp:
            source = Path(tmp) / "source"
            target = Path(tmp) / "target"
            escaped = source / "usr/bin/02"
            escaped.parent.mkdir(parents=True)
            escaped.symlink_to("/etc/passwd")
            desktop = source / "usr/share/applications/02os-install.desktop"
            desktop.parent.mkdir(parents=True)
            desktop.write_text("desktop")
            result = subprocess.run(
                [str(provision), "--source", str(source), "--target", str(target)],
                capture_output=True, text=True, timeout=30,
            )
            self.assertNotEqual(result.returncode, 0, result.stderr)
            self.assertFalse((target / "usr/bin/02").exists())
            self.assertFalse((target / "usr/share/applications/02os-install.desktop").exists())

            nested = Path(tmp) / "nested-source"
            nested_target = Path(tmp) / "nested-target"
            icon = nested / "usr/share/icons/02-OS"
            icon.mkdir(parents=True)
            (icon / "ok.txt").write_text("ok")
            (icon / "bad").symlink_to("/etc/passwd")
            result = subprocess.run(
                [str(provision), "--source", str(nested), "--target", str(nested_target)],
                capture_output=True, text=True, timeout=30,
            )
            self.assertNotEqual(result.returncode, 0, result.stderr)
            self.assertFalse((nested_target / "usr/share/icons/02-OS/ok.txt").exists())
            self.assertFalse((nested_target / "usr/share/icons/02-OS/bad").exists())

    def _write_provision_fixture(self, source, include_runtime):
        if include_runtime:
            bindir = source / "usr/bin"
            bindir.mkdir(parents=True)
            (bindir / "02").write_text("runtime")
            (bindir / "02agent").symlink_to("02")
        desktop = source / "usr/share/applications/02os-install.desktop"
        desktop.parent.mkdir(parents=True)
        desktop.write_text("desktop")
        apps = source / "usr/share/icons/02-OS/scalable/apps"
        apps.mkdir(parents=True)
        (apps / "b.svg").write_text("<svg xmlns='http://www.w3.org/2000/svg'/>")
        (apps / "a.svg").symlink_to("b.svg")
        schema_dir = source / "usr/share/glib-2.0/schemas"
        schema_dir.mkdir(parents=True)
        (schema_dir / "org.example.gschema.xml").write_text("<schemalist/>")
        (schema_dir / "org.gnome.shell.extensions.dash-to-dock.gschema.xml").write_text("dock")
        (schema_dir / "org.gnome.shell.extensions.pop-shell.gschema.xml").write_text("pop")
        dconf = source / "etc/dconf/db/local.d/00-02os"
        dconf.parent.mkdir(parents=True)
        dconf.write_text("dconf")
        provenance = source / "usr/lib/02-agent/SOURCE_REVISION"
        provenance.parent.mkdir(parents=True)
        provenance.write_text("rev\n")
        service = source / "usr/lib/systemd/user/02-agentd.service"
        service.parent.mkdir(parents=True)
        service.write_text("[Service]\n")
        wants = source / "etc/systemd/user/default.target.wants/02-agentd.service"
        wants.parent.mkdir(parents=True)
        wants.symlink_to("../../../../usr/lib/systemd/user/02-agentd.service")

    def _write_forbidden_live_paths(self, source):
        forbidden = [
            "etc/sudoers.d/01-live",
            "etc/greetd/config.toml",
            "etc/sysusers.d/02os.conf",
            "etc/systemd/system/02os-ensure-live-user.service",
            "etc/systemd/system/graphical.target.wants/02os-ensure-live-user.service",
            "etc/systemd/system/graphical.target.wants/greetd.service",
            "etc/ssh/sshd_config.d/10-archiso.conf",
            "etc/polkit-1/rules.d/10-live-power.rules",
            "usr/local/bin/02os-ensure-live-user",
            "etc/hostname",
        ]
        for rel in forbidden:
            path = source / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("live-only\n")
        return forbidden

    def test_pop_launcher_search_activate_and_exit(self):
        launcher = OVERLAY / "usr/local/bin/pop-launcher"
        self.assertTrue(launcher.is_file())
        dconf = (OVERLAY / "etc/dconf/db/local.d/00-02os").read_text()
        self.assertIn("pop-shell@system76.com", dconf)
        self.assertIn("dash-to-dock@micxgx.gmail.com", dconf)
        with tempfile.TemporaryDirectory(prefix="02os-launch-") as tmp:
            apps = Path(tmp) / "applications"
            apps.mkdir()
            desktop = apps / "org.gnome.Console.desktop"
            desktop.write_text(
                "[Desktop Entry]\n"
                "Type=Application\n"
                "Name=GNOME Console\n"
                "Exec=kgx\n"
                "Icon=org.gnome.Console\n"
            )
            env = os.environ.copy()
            env["XDG_DATA_DIRS"] = str(Path(tmp))
            proc = subprocess.Popen(
                [str(launcher)],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                env=env,
            )
            try:
                proc.stdin.write(json.dumps({"Search": "GNOME Console"}) + "\n")
                proc.stdin.flush()
                update = self._read_launcher_line(proc)
                self.assertIn("Update", update)
                matches = [item for item in update["Update"] if item.get("name") == "GNOME Console"]
                self.assertTrue(matches, update)
                found = None
                matched_item = None
                for item in matches:
                    proc.stdin.write(json.dumps({"Activate": item["id"]}) + "\n")
                    proc.stdin.flush()
                    activated = self._read_launcher_line(proc)
                    entry = activated.get("DesktopEntry")
                    if entry and entry.get("path") == str(desktop):
                        found = entry
                        matched_item = item
                        break
                self.assertIsNotNone(found, "DesktopEntry did not return the fixture desktop path")
                self.assertEqual(found["gpu_preference"], "Default")
                self.assertEqual(found["path"], str(desktop))
                self.assertEqual(matched_item.get("icon"), {"Name": "org.gnome.Console"})
                self.assertEqual(matched_item.get("exec"), "kgx")
                proc.stdin.write(json.dumps("Exit") + "\n")
                proc.stdin.flush()
                proc.stdin.close()
                code = proc.wait(timeout=5)
                err = ""
                if proc.stderr is not None:
                    err = proc.stderr.read()
                self.assertEqual(code, 0, err)
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait(timeout=5)
                for stream in (proc.stdin, proc.stdout, proc.stderr):
                    if stream is not None and not stream.closed:
                        stream.close()

    def _read_launcher_line(self, proc):
        ready, _, _ = select.select([proc.stdout.fileno()], [], [], 10)
        if not ready:
            proc.kill()
            self.fail("pop-launcher produced no output")
        line = proc.stdout.readline()
        self.assertTrue(line, "pop-launcher closed stdout")
        return json.loads(line)


if __name__ == "__main__":
    unittest.main()
