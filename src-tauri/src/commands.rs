//! Non-window commands: logging, diagnostics, counters, shortcuts, locale,
//! native menu sync, updater bridge, lifecycle.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::menu;
use crate::state::{AppShortcuts, AppState, Binding, NativeMenuLabels};
use crate::storage;

// ---------------------------------------------------------------- logging / diagnostics

#[tauri::command]
pub fn write_app_log(level: Option<String>, message: String) {
    match level.as_deref() {
        Some("error") => log::error!(target: "app", "{message}"),
        Some("warn") => log::warn!(target: "app", "{message}"),
        _ => log::info!(target: "app", "{message}"),
    }
}

fn sysctl(name: &str) -> Option<String> {
    let out = std::process::Command::new("/usr/sbin/sysctl")
        .arg("-n")
        .arg(name)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn os_version() -> String {
    let v = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
    format!("{}.{}.{}", v.majorVersion, v.minorVersion, v.patchVersion)
}

#[tauri::command]
pub fn get_track_os_info() -> serde_json::Value {
    serde_json::json!({ "os": "macOS", "osVersion": os_version() })
}

#[tauri::command]
pub async fn get_boot_diagnostics(app: AppHandle) -> Result<serde_json::Value, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(crate::macos::display_names());
    })
    .map_err(|e| e.to_string())?;
    let displays = rx.recv().unwrap_or_default();
    let arch = std::env::consts::ARCH;
    let mem_bytes = sysctl("hw.memsize").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    Ok(serde_json::json!({
        "system": {
            "os": "macOS",
            "os_version": os_version(),
            "arch": arch,
            "model": sysctl("hw.model").unwrap_or_default(),
            "build": sysctl("kern.osversion").unwrap_or_default(),
        },
        "runtime": {
            "app_version": app.package_info().version.to_string(),
            "tauri_version": tauri::VERSION,
            "webview": "WKWebView",
            "pid": std::process::id(),
        },
        "cpu": {
            "brand": sysctl("machdep.cpu.brand_string").unwrap_or_default(),
            "cores": sysctl("hw.ncpu").unwrap_or_default(),
        },
        "memory": {
            "total_mb": mem_bytes / (1024 * 1024),
        },
        "gpus": [],
        "display_names": displays,
    }))
}

#[tauri::command]
pub fn is_internal_testing_build() -> bool {
    false
}

#[tauri::command]
pub fn is_devtools_hotkeys_build() -> bool {
    cfg!(debug_assertions)
}

// ---------------------------------------------------------------- counters

#[tauri::command]
pub fn load_key_press_count(app: AppHandle, environment: String) -> u64 {
    storage::load_key_press_count(&app, &environment)
}

#[tauri::command]
pub fn save_key_press_count(app: AppHandle, count: f64, environment: String) -> Result<(), String> {
    let count = if count.is_finite() && count > 0.0 { count.floor() as u64 } else { 0 };
    storage::save_key_press_count(&app, &environment, count)
}

#[tauri::command]
pub fn load_cumulative_click_count(app: AppHandle, environment: String) -> storage::CumulativeClickStore {
    storage::load_cumulative_click(&app, &environment)
}

#[tauri::command]
pub fn save_cumulative_click_count(
    app: AppHandle,
    count: f64,
    cuid: String,
    environment: String,
) -> Result<(), String> {
    let count = if count.is_finite() && count > 0.0 { count.floor() as u64 } else { 0 };
    storage::save_cumulative_click(
        &app,
        &storage::CumulativeClickStore {
            count,
            cuid,
            environment,
        },
    )
}

// ---------------------------------------------------------------- shortcuts

#[tauri::command]
pub fn get_app_shortcuts(state: State<'_, AppState>) -> AppShortcuts {
    state.input.lock().unwrap().shortcuts
}

fn binding_is_set(b: &Binding) -> bool {
    b.key != 0
}

#[tauri::command]
pub fn set_app_shortcuts(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppShortcuts,
) -> Result<(), String> {
    let all = [config.game_mode, config.switch_screen, config.toggle_pet];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            if binding_is_set(&all[i]) && all[i] == all[j] {
                return Err("duplicate".into());
            }
        }
    }
    for b in &all {
        if binding_is_set(b) && crate::keys::vk_to_mac(b.key).is_none() {
            return Err("unavailable".into());
        }
    }
    storage::save_shortcuts(&app, &config)?;
    state.input.lock().unwrap().shortcuts = config;
    let _ = app.emit("app-shortcuts-changed", config);
    Ok(())
}

#[tauri::command]
pub fn set_shortcut_capture(state: State<'_, AppState>, active: bool) {
    state.input.lock().unwrap().shortcut_capture = active;
}

// ---------------------------------------------------------------- locale

const GLOBAL_LOCALES: &[&str] = &["zh", "en", "jp", "kr", "tw", "fr", "de", "es", "pt", "th"];

fn system_locale() -> String {
    let langs = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let first = langs
        .lines()
        .map(|l| l.trim().trim_matches(|c| c == '"' || c == ',' || c == '(' || c == ')').to_string())
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_lowercase();
    let code = if first.starts_with("zh-hant") || first.starts_with("zh-tw") || first.starts_with("zh-hk") {
        "tw"
    } else if first.starts_with("zh") {
        "zh"
    } else if first.starts_with("ja") {
        "jp"
    } else if first.starts_with("ko") {
        "kr"
    } else if first.starts_with("fr") {
        "fr"
    } else if first.starts_with("de") {
        "de"
    } else if first.starts_with("es") {
        "es"
    } else if first.starts_with("pt") {
        "pt"
    } else if first.starts_with("th") {
        "th"
    } else {
        "en"
    };
    code.to_string()
}

#[tauri::command]
pub fn get_installer_launch_locale(app: AppHandle) -> String {
    storage::load_ui_locale(&app)
        .filter(|l| GLOBAL_LOCALES.contains(&l.as_str()))
        .unwrap_or_else(system_locale)
}

#[tauri::command]
pub fn save_app_ui_locale(app: AppHandle, locale: String) -> Result<(), String> {
    storage::save_ui_locale(&app, &locale)
}

#[tauri::command]
pub fn get_installer_launch_mode() -> serde_json::Value {
    serde_json::json!({ "otaUpdate": false })
}

#[tauri::command]
pub fn get_preferred_system_ui_languages() -> Vec<String> {
    vec![system_locale()]
}

#[tauri::command]
pub fn sync_native_menu(
    app: AppHandle,
    state: State<'_, AppState>,
    labels: Option<NativeMenuLabels>,
    pet_dock_visible: Option<bool>,
    pet_scale: Option<f64>,
    window_opacity: Option<f64>,
    window_passthrough: Option<bool>,
    game_mode: Option<bool>,
) {
    {
        let mut ui = state.ui.lock().unwrap();
        let m = &mut ui.menu;
        if let Some(l) = labels {
            let fallback = NativeMenuLabels::english();
            let pick = |v: String, f: String| if v.trim().is_empty() { f } else { v };
            m.labels = NativeMenuLabels {
                open_bag: pick(l.open_bag, fallback.open_bag),
                open_atlas: pick(l.open_atlas, fallback.open_atlas),
                open_exchange: pick(l.open_exchange, fallback.open_exchange),
                open_room: pick(l.open_room, fallback.open_room),
                hide_pet: pick(l.hide_pet, fallback.hide_pet),
                show_pet: pick(l.show_pet, fallback.show_pet),
                tray_open_settings: pick(l.tray_open_settings, fallback.tray_open_settings),
                dock_settings: pick(l.dock_settings, fallback.dock_settings),
                window_size: pick(l.window_size, fallback.window_size),
                window_passthrough: pick(l.window_passthrough, fallback.window_passthrough),
                game_mode: pick(l.game_mode, fallback.game_mode),
                window_opacity: pick(l.window_opacity, fallback.window_opacity),
                restart: pick(l.restart, fallback.restart),
                quit: pick(l.quit, fallback.quit),
                tray_tooltip: pick(l.tray_tooltip, fallback.tray_tooltip),
            };
        }
        if let Some(v) = pet_dock_visible {
            m.pet_dock_visible = v;
        }
        if let Some(v) = pet_scale {
            m.pet_scale = v.round() as i64;
        }
        if let Some(v) = window_opacity {
            m.window_opacity = v.round() as i64;
        }
        if let Some(v) = window_passthrough {
            m.window_passthrough = v;
        }
        if let Some(v) = game_mode {
            m.game_mode = v;
        }
    }
    menu::refresh_tray(&app);
}

// ---------------------------------------------------------------- updater

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    rid: tauri::ResourceId,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

const API_HOST: &str = "https://api-wutheringwaves-wuwatappo.kurogames-global.com";

async fn fetch_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("{url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("{url}: {e}"))?
        .json()
        .await
        .map_err(|e| format!("{url}: invalid json: {e}"))
}

/// Resolve the updater manifest the same way the vendor app does:
/// activity-config → `assets_host` → `latest-global_host.json` → `url`.
/// The endpoint the frontend passes is normally the last hop already; if it
/// is missing or unreachable we walk the chain ourselves instead of relying
/// on a hard-coded URL (the one baked into the original binary is dead).
async fn resolve_manifest_url(
    client: &reqwest::Client,
    hint: &str,
) -> Result<tauri::Url, String> {
    if let Ok(url) = tauri::Url::parse(hint.trim()) {
        if url.scheme().starts_with("http") {
            return Ok(url);
        }
    }
    let cfg = fetch_json(client, &format!("{API_HOST}/api/common/activity-config")).await?;
    let host = cfg
        .pointer("/data/assets_host")
        .and_then(|v| v.as_str())
        .ok_or("activity-config has no assets_host")?
        .trim_end_matches('/')
        .to_string();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let host_json = fetch_json(client, &format!("{host}/app/json-host/latest-global_host.json?v={ts}")).await?;
    let url = host_json
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("latest-global_host.json has no url")?;
    tauri::Url::parse(url).map_err(|e| format!("manifest url: {e}"))
}

/// The vendor manifest is a Tauri updater manifest except that `notes` is a
/// locale→text map, which `tauri-plugin-updater` refuses to deserialize. We
/// fetch and compare versions ourselves and only hand off to the plugin (for
/// the signed download/install path) when the manifest actually carries a
/// macOS artifact. Without one there is nothing installable, so the app is
/// reported as up to date (the Windows-only installer URL would otherwise
/// be offered to the user).
#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    webview: tauri::Webview,
    endpoint: Option<String>,
) -> Result<Option<UpdateMetadata>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let url = resolve_manifest_url(&client, endpoint.as_deref().unwrap_or("")).await?;
    let raw = match fetch_json(&client, url.as_str()).await {
        Ok(v) => v,
        Err(e) if endpoint.as_deref().map_or(false, |h| !h.trim().is_empty()) => {
            // The frontend's hint was stale; re-resolve from the live chain.
            log::warn!("[UPDATE] manifest hint failed ({e}); resolving via activity-config");
            let url = resolve_manifest_url(&client, "").await?;
            fetch_json(&client, url.as_str()).await?
        }
        Err(e) => return Err(e),
    };

    let current_version = app.package_info().version.to_string();
    let remote_version = raw
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches('v').to_string())
        .ok_or_else(|| "manifest missing version".to_string())?;
    let newer = match (
        semver::Version::parse(&remote_version),
        semver::Version::parse(&current_version),
    ) {
        (Ok(r), Ok(c)) => r > c,
        _ => remote_version != current_version,
    };
    if !newer {
        log::info!("[UPDATE] up to date current={current_version} remote={remote_version}");
        return Ok(None);
    }

    let target = tauri_plugin_updater::target().unwrap_or_else(|| "darwin-aarch64".into());
    let has_mac_artifact = raw
        .get("platforms")
        .and_then(|p| p.as_object())
        .map(|p| p.contains_key(&target) || p.contains_key("darwin-universal"))
        .unwrap_or(false);
    if !has_mac_artifact {
        log::info!(
            "[UPDATE] remote={remote_version} has no `{target}` artifact; nothing installable on macOS"
        );
        return Ok(None);
    }

    // Hand the signed download/install flow to the plugin.
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let date = update
                .date
                .and_then(|d| d.format(&time::format_description::well_known::Rfc3339).ok());
            let meta = UpdateMetadata {
                rid: 0,
                current_version: update.current_version.clone(),
                version: update.version.clone(),
                date,
                body: update.body.clone(),
                raw_json: raw.clone(),
            };
            let rid = webview.resources_table().add(update);
            log::info!("[UPDATE] available version={} rid={rid}", meta.version);
            Ok(Some(UpdateMetadata { rid, ..meta }))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn is_force_update_locked(state: State<'_, AppState>) -> bool {
    state.input.lock().unwrap().force_update_locked
}

#[tauri::command]
pub async fn download_and_open_installer(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- autostart (Windows-only API surface)

#[tauri::command]
pub fn is_elevated_autostart_enabled(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_elevated_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    if enabled { al.enable() } else { al.disable() }.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- lifecycle

#[tauri::command]
pub async fn exit_app(_app: AppHandle) {
    log::info!("[APP] exit requested pid={}", std::process::id());
    // Skip Tauri/tao teardown: the windows are re-classed NSPanels whose
    // dealloc path raises; the frontend has already flushed its state.
    std::process::exit(0);
}
