"""Dash-to-dock relative imports must point at files shipped in the overlay."""

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
EXT = ROOT / "profile/airootfs/usr/share/gnome-shell/extensions/dash-to-dock@micxgx.gmail.com"
IMPORT_RE = re.compile(r"""from\s*(['"])(\./[^'"]+)\1""")


class DashToDockImportTests(unittest.TestCase):
    def test_relative_imports_resolve_to_files(self):
        scripts = sorted(EXT.rglob("*.js"))
        self.assertGreater(len(scripts), 0)
        found = 0
        for path in scripts:
            text = path.read_text(encoding="utf-8", errors="replace")
            for match in IMPORT_RE.finditer(text):
                found += 1
                spec = match.group(2)
                target = path.parent / spec
                candidates = [target]
                if target.suffix != ".js":
                    candidates.append(Path(str(target) + ".js"))
                self.assertTrue(
                    any(candidate.is_file() for candidate in candidates),
                    f"{path.relative_to(EXT)} imports missing {spec}",
                )
        self.assertGreater(found, 0)

    def test_settings_ui_and_media_svg_exist(self):
        self.assertTrue((EXT / "Settings.ui").is_file())
        for name in ("glossy.svg", "highlight_stacked_bg.svg", "highlight_stacked_bg_h.svg", "logo.svg"):
            asset = EXT / "media" / name
            with self.subTest(asset=name):
                self.assertTrue(asset.is_file())


if __name__ == "__main__":
    unittest.main()
