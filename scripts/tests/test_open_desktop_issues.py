"""Regressions for desktop issues 85-90, 103, and 104."""

import hashlib
import importlib.util
import json
import os
import re
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LAUNCHER = ROOT / "profile/airootfs/usr/local/bin/pop-launcher"
POP = ROOT / "profile/airootfs/usr/share/gnome-shell/extensions/pop-shell@system76.com"
COMPIZ = ROOT / "profile/airootfs/usr/share/gnome-shell/extensions/compiz-windows-effect@hermes83.github.com"
SCHEMA_NAME = "org.gnome.shell.extensions.com.github.hermes83.compiz-windows-effect.gschema.xml"
GALLERY = ROOT / "scripts/icon-generator/generate_html_gallery.py"
HTML_OUT = ROOT / "02-OS-icons-preview.html"
ID_RE = re.compile(r"""\bid\s*=\s*["']([^"']+)["']""")


def load_launcher():
    spec = importlib.util.spec_from_file_location("pop_launcher_under_test", LAUNCHER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_gallery():
    spec = importlib.util.spec_from_file_location("generate_html_gallery", GALLERY)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_desktop(path, name, hidden=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    hidden_line = "Hidden=true\n" if hidden else ""
    path.write_text(
        "[Desktop Entry]\n"
        "Type=Application\n"
        f"Name={name}\n"
        "Exec=true\n"
        f"{hidden_line}",
        encoding="utf-8",
    )


def run_launcher(home, data_home, data_dirs, messages):
    env = os.environ.copy()
    env["HOME"] = str(home)
    env["XDG_DATA_HOME"] = str(data_home)
    env["XDG_DATA_DIRS"] = str(data_dirs)
    result = subprocess.run(
        [str(LAUNCHER)],
        input="".join(message + "\n" for message in messages),
        text=True,
        capture_output=True,
        env=env,
        timeout=10,
        check=False,
    )
    updates = []
    for line in result.stdout.splitlines():
        if not line.strip():
            continue
        payload = json.loads(line)
        if "Update" in payload:
            updates.append(payload["Update"])
    return result, updates


def pop_shell_content_sha256(root):
    digest = hashlib.sha256()
    files = sorted(
        path for path in root.rglob("*")
        if path.is_file() and path.name != "PROVENANCE.md"
    )
    for path in files:
        rel = path.relative_to(root).as_posix()
        digest.update(rel.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


class DesktopJavaScriptTests(unittest.TestCase):
    def test_node_regressions(self):
        script = Path(__file__).resolve().parent / "desktop_js_regressions.mjs"
        result = subprocess.run(
            ["node", str(script)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            timeout=20,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class PopLauncherTests(unittest.TestCase):
    def test_item_quit_does_not_stop_later_search(self):
        with tempfile.TemporaryDirectory(prefix="02os-launcher-") as tmp:
            root = Path(tmp)
            system = root / "system"
            write_desktop(system / "applications" / "org.example.SystemAudit.desktop", "SYSTEM_AUDIT_UNIQUE")
            result, updates = run_launcher(
                root / "home",
                root / "user-data",
                system,
                [
                    '{"Search":"SYSTEM_AUDIT_UNIQUE"}',
                    '{"Quit":0}',
                    '{"Search":"SYSTEM_AUDIT_UNIQUE"}',
                    '"Exit"',
                ],
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(updates), 2)
        self.assertEqual([row["name"] for row in updates[0]], ["SYSTEM_AUDIT_UNIQUE"])
        self.assertEqual([row["name"] for row in updates[1]], ["SYSTEM_AUDIT_UNIQUE"])
        self.assertNotIn("Activate", result.stdout)

    def test_user_desktop_and_hidden_override(self):
        with tempfile.TemporaryDirectory(prefix="02os-xdg-") as tmp:
            root = Path(tmp)
            system = root / "system"
            user = root / "user-data"
            write_desktop(user / "applications" / "org.example.UserAudit.desktop", "USER_AUDIT_UNIQUE")
            write_desktop(system / "applications" / "org.example.Shared.desktop", "SYSTEM_SHARED_AUDIT")
            write_desktop(user / "applications" / "org.example.Shared.desktop", "USER_SHARED_HIDDEN", hidden=True)
            write_desktop(system / "applications" / "org.example.Override.desktop", "SYSTEM_OVERRIDE_AUDIT")
            write_desktop(user / "applications" / "org.example.Override.desktop", "USER_OVERRIDE_AUDIT")
            result, updates = run_launcher(
                root / "home",
                user,
                system,
                [
                    '{"Search":"USER_AUDIT_UNIQUE"}',
                    '{"Search":"SYSTEM_SHARED_AUDIT"}',
                    '{"Search":"USER_OVERRIDE_AUDIT"}',
                    '{"Search":"SYSTEM_OVERRIDE_AUDIT"}',
                    '"Exit"',
                ],
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(updates), 4)
        self.assertEqual([row["name"] for row in updates[0]], ["USER_AUDIT_UNIQUE"])
        self.assertEqual(updates[1], [])
        self.assertEqual([row["name"] for row in updates[2]], ["USER_OVERRIDE_AUDIT"])
        self.assertEqual(updates[3], [])

    def test_frontend_drops_service_after_backend_eof(self):
        service = (POP / "launcher_service.js").read_text(encoding="utf-8")
        launcher = (POP / "launcher.js").read_text(encoding="utf-8")
        self.assertIn("else if (this.onClosed)", service)
        self.assertIn("this.on_service_closed(launcher_service)", launcher)
        self.assertIn("if (this.service === closed)", launcher)
        self.assertIn("if (this.service === null && this.opened)", launcher)


class IconGalleryTests(unittest.TestCase):
    def test_prefixed_svg_ids_are_unique_and_references_follow(self):
        gallery = load_gallery()
        svg = """<svg><defs><linearGradient id="bg-stat"/><filter id="shadow"/></defs>
            <circle fill="url(#bg-stat)" filter="url(#shadow)" href="#bg-stat"/>
            <use xlink:href="#shadow"/></svg>"""
        first = gallery.prefix_svg_ids(svg, "icon-a-")
        second = gallery.prefix_svg_ids(svg, "icon-b-")
        html = first + second
        ids = ID_RE.findall(html)
        self.assertEqual(len(ids), len(set(ids)))
        self.assertIn('id="icon-a-bg-stat"', first)
        self.assertIn('fill="url(#icon-a-bg-stat)"', first)
        self.assertIn('href="#icon-a-bg-stat"', first)
        self.assertIn('xlink:href="#icon-a-shadow"', first)
        self.assertNotIn("url(#bg-stat)", html)
        self.assertNotIn('id="bg-stat"', html)

    def test_committed_gallery_ids_are_unique(self):
        gallery = load_gallery()
        html = HTML_OUT.read_text(encoding="utf-8")
        self.assertEqual(html, gallery.build_gallery())
        ids = ID_RE.findall(html)
        self.assertGreater(len(ids), 0)
        self.assertEqual(len(ids), len(set(ids)))
        self.assertNotIn('id="bg-stat"', html)
        self.assertNotIn("url(#bg-stat)", html)
        error = html.split('<div class="icon-name">dialog-error</div>', 1)[0]
        warning = html.split('<div class="icon-name">dialog-warning</div>', 1)[0]
        error_card = error[error.rfind('<div class="icon-card">'):]
        warning_card = warning[warning.rfind('<div class="icon-card">'):]
        self.assertIn("#dc2626", error_card)
        self.assertIn("#b45309", warning_card)
        self.assertNotIn("#dc2626", warning_card)


class CompizSchemaTests(unittest.TestCase):
    def test_speedup_divider_range_matches_on_both_copies(self):
        local = COMPIZ / "schemas" / SCHEMA_NAME
        system = ROOT / "profile/airootfs/usr/share/glib-2.0/schemas" / SCHEMA_NAME
        self.assertEqual(local.read_bytes(), system.read_bytes())
        tree = ET.parse(local)
        key = tree.find(".//key[@name='speedup-factor-divider']")
        self.assertIsNotNone(key)
        bounds = key.find("range")
        self.assertIsNotNone(bounds)
        self.assertGreater(float(bounds.get("min")), 0)
        self.assertGreater(float(bounds.get("max")), float(bounds.get("min")))
        self.assertLessEqual(float(bounds.get("max")), 40)
        self.assertGreaterEqual(float(bounds.get("min")), 2)


class PopShellProvenanceTests(unittest.TestCase):
    def test_license_and_content_pin(self):
        manifest = json.loads((ROOT / "docs/desktop-motion-vendor.json").read_text(encoding="utf-8"))
        entry = manifest["existing_extension_patches"][0]
        self.assertEqual(entry["uuid"], "pop-shell@system76.com")
        self.assertEqual(entry["version"], 2)
        self.assertEqual(entry["license"], "GPL-3.0-only")
        self.assertFalse(entry["snapshot_commit_verified"])
        license_path = POP / entry["license_file"]
        text = license_path.read_text(encoding="utf-8")
        self.assertIn("GNU GENERAL PUBLIC LICENSE", text)
        self.assertIn("Version 3, 29 June 2007", text)
        provenance = (POP / "PROVENANCE.md").read_text(encoding="utf-8")
        self.assertIn("not a verified pin", provenance)
        self.assertIn(entry["inspected_upstream_head"], provenance)
        metadata = json.loads((POP / "metadata.json").read_text(encoding="utf-8"))
        self.assertEqual(metadata["version"], entry["version"])
        self.assertEqual(pop_shell_content_sha256(POP), entry["content_sha256"])


if __name__ == "__main__":
    unittest.main()
