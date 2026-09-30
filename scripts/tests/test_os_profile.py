"""Validate the ISO overlay without running root customization or installing an OS."""
import ast
from pathlib import Path
import re
import shutil
import subprocess
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
        for relative, encoded in permissions.items():
            with self.subTest(path=relative):
                path = OVERLAY / relative.lstrip("/")
                self.assertTrue(path.exists() or path.is_symlink(), f"Permission mapping has no overlay path: {relative}")
                uid, gid, mode = encoded.split(":")
                self.assertEqual((uid, gid), ("0", "0"))
                self.assertEqual(int(mode, 8) & 0o022, 0, f"Group/other writable privileged overlay file: {relative}")
        self.assertEqual(permissions["/etc/sudoers.d/01-live"], "0:0:440")
        self.assertEqual((OVERLAY / "usr/bin/02agent").readlink(), Path("02"))
        for script in ("02os-gnome-session", "02os-ensure-live-user", "pop-shell-shortcuts"):
            self.assertEqual(permissions[f"/usr/local/bin/{script}"], "0:0:755")


if __name__ == "__main__":
    unittest.main()
