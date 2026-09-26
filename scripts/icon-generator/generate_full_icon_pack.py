#!/usr/bin/env python3
"""
02-OS Full Glassmorphic Icon Theme Generator
Generates a complete vector icon suite for 02_OS (Arch Linux Hyprland / GNOME Live OS).
"""

import os
import sys
import subprocess

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, "..", ".."))

TARGET_DIRS = [
    os.path.join(REPO_ROOT, "profile", "airootfs", "usr", "share", "icons", "02-OS"),
]
if "--install-user" in sys.argv or "--install" in sys.argv:
    TARGET_DIRS.append(os.path.expanduser("~/.local/share/icons/02-OS"))


INDEX_THEME = """[Icon Theme]
Name=02-OS
Comment=02_OS Premium Glassmorphic Vector Icon Theme
Inherits=Papirus-Dark,Adwaita,hicolor
Example=02os-menu

Directories=scalable/apps,scalable/places,scalable/devices,scalable/categories,scalable/mimetypes,scalable/status,scalable/actions

[scalable/apps]
Context=Applications
Size=64
MinSize=16
MaxSize=512
Type=Scalable

[scalable/places]
Context=Places
Size=64
MinSize=16
MaxSize=512
Type=Scalable

[scalable/devices]
Context=Devices
Size=64
MinSize=16
MaxSize=512
Type=Scalable

[scalable/categories]
Context=Categories
Size=64
MinSize=16
MaxSize=512
Type=Scalable

[scalable/mimetypes]
Context=MimeTypes
Size=64
MinSize=16
MaxSize=512
Type=Scalable

[scalable/status]
Context=Status
Size=64
MinSize=16
MaxSize=512
Type=Scalable

[scalable/actions]
Context=Actions
Size=64
MinSize=16
MaxSize=512
Type=Scalable
"""

def svg_header(defs, body, width=512, height=512):
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="100%" height="100%">
  <defs>
    <!-- Ambient Drop Shadow -->
    <filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx="0" dy="18" stdDeviation="20" flood-color="#000000" flood-opacity="0.45"/>
    </filter>
    <filter id="folder-shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx="0" dy="14" stdDeviation="16" flood-color="#000000" flood-opacity="0.40"/>
    </filter>
    <filter id="pocket-shadow" x="-15%" y="-15%" width="130%" height="130%">
      <feDropShadow dx="0" dy="8" stdDeviation="10" flood-color="#000000" flood-opacity="0.32"/>
    </filter>
    <filter id="glyph-shadow" x="-25%" y="-25%" width="150%" height="150%">
      <feDropShadow dx="0" dy="8" stdDeviation="10" flood-color="#000000" flood-opacity="0.36"/>
    </filter>
    <filter id="neon-glow" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="12" result="blur"/>
      <feComposite in="SourceGraphic" in2="blur" operator="over"/>
    </filter>

    <!-- Glass Specular Rim Highlight -->
    <linearGradient id="specular-rim" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.50"/>
      <stop offset="25%" stop-color="#ffffff" stop-opacity="0.18"/>
      <stop offset="70%" stop-color="#ffffff" stop-opacity="0.04"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0.12"/>
    </linearGradient>

    <!-- Top Gloss Sheen Overlay -->
    <linearGradient id="top-gloss" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.32"/>
      <stop offset="40%" stop-color="#ffffff" stop-opacity="0.06"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0.0"/>
    </linearGradient>
    {defs}
  </defs>
  {body}
</svg>"""

def squircle_app(bg_id, inner_art):
    return f"""
  <!-- Base Squircle with Shadow -->
  <rect x="36" y="36" width="440" height="440" rx="104" ry="104" fill="url(#{bg_id})" filter="url(#shadow)"/>

  <!-- Top Glass Sheen -->
  <path d="M 36 140 C 36 82, 82 36, 140 36 L 372 36 C 430 36, 476 82, 476 140 L 476 210 C 360 240, 150 240, 36 210 Z" fill="url(#top-gloss)"/>

  <!-- Center Hero Artwork -->
  <g filter="url(#glyph-shadow)">
    {inner_art}
  </g>

  <!-- Specular Rim Border -->
  <rect x="36" y="36" width="440" height="440" rx="104" ry="104" fill="none" stroke="url(#specular-rim)" stroke-width="2.5"/>
"""

print("Base library template ready.")
