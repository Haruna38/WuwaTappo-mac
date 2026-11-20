//! Small JSON files under ~/Library/Application Support/<identifier>/.

use std::path::PathBuf;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::state::AppShortcuts;

fn data_dir(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("wuwatappo"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn read_json<T: DeserializeOwned>(app: &AppHandle, name: &str) -> Option<T> {
    let path = data_dir(app).join(name);
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_json<T: Serialize>(app: &AppHandle, name: &str, value: &T) -> Result<(), String> {
    let dir = data_dir(app);
    let path = dir.join(name);
    let tmp = dir.join(format!("{name}.tmp"));
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct KeyPressStore {
    count: u64,
    environment: String,
}

pub fn load_key_press_count(app: &AppHandle, environment: &str) -> u64 {
    let name = format!("key_press_count_{}.json", sanitize(environment));
    match read_json::<KeyPressStore>(app, &name) {
        Some(s) if s.environment == environment => s.count,
        Some(_) => {
            log::warn!("[STORAGE] event=load_fallback store=key_press reason_code=environment_mismatch");
            0
        }
        None => 0,
    }
}

pub fn save_key_press_count(app: &AppHandle, environment: &str, count: u64) -> Result<(), String> {
    let name = format!("key_press_count_{}.json", sanitize(environment));
    write_json(
        app,
        &name,
        &KeyPressStore {
            count,
            environment: environment.to_string(),
        },
    )
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CumulativeClickStore {
    pub count: u64,
    pub cuid: String,
    pub environment: String,
}

pub fn load_cumulative_click(app: &AppHandle, environment: &str) -> CumulativeClickStore {
    let name = format!("cumulative_click_{}.json", sanitize(environment));
    match read_json::<CumulativeClickStore>(app, &name) {
        Some(s) if s.environment == environment => s,
        Some(_) => {
            log::warn!(
                "[STORAGE] event=load_fallback store=cumulative_click reason_code=environment_mismatch"
            );
            CumulativeClickStore {
                environment: environment.to_string(),
                ..Default::default()
            }
        }
        None => CumulativeClickStore {
            environment: environment.to_string(),
            ..Default::default()
        },
    }
}

pub fn save_cumulative_click(app: &AppHandle, store: &CumulativeClickStore) -> Result<(), String> {
    let name = format!("cumulative_click_{}.json", sanitize(&store.environment));
    write_json(app, &name, store)
}

pub fn load_shortcuts(app: &AppHandle) -> AppShortcuts {
    read_json::<AppShortcuts>(app, "shortcuts.json").unwrap_or_default()
}

pub fn save_shortcuts(app: &AppHandle, sc: &AppShortcuts) -> Result<(), String> {
    write_json(app, "shortcuts.json", sc)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocaleStore {
    locale: String,
}

pub fn load_ui_locale(app: &AppHandle) -> Option<String> {
    read_json::<LocaleStore>(app, "ui_locale.json")
        .map(|s| s.locale)
        .filter(|l| !l.is_empty())
}

pub fn save_ui_locale(app: &AppHandle, locale: &str) -> Result<(), String> {
    write_json(
        app,
        "ui_locale.json",
        &LocaleStore {
            locale: locale.to_string(),
        },
    )
}

pub fn remove_cumulative_click(app: &AppHandle, environment: &str) {
    let name = format!("cumulative_click_{}.json", sanitize(environment));
    let _ = std::fs::remove_file(data_dir(app).join(name));
}

pub fn remove_shortcuts(app: &AppHandle) {
    let _ = std::fs::remove_file(data_dir(app).join("shortcuts.json"));
}

/// Click totals preserved across "Reset App", keyed by account id. The
/// reset wipes the WebView profile and the cumulative store; the frontend
/// asks for this after the next login so no clicks are lost.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ResetClickArchive {
    environment: String,
    counts: std::collections::BTreeMap<String, u64>,
}

fn reset_archive_name(environment: &str) -> String {
    format!("reset_click_archive_{}.json", sanitize(environment))
}

pub fn load_reset_click_archive(app: &AppHandle, environment: &str, cuid: &str) -> u64 {
    read_json::<ResetClickArchive>(app, &reset_archive_name(environment))
        .filter(|a| a.environment == environment)
        .and_then(|a| a.counts.get(cuid).copied())
        .unwrap_or(0)
}

pub fn save_reset_click_archive(
    app: &AppHandle,
    environment: &str,
    cuid: &str,
    count: u64,
) -> Result<(), String> {
    let name = reset_archive_name(environment);
    let mut archive = read_json::<ResetClickArchive>(app, &name)
        .filter(|a| a.environment == environment)
        .unwrap_or_default();
    archive.environment = environment.to_string();
    let entry = archive.counts.entry(cuid.to_string()).or_insert(0);
    *entry = (*entry).max(count);
    write_json(app, &name, &archive)
}

const PENDING_RESET_MARKER: &str = "pending_reset";

/// Ask the next launch to wipe the WebView profile before any webview exists.
pub fn mark_pending_reset(app: &AppHandle) -> Result<(), String> {
    std::fs::write(data_dir(app).join(PENDING_RESET_MARKER), b"1").map_err(|e| e.to_string())
}

pub fn take_pending_reset(app: &AppHandle) -> bool {
    let path = data_dir(app).join(PENDING_RESET_MARKER);
    path.exists() && std::fs::remove_file(path).is_ok()
}
