# Pop Shell provenance

This directory is a compiled JavaScript snapshot of [Pop Shell](https://github.com/pop-os/shell). Upstream develops the extension in TypeScript. The snapshot has no TypeScript sources and no build metadata that records the commit it was compiled from.

Upstream licenses Pop Shell under the GNU General Public License, version 3. `LICENSE` is the upstream license text fetched from branch `master_noble` (the same `LICENSE` blob as commit `7898b65c20735057faf0797f8ed056704ca55f0d`). That commit is the `master_noble` HEAD observed on 2026-10-06 (`feat: GNOME 50 support`). It is not a verified pin for this snapshot: the tree was not byte-compared with a fresh compile of that commit, and local `metadata.json` also lists GNOME Shell 51, which that commit's metadata does not. Do not treat the commit hash as the source revision of these files.

The pin is metadata version `2` plus `content_sha256` in `docs/desktop-motion-vendor.json`. The digest is SHA-256 over every file in this directory except `PROVENANCE.md`, in sorted relative-path order. Each file contributes `utf-8 path`, a NUL byte, the file bytes, and a NUL byte.

Local fixes and the previously recorded border-timer patch are listed on that manifest entry.
