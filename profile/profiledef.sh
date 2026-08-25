#!/usr/bin/env bash
# shellcheck disable=SC2034

iso_name="02_OS"
iso_label="02_OS_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="02_OS <https://github.com/csy20/02_OS>"
iso_application="02_OS Live"
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="arch"
buildmodes=('iso')
bootmodes=('bios.syslinux'
           'uefi.systemd-boot')
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"
airootfs_image_tool_options=('-comp' 'zstd' '-Xcompression-level' '15' '-b' '1M')
bootstrap_tarball_compression=('zstd' '-c' '-T0' '--auto-threads=logical' '--long' '-19')
file_permissions=(
  ["/etc/shadow"]="0:0:400"
  ["/etc/sudoers.d/01-live"]="0:0:440"
  ["/root"]="0:0:750"
  ["/root/.automated_script.sh"]="0:0:755"
  ["/root/.gnupg"]="0:0:700"
  ["/home/live"]="1000:1000:750"
  ["/usr/local/bin/choose-mirror"]="0:0:755"
  ["/usr/local/bin/Installation_guide"]="0:0:755"
  ["/usr/local/bin/livecd-sound"]="0:0:755"
  ["/usr/local/bin/02os-alt-f4"]="0:0:755"
  ["/usr/local/bin/02os-power-dialog"]="0:0:755"
  ["/usr/local/bin/02os-screenshot"]="0:0:755"
  ["/usr/local/bin/02os-spotlight"]="0:0:755"
  ["/usr/local/bin/02os-apple-menu"]="0:0:755"
  ["/usr/local/bin/02os-window-switcher"]="0:0:755"
  ["/usr/local/bin/02os-installed-cleanup"]="0:0:755"
  ["/usr/local/bin/02os-session"]="0:0:755"
  ["/usr/local/bin/02os-ensure-live-user"]="0:0:755"
)
