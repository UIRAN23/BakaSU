# Native SUSFS LKM prototype

This fork adds optional SUSFS LKM loading to BakaSU's `ksud` and exposes it in the ordinary manager Settings screen. Autoload is disabled by default. The existing SUSFS configuration and early/late application paths remain responsible for managing SUSFS after it is available.

The first device-test build supports AArch64 `android14-6.1` only. It embeds the unchanged binary from [SUSFS build 38120855249](https://github.com/inforcqb/susfs4ksu-lkm/actions/runs/38120855249), commit `3ca6bdf5d6fb8e3a22b1f8f2a88305ce0111d7cd`, SHA-256 `0a14fed58e08efad89d637ca7a2cd1f65ba3bab08c6888e2be5ab85209564ae0`. That binary was previously checked on kernel `6.1.157-android14`; the new native loading path still requires a device test.

## Build

Run **Build native SUSFS LKM test APK** on branch `feat/native-susfs-lkm`. The workflow obtains pinned BakaSU kernel/ksuinit inputs, builds the modified daemon and manager, checks formatting and lint, then signs a separate arm64 test APK. It also provides corresponding source archives, licenses, binary provenance and an APK checksum. The test signing key is persistent so later test APKs can update the same installation.

The package is `org.bakasu.bakasu.susfslkm`, named **BakaSU SUSFS LKM**. Its signature differs from the official manager; install it alongside the official app and register it through the official manager's **Dynamic Manager** setting. No new boot image is required for this prototype.

## Device test

1. Install the test APK and register it in the official BakaSU manager's Dynamic Manager settings. Open the test app and confirm that it recognizes BakaSU and has root access.
2. Disable the separate SUSFS LKM module and any old common-script loader. Keep them installed for rollback.
3. In the test app's ordinary Settings, enable **Load SUSFS at boot**. Enabling verifies that the modified daemon is installed at `/data/adb/ksud`.
4. Perform a full reboot. A soft reboot does not reload this kernel module.
5. Check that the loader status is **Loaded by BakaSU**, then inspect the existing SUSFS configuration panel and create a fresh BakaSU bug report.
6. Save the loader status alongside the bug report:

```sh
su -c '/data/adb/ksud susfs lkm status'
su -c 'cat /data/adb/ksu/log/susfs-lkm-status.json'
```

The expected status is `loaded`, with `enabled`, `supported` and `available` all true. `already_active` means loading was skipped because an existing SUSFS backend was detected; it does not validate this prototype's loader.

Verify the startup log, SUSFS version/features, early and boot-completed configuration application, and the specific features used on the device. A visible panel or a successful module load alone does not establish that every SUSFS feature works.

## Recovery

Autoload skips safe mode. Before entering the kernel loader, `ksud` synchronizes a loading journal to disk. If a later normal boot finds an unfinished attempt from another boot, autoload is disabled. A returned loader or post-load verification error also disables future autoload. This recovery does not prevent the first kernel fault, and no automatic unload is attempted.

To stop native loading, turn the setting off or run:

```sh
su -c '/data/adb/ksud susfs lkm disable'
```

Then perform a full reboot. For rollback, reopen the official manager to reinstall its daemon, re-enable the separate SUSFS LKM module, and reboot again. If Android cannot start, use BakaSU safe mode or recovery to remove `/data/adb/ksu/.susfs-lkm.json` before trying another normal boot. Do not repeatedly re-enable a failed load without inspecting its logs.

## Scope and licensing

The `.ko` remains a separate GPL-2.0 component from `inforcqb/susfs4ksu-lkm`. The fork does not copy the upstream C loader into BakaSU; it uses BakaSU's Rust module loader with exact symbol lookup and refusal of missing or zero-address imports. Corresponding SUSFS sources and license are distributed with each test APK. BakaSU's existing licenses continue to apply to its components.

Matching KMI is an initial compatibility filter; vendor ABI and CFI behavior must still be validated on the actual device. No upstream pull request has been created for this prototype.
