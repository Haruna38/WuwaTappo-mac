# Frontend ↔ backend contract (Wuwa Tappo 1.0.4)

What the vendor's frontend bundle expects from the native backend, as observed from its
behaviour, and how this port provides it on macOS. This is a description of an interface,
not a copy of the frontend; the bundle itself is Kuro Games' property and is extracted
locally by `scripts/extract-dist.py`.

Shapes use TypeScript-ish notation. Numbers are logical (CSS) pixels unless marked physical.

## Windows

The frontend never creates windows; the backend creates them all. Label determines the route
(`index.html#/<route>`).

| Label | Route | Notes |
|---|---|---|
| `main` | `/` | Pet overlay. Transparent, frameless, covers the whole display, always on top, never focused, ignores the cursor except over hit regions. |
| `toast` | `/toast` | Transparent click-through overlay for cross-window toasts, same bounds as `main`. Shown by `show_toast_overlay`, never hidden by the frontend. |
| `panel` | `/panel[?tab=…]` | Bag/atlas/exchange/rooms/settings. Frameless, draggable and resizable from the page (1100×760 expanded, 958×760 collapsed). |
| `login` | `/login` | Hosts the login flow (520×520 here). |
| `update` | `/update` | Update dialog (568×620). |
| `maintenance` | `/update` | Same page rendering the maintenance notice (568×560). |
| `about` | `/about` | About dialog (820×720). |
| `oss-notice` | `/oss-notice` | Open-source notices (820×720). |

All windows are hidden (not destroyed) on close and re-shown on open.

## Commands (frontend → backend)

Argument objects use camelCase keys. Unless noted, return values are ignored.

**Window management**
- `open_panel_window({payload:{tab?}})`, `close_panel_window()`, `is_panel_window_open() → bool`,
  `raise_panel_window_above_pets()`, `set_panel_window_collapsed({payload:{collapsed}})`
- `open_login_window({intent:"login"|"show", compact})` - backend must then emit `login-modal-open {intent}` to the login window; `close_login_window()`; `resize_login_window_to_content({payload:{width,height}})`
- `open_update_window({detail})` - detail `{currentVersion,newVersion,notes,notesByLocale?,installerUrl,requiresManualDownload}`; backend emits `cat-app-update-available` (detail) and `update-modal-open`; `close_update_window()`; `get_app_update_detail() → detail|null`
- `open_maintenance_notice({payload})` - backend emits `maintenance-notice-open`; `get_maintenance_notice_payload()`, `has_maintenance_notice() → bool`, `hide_main_after_maintenance_notice_ready()`, `exit_maintenance_app()`
- `open_about_window()` / `close_about_window()` (emit `about-open`), `open_oss_notice_window()` / `close_oss_notice_window()` (emit `oss-notice-open`)
- `show_toast_overlay()`, `hide_toast_overlay()`

**Main overlay geometry**
- `set_main_window_fullscreen_shell()` - size the overlay to its display
- `get_main_desktop_safe_area() → {insets:{left,top,right,bottom}, autoHideEdge?:"left"|"top"|"right"|"bottom", foregroundFullscreen:bool, foregroundApplicationFullscreen:bool}` - polled every 1.5 s; drives dock placement and auto game mode
- `get_main_taskbar_insets() → {left,top,right,bottom}`, `get_taskbar_insets_at_point({payload:{x,y}})`
- `apply_main_window_position({payload:{alwaysOnTop,position,applyPosition}})`
- `sync_main_window_to_current_monitor()`, `set_main_window_content_bounds(...)` (unused by this build)
- Display switch: `set_main_window_monitor_bounds({payload:{x,y,width,height}})` (physical px, errors if a switch is in progress), `sync_aux_windows_to_main_monitor()`, `finish_main_window_monitor_switch()`
- `set_main_skip_taskbar({skip})` - Dock icon visibility

**Main overlay input**
- `set_main_hit_region({payload:{regions, inputRegions, updateNativeInput, windowPassthrough, nativeInput}})` - rectangles `{left,top,width,height,memberId?}` relative to the overlay. `inputRegions` (only when `updateNativeInput`) are where clicks must be captured and routed; `windowPassthrough` means capture nothing. `memberId` must be echoed in mouse events over that rectangle.
- `set_main_force_cursor_ignore({enabled})` - overlay ignores the cursor (baseline state)
- `set_main_swallow_mouse({enabled})` - keep capturing for the rest of a gesture
- `set_native_input_cursor({kind:"default"|"pointer"|"grab"})`, `set_overlay_cursor({kind})`
- `popup_main_dock_context_menu({x,y})` - native context menu at overlay-relative point; resolves when the menu closes

**Menu, shortcuts, toast**
- `sync_native_menu({labels:{openBag,openAtlas,openExchange,openRoom,hidePet,showPet,trayOpenSettings,dockSettings,windowSize,windowPassthrough,gameMode,windowOpacity,restart,quit,trayTooltip}, petDockVisible, petScale, windowOpacity, windowPassthrough, gameMode})`
- `get_app_shortcuts() → {gameMode,switchScreen,togglePet}` with `{modifiers,key}`; `modifiers` bitmask Alt=1 Ctrl=2 Shift=4 Win/⌘=8; `key` is a Windows virtual-key code, 0 = unbound. Defaults F7/F8/F9.
- `set_app_shortcuts({config})` - reject with the string `"duplicate"` or `"occupied"`; emit `app-shortcuts-changed` on success
- `set_shortcut_capture({active})` - suspend shortcuts while the user records one

**Counters** (`environment` is a fixed string per build)
- `load_key_press_count({environment}) → number`, `save_key_press_count({count,environment})`
- `load_cumulative_click_count({environment}) → {count,cuid,environment}`, `save_cumulative_click_count({count,cuid,environment})`

**Misc**
- `write_app_log({level:"info"|"warn"|"error", message})`
- `get_boot_diagnostics() → {system?,runtime?,cpu?,memory?,gpus?:[],display_names?:[{device_name,name}]}` (all optional, logged only)
- `get_track_os_info() → {os,osVersion}`
- `is_internal_testing_build() → bool`, `is_devtools_hotkeys_build() → bool`
- `get_installer_launch_locale() → string`, `save_app_ui_locale({locale})`
- `check_app_update({endpoint}) → null | {rid,currentVersion,version,date,body,rawJson}` - Tauri updater metadata; `rid` must be usable with the updater plugin's `download`/`install`
- `is_force_update_locked() → bool`
- `is_elevated_autostart_enabled()`, `set_elevated_autostart({enabled})` - Windows only; macOS uses the autostart plugin
- `exit_app()`
- Installer/uninstaller commands (`*_nsis_*`, `get_installer_launch_mode`, `get_preferred_system_ui_languages`) belong to the Windows NSIS UI and are not needed.

Plugins used: event, window, webview, app, path, image, resources, opener, dialog, process,
updater, deep-link, autostart.

## Events (backend → frontend)

All listeners are global (any window), so plain broadcasts work.

| Event | Payload |
|---|---|
| `mouse-move` | `{x,y,memberId?}` - physical screen pixels |
| `mouse-button` | `{button:"left"\|"right"\|…, action:"press"\|"release", memberId?}` - routed input over the pet; a press counts a click |
| `global-clap-mouse` | none - a click anywhere else, counts once |
| `gamepad-button-press` | none (not implemented here) |
| `key-press` / `key-release` | key name string (DOM `KeyboardEvent.key` style); a release counts a click |
| `app-shortcut` | `"togglePet"` \| `"gameMode"` \| `"switchScreen"` |
| `app-shortcuts-changed` | shortcuts object |
| `tray-quit`, `tray-restart`, `tray-toggle-pet-visibility`, `tray-toggle-window-passthrough`, `tray-toggle-game-mode` | none |
| `tray-set-pet-scale`, `tray-set-window-opacity` | number |
| `dock-open-panel`, `panel-open-tab` | `"bag"\|"atlas"\|"exchange"\|"room"\|"setting"` |
| `login-modal-open` | `{intent}` |
| `cat-app-update-available`, `update-modal-open`, `maintenance-notice-open`, `about-open`, `oss-notice-open` | see commands above |
| `cat-force-update-lock` | bool |
| `cat-native-network-status-changed` | `{internetAccess}` (optional) |
| `deep-link://new-url` | `string[]` (deep-link plugin) |

Clicks on the backend's own non-overlay windows (panel, dialogs) must not be re-emitted:
those pages count them themselves.

The frontend registers its listeners asynchronously after mount, so window-open events must
not be emitted before the target window has registered its listener. This port patches the
frontend's `listen` helper to call a `listener_ready {event}` command, and queues pending
payloads until then.

Frontend ↔ frontend events (`cat-settings-*`, `cat-panel-*`, `cat-toast*`, monitor-switch
handshakes, persist/disconnect-before-exit handshakes) only need the event plugin.

## Login and network

- Login opens the system browser on Kuro's login page with a `state` nonce and the custom
  scheme `wuwa-buddy`. The page redirects to `wuwa-buddy://auth/callback?state=…&access_token=…&cuid=…`,
  delivered through the deep-link plugin (`get_current` at launch, `deep-link://new-url` later).
  The app must be registered for that URL scheme.
- The session token is stored in `document.cookie`; WKWebView has no cookie jar for the app's
  origin, so `src-tauri/src/init.js` backs cookies with localStorage.
- All HTTP is done by the frontend (axios/fetch). Multiplayer rooms use socket.io over
  WebSocket; the server rejects upgrades from `tauri://localhost`, so the port serves the
  bundle from `http://localhost:1421` (see `src-tauri/src/server.rs`).

## Startup order of the main window (why timing matters)

1. Root mount: menu sync, force-update lock, deep-link binding, network monitoring.
2. Overlay mount: `set_focusable(false)`, `set_main_force_cursor_ignore`, safe-area refresh,
   build flags, layout (`set_main_window_fullscreen_shell`, `apply_main_window_position`,
   `set_main_hit_region`), optional about window, then input/tray listeners.
3. A step that never resolves stalls everything after it (including input listeners), and the
   page relies on `requestAnimationFrame`, which WebKit suspends for windows that are not on
   the active Space - hence the overlay must be visible on every Space and background
   throttling is disabled.
