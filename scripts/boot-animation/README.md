# Native boot animation assets

`02-logo.svg` is the approved vector master. `generate_assets.py` renders its
34 perspective turn frames, one high-resolution static logo, and one ripple.
The committed transparent PNGs total less than 1 MB. Rebuilding the ISO does
not require Python image packages.

To edit the artwork, install Python's Pillow and pycairo and run:

```sh
python3 scripts/boot-animation/generate_assets.py
```

The native theme uses `SetBootProgressFunction(duration, progress)` for elapsed
time. Refresh calls do not advance the 2.5-second intro. It keeps only the
current projected image, settles to the static logo on a long boot or an input
prompt, and skips the animation in shutdown/reboot modes. Real messages and
password/question prompts are displayed through Plymouth callbacks. GDM owns
the final display handoff; the theme does not delay it or imitate a lock screen.

`check_native.c` exercises the actual installed Plymouth script plugin without
starting a daemon, acquiring a VT, or modifying host boot settings. It verifies
elapsed timing under repeated refreshes, delayed boot, prompt/message callbacks,
resize, shutdown, and quit. It writes sample PNGs to `/tmp` using Plymouth's
native pixel-buffer composition.

Compile it against the matching upstream Plymouth source headers and your
distribution's installed `script.so` and libply libraries. For the verified
Ubuntu Plymouth 24.004.60 installation, with the official source extracted to
`/tmp/02os-plymouth-source`:

```sh
cc -Wall -Wextra -Werror -o /tmp/02os-native-plymouth-check \
  scripts/boot-animation/check_native.c \
  -I/tmp/02os-plymouth-source/src/plugins/splash/script \
  -I/tmp/02os-plymouth-source/src/libply \
  -I/tmp/02os-plymouth-source/src/libply-splash-core \
  -I/tmp/02os-plymouth-source/src/libply-splash-graphics \
  /usr/lib/x86_64-linux-gnu/plymouth/script.so \
  -l:libply.so.5 -l:libply-splash-core.so.5 -l:libply-splash-graphics.so.5 \
  $(pkg-config --cflags --libs cairo) -lm
/tmp/02os-native-plymouth-check "$PWD/profile/airootfs/usr/share/plymouth/themes/02-turn-ripple" 0
/tmp/02os-native-plymouth-check "$PWD/profile/airootfs/usr/share/plymouth/themes/02-turn-ripple" 1
```

This validates native script behavior and rendering. A new ISO must still be
booted in BIOS and UEFI VMs to validate initramfs inclusion, GPU setup, and GDM
takeover. See the [official Plymouth source](https://gitlab.freedesktop.org/plymouth/plymouth)
and [ArchWiki integration instructions](https://wiki.archlinux.org/title/Plymouth).
