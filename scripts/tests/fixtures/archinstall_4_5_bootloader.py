# SPDX-License-Identifier: GPL-3.0-only
"""Unmodified methods from Archinstall 4.5 for side-effect-free recording tests.
Attribution: Archinstall contributors. See LICENSE.archinstall in this directory.
Source: https://github.com/archlinux/archinstall/blob/4.5/archinstall/lib/installer.py
Full source SHA-256: 8a891c129a4baf266f60c0fe9cbbcc38b22c4e895c6708b3fabef0aea7abdea3
Extracted 2026-10-08; only the wrapper, imports, dependencies and effects are
replaced by the test harness. Both copied method bodies remain unmodified.
"""
from __future__ import annotations


class Installer45:
	def _add_efistub_bootloader(
		self,
		boot_partition: PartitionModification,
		root: PartitionModification | LvmVolume,
		uki_enabled: bool = False,
	) -> None:
		debug('Installing efistub bootloader')

		self.pacman.strap('efibootmgr')

		if not SysInfo.has_uefi():
			raise HardwareIncompatibilityError

		# TODO: Ideally we would want to check if another config
		# points towards the same disk and/or partition.
		# And in which case we should do some clean up.

		if not uki_enabled:
			loader = '/vmlinuz-{kernel}'
			# EFI standards stipulate backslashes
			entries = (
				r'initrd=\initramfs-{kernel}.img',
				*self._get_kernel_params(root),
			)

			cmdline = [' '.join(entries)]
		else:
			loader = '/EFI/Linux/arch-{kernel}.efi'
			cmdline = []

		parent_dev_path = get_parent_device_path(boot_partition.safe_dev_path)

		cmd_template = (
			'efibootmgr',
			'--create',
			'--disk',
			str(parent_dev_path),
			'--part',
			str(boot_partition.partn),
			'--label',
			'Arch Linux ({kernel})',
			'--loader',
			loader,
			'--unicode',
			*cmdline,
			'--verbose',
		)

		for kernel in self.kernels:
			# Setup the firmware entry
			cmd = [arg.format(kernel=kernel) for arg in cmd_template]
			SysCommand(cmd)

		self._helper_flags['bootloader'] = 'efistub'

	def add_bootloader(
		self, bootloader: Bootloader, uki_enabled: bool = False, bootloader_removable: bool = False, plymouth: PlymouthTheme | None = None
	) -> None:
		"""
		Adds a bootloader to the installation instance.
		Archinstall supports one of five types:
		* systemd-bootctl
		* grub
		* limine
		* efistub (beta)
		* refnd (beta)

		:param bootloader: Type of bootloader to be added
		:param uki_enabled: Whether to use unified kernel images
		:param bootloader_removable: Whether to install to removable media location (UEFI only, for GRUB and Limine)
		:param plymouth: Optional Plymouth theme to install and configure
		"""

		for plugin in plugins.values():
			if hasattr(plugin, 'on_add_bootloader'):
				# Allow plugins to override the boot-loader handling.
				# This allows for boot configuring and installing bootloaders.
				if plugin.on_add_bootloader(self):
					return

		efi_partition = self._get_efi_partition()
		boot_partition = self._get_boot_partition()
		root = self._get_root()

		if boot_partition is None:
			raise ValueError(f'Could not detect boot at mountpoint {self.target}')

		if root is None:
			raise ValueError(f'Could not detect root at mountpoint {self.target}')

		info(f'Adding bootloader {bootloader.value} to {boot_partition.dev_path}')

		# validate UKI support
		if uki_enabled and not bootloader.has_uki_support():
			warn(f'Bootloader {bootloader.value} does not support UKI; disabling.')
			uki_enabled = False

		# validate removable bootloader option
		if bootloader_removable:
			if not SysInfo.has_uefi():
				warn('Removable install requested but system is not UEFI; disabling.')
				bootloader_removable = False
			elif not bootloader.has_removable_support():
				warn(f'Bootloader {bootloader.value} lacks removable support; disabling.')
				bootloader_removable = False

		if plymouth is not None:
			self._install_plymouth(plymouth)

		if uki_enabled:
			keep_initramfs = (
				bootloader == Bootloader.Grub
				and self._disk_config.has_default_btrfs_vols()
				and self._disk_config.btrfs_options is not None
				and self._disk_config.btrfs_options.snapshot_config is not None
			)
			self._config_uki(root, efi_partition, keep_initramfs)

		match bootloader:
			case Bootloader.Systemd:
				self._add_systemd_bootloader(boot_partition, root, efi_partition, uki_enabled)
			case Bootloader.Grub:
				self._add_grub_bootloader(boot_partition, root, efi_partition, uki_enabled, bootloader_removable)
			case Bootloader.Efistub:
				self._add_efistub_bootloader(boot_partition, root, uki_enabled)
			case Bootloader.Limine:
				self._add_limine_bootloader(boot_partition, efi_partition, root, uki_enabled, bootloader_removable)
			case Bootloader.Refind:
				self._add_refind_bootloader(boot_partition, efi_partition, root, uki_enabled)
