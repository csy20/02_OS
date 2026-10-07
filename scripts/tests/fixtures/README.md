# Archinstall bootloader recording fixture

`archinstall_4_5_bootloader.py` copies `Installer._add_efistub_bootloader` and
`Installer.add_bootloader` from the Archinstall contributors' [4.5 source](https://github.com/archlinux/archinstall/blob/4.5/archinstall/lib/installer.py).
The upstream file SHA-256 is
`8a891c129a4baf266f60c0fe9cbbcc38b22c4e895c6708b3fabef0aea7abdea3`.

The two method bodies are unchanged. On 2026-10-08 the extraction added a
standalone class wrapper and deferred type annotations; recording tests supply
dependencies and replace side effects. The installed-boot plugin adapts the
same EFISTUB method on one installer instance and changes the firmware label
to `02_OS`. It retains the upstream package, disk, partition, kernel, initrd,
command-line and UKI behavior.

The fixture and derived plugin are licensed under `GPL-3.0-only`.
[LICENSE.archinstall](LICENSE.archinstall) is the unmodified [upstream 4.5 license](https://github.com/archlinux/archinstall/blob/4.5/LICENSE), including its
copyright and warranty notices. Its SHA-256 is
`3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986`.
