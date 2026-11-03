# Wuwa Tappo for macOS (unofficial port)

A native macOS build of **Wuwa Tappo**, the Wuthering Waves desktop pet by **Kuro Games**.
Kuro ships it only as a Windows installer; this project re-implements the native layer
(Rust + AppKit) so the same app runs on macOS - pet overlay, key/click counting, native
menus, login, online sync, multiplayer rooms, full-screen overlay, shortcuts, autostart.

> **This is a community stop-gap until Kuro Games releases an official macOS version.**
> It is not affiliated with, endorsed by, or supported by Kuro Games.

## Disclaimer

- **All Wuwa Tappo / Wuthering Waves content is © Kuro Games, all rights reserved.** That
  includes the frontend bundle, characters, artwork, fonts, icons, names, and the servers the
  app talks to. None of it is redistributed in this repository: the build extracts it on your
  machine from the official installer you download yourself. Anything produced from it
  (`dist/`, `src-tauri/icons/`, the built `.app` / `.dmg`) is Kuro Games' property and must not
  be redistributed.
- Only the macOS port code in this repository (`src-tauri/src/`, `scripts/`, configuration,
  docs) is original work.
- **No warranty, no liability.** This software is provided "as is". The authors accept no
  responsibility for anything that happens from using it - including, but not limited to,
  data loss, account issues, violations of Kuro Games' terms of service, or damage to your
  system. Use it at your own risk. If Kuro Games objects to this project, it will be taken
  down.

## Requirements

- Apple Silicon Mac, macOS 12 or newer (built and tested on macOS 26, M4).
- Xcode Command Line Tools (`xcode-select --install`)
- Rust toolchain (`https://rustup.rs`), Node.js 18+, Python 3
- 7-Zip and the `brotli` Python module: `brew install sevenzip && pip3 install brotli`
- The official Windows installer, e.g. `WuwaTappo-global_1.0.3_x64-setup.exe`
  (download it from Kuro Games' Wuwa Tappo site - the **Global** build)

## Install (prebuilt)

A prebuilt `.dmg` is published under **Releases**. It is self-signed, not notarized, so on
first launch macOS will refuse to open it: right-click `Wuwa Tappo.app` > **Open** > **Open**
(or `xattr -dr com.apple.quarantine "/Applications/Wuwa Tappo.app"`). The signature is stable
across releases so the Input Monitoring grant survives updates.

The prebuilt app embeds Kuro Games' frontend bundle, artwork and fonts, which remain their
property. It is published only for convenience; it will be removed at Kuro Games' request.

## Build

```sh
git clone <this repo> && cd WuwaTappo-mac
scripts/build.sh --dist /path/to/WuwaTappo-global_1.0.3_x64-setup.exe --open
```

That does everything:

1. `scripts/extract-dist.py` unpacks the installer (installer shell → NSIS → `WuwaTappo.exe` →
   brotli'd Tauri asset map) into `dist/` and the app icon into `src-tauri/icons/`.
2. `scripts/patch-dist.py` applies the single 1-line frontend patch the port needs
   (`listen$1` reports listener registration so window-open events are delivered exactly once).
3. `scripts/make-signing-identity.sh` creates a local self-signed code-signing identity
   ("Wuwa Tappo Local Signing") once. macOS ties the Input Monitoring permission to the app's
   signature; ad-hoc signatures change on every build and silently lose the grant.
4. `npx tauri build` produces `src-tauri/target/release/bundle/macos/Wuwa Tappo.app` and a `.dmg`.

Options: `--debug` (debug bundle: verbose log, F5 devtools), `--adhoc` (skip the signing
identity; re-grant Input Monitoring after every rebuild), `--open` (launch when done).
Once `dist/` exists you can just run `scripts/build.sh` or `npx tauri build` directly.

## First run

1. Move `Wuwa Tappo.app` to `/Applications` (this registers the `wuwa-buddy://` login callback).
2. Launch it. Grant **Input Monitoring** when macOS asks (System Settings → Privacy & Security →
   Input Monitoring), then quit and relaunch. Without it there is no key/click counting and the
   pet cannot be clicked.
3. Open the panel (right-click the pet, or the status-bar icon) → log in. The browser opens
   Kuro's login page and redirects back into the app.

Logs: `~/Library/Logs/com.kuro.wuwatappo.global/wuwatappo.log` (Settings → "View log").
`WUWA_LOG_DEBUG=1` enables verbose logging, `WUWA_DEVTOOLS=1` opens the web inspector.

## What's different from the Windows build

| Topic | macOS implementation |
|---|---|
| Pet overlay | Windows are re-classed at runtime to non-activating `NSPanel`s at level 1001 with `CanJoinAllSpaces\|Stationary\|FullScreenAuxiliary` (the only combination macOS shows over other apps' full-screen Spaces - same as Bongo Cat). Panel and dialogs sit one level below the pet; toasts share its level. |
| Input | A listen-only `CGEventTap` feeds the frontend's `mouse-move` / `mouse-button` / `key-press` / `key-release` / `app-shortcut` events. The overlay stops ignoring the mouse only while the pointer is over the pet's hit regions; an event-sink view keeps WKWebView from double-firing and clicks never activate the app (your game keeps focus). |
| Frontend origin | Served from an in-process loopback server at `http://localhost:1421` (Kuro's socket.io server rejects WebSocket upgrades from `tauri://localhost`). A `document.cookie` shim keeps the session token, which the app stores in cookies. |
| Game mode auto-detect | Triggers when the frontmost app has a full-screen window on the pet's display. |
| Shortcuts | F7 game mode, F8 switch display, F9 hide/show pet (configurable; "Win" in the settings UI is ⌘). |
| Autostart / Dock icon | LaunchAgent; "Show taskbar icon" toggles the Dock icon. |
| Updater | Resolves the manifest the same way the vendor app does (activity-config, assets host, host json, manifest). There is no macOS artifact, so it reports "latest"; if `darwin-*` entries appear, the stock Tauri updater path is used. |
| Not implemented | Gamepad clicks. |

## Layout

| Path | What |
|---|---|
| `scripts/build.sh` | One-shot build (`--dist`, `--debug`, `--adhoc`, `--open`) |
| `scripts/extract-dist.py` | Installer → `dist/` + icons (never committed) |
| `scripts/patch-dist.py` | The single frontend patch |
| `scripts/make-signing-identity.sh` | Local code-signing identity |
| `src-tauri/src/lib.rs` | Plugin wiring, command table, main-thread watchdog |
| `src-tauri/src/windows.rs` | All windows (overlay, toast, panel, login, update/maintenance, about, OSS notice) and window commands |
| `src-tauri/src/macos.rs` | AppKit layer: NSPanel re-classing, levels, Spaces, click-through, event-sink view, cursor, screen insets, Dock auto-hide, full-screen detection |
| `src-tauri/src/input.rs` | Global event tap and hit-region capture |
| `src-tauri/src/menu.rs` | Status-bar menu and pet context menu |
| `src-tauri/src/commands.rs` | Logging, diagnostics, counters, shortcuts, locale, native-menu sync, updater bridge |
| `src-tauri/src/server.rs` | Loopback asset server |
| `src-tauri/src/storage.rs` | JSON persistence under `~/Library/Application Support/com.kuro.wuwatappo.global/` |
| `src-tauri/src/init.js` | Per-window init script (cookie shim, console → log) |
| `docs/frontend-contract.md` | Frontend ↔ backend interface the port implements (commands, events, windows, auth flow) |
