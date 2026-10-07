# Zero Portal boot transition

`02-logo.svg` is the approved vector master. The old turn-and-ripple theme and
its projected PNG frames have been removed. `generate_assets.py` now renders
one high-resolution transparent logo for the native `02-zero-portal` theme.
Pillow and pycairo are needed only to regenerate that asset, not to build an ISO.

Plymouth fades the logo in over 0.25 seconds of real elapsed boot time, then
holds it while the system starts. Refresh ticks do not advance the timeline.
Real boot messages, disk-password prompts and questions retain their native
callbacks; shutdown/reboot never play the intro.

The `02-zero-portal@02os` GNOME Shell extension completes the transition in GDM:
the 2 slips behind the 0, and the 0 keeps its oval and expands from where it
sits to reveal the **live login screen** through its center. A transparent hole
in a decorative overlay reveals GNOME's own actors; no login screenshot or
imitation password field is used. The monotonic easing has no bounce or overshoot.

This extension runs only in the greeter session. An atomic marker under
`/run/02os-zero-portal`, created by the owned tmpfiles configuration, permits
one reveal per system boot even if GDM replaces its ephemeral account. The
mask is nonreactive and exits immediately on interaction, errors, reduced
motion, monitor changes, session changes or a bounded timeout. Authentication
and system controls remain owned by GNOME.

GDM's dconf profile enables only this greeter extension. Ordinary desktop
extensions and their user defaults remain separate. The provisioner copies
both boot stages, the greeter settings and the tmpfiles configuration into
installed systems. A new ISO must be built to include these changes.

```sh
python3 scripts/boot-animation/generate_assets.py
```

`check_native.c` exercises the actual Plymouth script plugin, including parse,
elapsed fade, repeated refresh, long-boot hold, prompts/messages, resizing,
shutdown and quit. Compile it against the matching installed Plymouth headers
and libraries. This verifies the native logo stage; the circular live reveal
requires a GNOME 50 VM check, including boot, interruption and greeter re-entry.

The screen-design PNGs are proposals only. Changing desktop wallpaper affects
the user's actual lock screen; a richer GDM background requires separate
GNOME greeter customization.
