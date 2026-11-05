//! Status-bar (tray) menu and the pet's right-click context menu. Both are
//! built from the same `MenuState` the frontend pushes via `sync_native_menu`.

use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, MenuState};

pub const TRAY_ID: &str = "main-tray";
const PET_SCALES: [i64; 6] = [50, 75, 100, 125, 150, 200];
const OPACITIES: [i64; 4] = [25, 50, 75, 100];

pub fn build_menu(app: &AppHandle, st: &MenuState, dock: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let l = &st.labels;
    let toggle_label = if st.pet_dock_visible { &l.hide_pet } else { &l.show_pet };
    let settings_label = if dock { &l.dock_settings } else { &l.tray_open_settings };

    let size_items: Vec<CheckMenuItem<tauri::Wry>> = PET_SCALES
        .iter()
        .map(|v| {
            CheckMenuItem::with_id(
                app,
                format!("pet_scale_{v}"),
                format!("{v}%"),
                true,
                *v == st.pet_scale,
                None::<&str>,
            )
        })
        .collect::<Result<_, _>>()?;
    let size_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        size_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let size_menu = Submenu::with_id_and_items(app, "pet_scale_submenu", &l.window_size, true, &size_refs)?;

    let opacity_items: Vec<CheckMenuItem<tauri::Wry>> = OPACITIES
        .iter()
        .map(|v| {
            CheckMenuItem::with_id(
                app,
                format!("window_opacity_{v}"),
                format!("{v}%"),
                true,
                *v == st.window_opacity,
                None::<&str>,
            )
        })
        .collect::<Result<_, _>>()?;
    let opacity_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = opacity_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let opacity_menu =
        Submenu::with_id_and_items(app, "window_opacity_submenu", &l.window_opacity, true, &opacity_refs)?;

    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open_bag", &l.open_bag, true, None::<&str>)?,
            &MenuItem::with_id(app, "open_atlas", &l.open_atlas, true, None::<&str>)?,
            &MenuItem::with_id(app, "open_exchange", &l.open_exchange, true, None::<&str>)?,
            &MenuItem::with_id(app, "open_room", &l.open_room, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "toggle_pet", toggle_label, true, None::<&str>)?,
            &size_menu,
            &opacity_menu,
            &CheckMenuItem::with_id(
                app,
                "window_passthrough",
                &l.window_passthrough,
                true,
                st.window_passthrough,
                None::<&str>,
            )?,
            &CheckMenuItem::with_id(app, "game_mode", &l.game_mode, true, st.game_mode, None::<&str>)?,
            &MenuItem::with_id(app, "open_settings", settings_label, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "restart", &l.restart, true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", &l.quit, true, None::<&str>)?,
        ],
    )?;
    Ok(menu)
}

pub fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    log::info!("[menu] command id={id}");
    let emit = |name: &str, payload: serde_json::Value| {
        let _ = app.emit(name, payload);
    };
    match id {
        "open_bag" => emit("dock-open-panel", "bag".into()),
        "open_atlas" => emit("dock-open-panel", "atlas".into()),
        "open_exchange" => emit("dock-open-panel", "exchange".into()),
        "open_room" => emit("dock-open-panel", "room".into()),
        "open_settings" => emit("dock-open-panel", "setting".into()),
        "toggle_pet" => emit("tray-toggle-pet-visibility", serde_json::Value::Null),
        "window_passthrough" => emit("tray-toggle-window-passthrough", serde_json::Value::Null),
        "game_mode" => emit("tray-toggle-game-mode", serde_json::Value::Null),
        "restart" => emit("tray-restart", serde_json::Value::Null),
        "quit" => emit("tray-quit", serde_json::Value::Null),
        other => {
            if let Some(v) = other.strip_prefix("pet_scale_") {
                if let Ok(n) = v.parse::<i64>() {
                    emit("tray-set-pet-scale", n.into());
                }
            } else if let Some(v) = other.strip_prefix("window_opacity_") {
                if let Ok(n) = v.parse::<i64>() {
                    emit("tray-set-window-opacity", n.into());
                }
            }
        }
    }
    // Immediately refresh the tray so check marks are not stale.
    refresh_tray(app);
}

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let st = state.ui.lock().unwrap().menu.clone();
    let menu = build_menu(app, &st, false)?;
    // Menu events are handled once by `Builder::on_menu_event` (lib.rs);
    // `TrayIconBuilder::on_menu_event` registers into the same global list
    // and would dispatch every click twice (toggles cancel out).
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip(&st.labels.tray_tooltip);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone()).icon_as_template(false);
    }
    builder.build(app)?;
    Ok(())
}

pub fn refresh_tray(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let st = state.ui.lock().unwrap().menu.clone();
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app, &st, false) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
            let _ = tray.set_tooltip(Some(&st.labels.tray_tooltip));
        }
        Err(e) => log::error!("[menu] rebuild failed: {e}"),
    }
}
