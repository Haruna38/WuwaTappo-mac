//! Window creation and the window-related Tauri commands. The frontend never
//! creates windows itself; every label below is created here on demand.

use serde::Deserialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalSize, State,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::input;
use crate::macos::{self, LEVEL_AUX, LEVEL_MAIN, LEVEL_MODAL, LEVEL_PANEL, LEVEL_TOAST};
use crate::state::{AppState, CursorKind, Rect};

const INIT_SCRIPT: &str = include_str!("init.js");

pub const MAIN: &str = "main";
pub const TOAST: &str = "toast";
pub const PANEL: &str = "panel";
pub const LOGIN: &str = "login";
pub const UPDATE: &str = "update";
pub const MAINTENANCE: &str = "maintenance";
pub const ABOUT: &str = "about";
pub const OSS: &str = "oss-notice";

pub const PANEL_EXPANDED_W: f64 = 1100.0;
pub const PANEL_COLLAPSED_W: f64 = 958.0;
pub const PANEL_H: f64 = 760.0;

struct Spec {
    label: &'static str,
    route: String,
    width: f64,
    height: f64,
    level: isize,
    click_through: bool,
    resizable: bool,
    focusable: bool,
    min_size: Option<(f64, f64)>,
}

fn main_monitor(app: &AppHandle) -> Option<Monitor> {
    if let Some(main) = app.get_webview_window(MAIN) {
        if let Ok(Some(m)) = main.current_monitor() {
            return Some(m);
        }
    }
    if let Some((x, y)) = macos::mouse_location_top_left() {
        if let Ok(Some(m)) = app.monitor_from_point(x, y) {
            return Some(m);
        }
    }
    app.primary_monitor().ok().flatten()
}

fn center_on_main_monitor(app: &AppHandle, win: &WebviewWindow) {
    let Some(m) = main_monitor(app) else {
        let _ = win.center();
        return;
    };
    let Ok(size) = win.outer_size() else {
        return;
    };
    let mp = m.position();
    let ms = m.size();
    let x = mp.x + (ms.width as i32 - size.width as i32) / 2;
    let y = mp.y + (ms.height as i32 - size.height as i32) / 2;
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

fn ensure(app: &AppHandle, spec: Spec) -> tauri::Result<(WebviewWindow, bool)> {
    if let Some(w) = app.get_webview_window(spec.label) {
        return Ok((w, false));
    }
    let url = tauri::Url::parse(&format!("{}/{}", crate::server::ORIGIN, spec.route))
        .map_err(|e| tauri::Error::InvalidUrl(e))?;
    let mut b = WebviewWindowBuilder::new(app, spec.label, WebviewUrl::External(url))
        .title("Wuwa Tappo")
        .inner_size(spec.width, spec.height)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(spec.resizable)
        .maximizable(false)
        .minimizable(false)
        .visible(false)
        .focusable(spec.focusable)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .accept_first_mouse(true)
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .initialization_script(INIT_SCRIPT);
    if let Some((w, h)) = spec.min_size {
        b = b.min_inner_size(w, h);
    }
    let label = spec.label.to_string();
    let app2 = app.clone();
    let win = b.build()?;
    win.on_window_event(move |ev| {
        if let WindowEvent::Destroyed = ev {
            if let Some(state) = app2.try_state::<AppState>() {
                let mut ui = state.ui.lock().unwrap();
                ui.ready.retain(|(l, _)| l != &label);
                ui.pending.retain(|(l, _), _| l != &label);
            }
            if label == PANEL {
                let _ = app2.emit("cat-panel-window-closed", ());
            }
        }
    });
    let ptr = win.ns_window()? as usize;
    let level = spec.level;
    let click_through = spec.click_through;
    let focusable = spec.focusable;
    let is_main = spec.label == MAIN;
    let is_overlay = is_main || spec.label == TOAST;
    let app3 = app.clone();
    win.run_on_main_thread(move || {
        let ptr = ptr as *mut std::ffi::c_void;
        macos::convert_to_panel(ptr, focusable);
        macos::configure_overlay(ptr, level, click_through, false);
        if is_main {
            macos::install_sink_view(ptr);
        }
        if is_overlay {
            if let Some(state) = app3.try_state::<AppState>() {
                state
                    .input
                    .lock()
                    .unwrap()
                    .overlay_window_numbers
                    .push(macos::window_number(ptr));
            }
        }
    })?;
    Ok((win, true))
}

fn full_monitor_bounds(app: &AppHandle) -> Option<(PhysicalPosition<i32>, PhysicalSize<u32>, f64)> {
    let m = main_monitor(app)?;
    Some((*m.position(), *m.size(), m.scale_factor()))
}

/// Cover the main monitor with the given overlay window.
fn cover_monitor(app: &AppHandle, win: &WebviewWindow) -> tauri::Result<()> {
    let Some((pos, size, scale)) = full_monitor_bounds(app) else {
        return Ok(());
    };
    let ptr = win.ns_window()? as usize;
    let (x, y, w, h) = (
        pos.x as f64 / scale,
        pos.y as f64 / scale,
        size.width as f64 / scale,
        size.height as f64 / scale,
    );
    win.run_on_main_thread(move || {
        macos::set_frame_top_left(ptr as *mut std::ffi::c_void, x, y, w, h);
    })?;
    Ok(())
}

pub fn refresh_main_geometry(app: &AppHandle) {
    let Some(main) = app.get_webview_window(MAIN) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let scale = main.scale_factor().unwrap_or(1.0);
    if let Ok(pos) = main.outer_position() {
        let mut s = state.input.lock().unwrap();
        s.origin_x = pos.x as f64 / scale;
        s.origin_y = pos.y as f64 / scale;
        s.scale = scale;
    }
}

pub fn create_main(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let (win, _) = ensure(
        app,
        Spec {
            label: MAIN,
            route: "index.html#/".into(),
            width: 800.0,
            height: 600.0,
            level: LEVEL_MAIN,
            click_through: true,
            resizable: false,
            focusable: false,
            min_size: None,
        },
    )?;
    cover_monitor(app, &win)?;
    win.show()?;
    if std::env::var_os("WUWA_DEVTOOLS").is_some() {
        win.open_devtools();
    }
    {
        let w = win.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(4));
            log::info!("[main] webview url={:?}", w.url().map(|u| u.to_string()));
            if std::env::var_os("WUWA_DEVTOOLS").is_some() {
                let _ = w.eval(r#"(function(){
                    var dbg=function(m){try{fetch('/__dbg?'+encodeURIComponent(m));}catch(e){}};
                    dbg('tauri='+(!!window.__TAURI_INTERNALS__)+' isTauri='+window.isTauri+' appLen='+(document.getElementById('app')||{}).innerHTML.length+' errs='+JSON.stringify(window.__wuwa_errs||[]));
                    try{window.__TAURI_INTERNALS__.invoke('get_track_os_info').then(function(r){dbg('invoke ok '+JSON.stringify(r))},function(e){dbg('invoke err '+String(e))});}catch(e){dbg('invoke throw '+String(e))}
                })();"#);
            }
        });
    }
    refresh_main_geometry(app);
    let ptr = win.ns_window()? as usize;
    let _ = win.run_on_main_thread(move || {
        log::info!("[main] native {}", macos::describe_window(ptr as *mut std::ffi::c_void));
    });
    Ok(win)
}

pub fn create_toast(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let (win, _) = ensure(
        app,
        Spec {
            label: TOAST,
            route: "index.html#/toast".into(),
            width: 800.0,
            height: 600.0,
            level: LEVEL_TOAST,
            click_through: true,
            resizable: false,
            focusable: false,
            min_size: None,
        },
    )?;
    cover_monitor(app, &win)?;
    Ok(win)
}

/// Emit `event` to `label` now if its listener is registered, otherwise queue
/// it for `listener_ready`.
pub fn deliver(app: &AppHandle, label: &str, event: &str, payload: serde_json::Value) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let ready = {
        let ui = state.ui.lock().unwrap();
        ui.ready.contains(&(label.to_string(), event.to_string()))
    };
    if ready {
        let _ = app.emit_to(label, event, payload);
    } else {
        let mut ui = state.ui.lock().unwrap();
        ui.pending.insert((label.to_string(), event.to_string()), payload);
    }
}

#[tauri::command]
pub fn listener_ready(app: AppHandle, window: tauri::Window, event: String) {
    let label = window.label().to_string();
    log::debug!("[ipc] listener ready window={label} event={event}");
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let pending = {
        let mut ui = state.ui.lock().unwrap();
        ui.ready.insert((label.clone(), event.clone()));
        ui.pending.remove(&(label.clone(), event.clone()))
    };
    if let Some(payload) = pending {
        let _ = app.emit_to(&label, &event, payload);
    }
}

/// Aux windows are hidden rather than destroyed: they are NSPanel
/// subclasses swapped in at runtime (see macos::convert_to_panel) and
/// tao's teardown path raises an ObjC exception for them; hiding is also
/// much faster to reopen.
fn hide_window(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.hide();
    }
}

fn show_centered(app: &AppHandle, win: &WebviewWindow, created: bool) -> tauri::Result<()> {
    if created {
        center_on_main_monitor(app, win);
    }
    win.show()?;
    let ptr = win.ns_window()? as usize;
    let _ = win.run_on_main_thread(move || macos::order_front(ptr as *mut std::ffi::c_void));
    let _ = win.set_focus();
    Ok(())
}

// ---------------------------------------------------------------- panel

#[derive(Deserialize)]
pub struct OpenPanelPayload {
    #[serde(default)]
    pub tab: Option<String>,
}

#[tauri::command]
pub async fn open_panel_window(app: AppHandle, payload: OpenPanelPayload) -> Result<(), String> {
    let tab = payload.tab.filter(|t| !t.is_empty());
    let route = match &tab {
        Some(t) => format!("index.html#/panel?tab={t}"),
        None => "index.html#/panel".to_string(),
    };
    let (win, created) = ensure(
        &app,
        Spec {
            label: PANEL,
            route,
            width: PANEL_EXPANDED_W,
            height: PANEL_H,
            level: LEVEL_PANEL,
            click_through: false,
            resizable: true,
            focusable: true,
            min_size: Some((PANEL_COLLAPSED_W, PANEL_H)),
        },
    )
    .map_err(|e| e.to_string())?;
    if created {
        let state = app.state::<AppState>();
        state.ui.lock().unwrap().panel_collapsed = false;
    } else {
        if let Some(t) = tab {
            deliver(&app, PANEL, "panel-open-tab", t.into());
        }
        // Reopen always starts expanded (frontend resets collapsed state).
        let state = app.state::<AppState>();
        let was_collapsed = std::mem::replace(&mut state.ui.lock().unwrap().panel_collapsed, false);
        if was_collapsed {
            let _ = win.set_min_size(Some(LogicalSize::new(PANEL_EXPANDED_W, PANEL_H)));
            let _ = win.set_size(LogicalSize::new(PANEL_EXPANDED_W, PANEL_H));
        }
    }
    show_centered(&app, &win, created).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn close_panel_window(app: AppHandle) -> Result<(), String> {
    hide_window(&app, PANEL);
    let _ = app.emit("cat-panel-window-closed", ());
    Ok(())
}

#[tauri::command]
pub async fn is_panel_window_open(app: AppHandle) -> bool {
    app.get_webview_window(PANEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn raise_panel_window_above_pets(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(PANEL) {
        let ptr = w.ns_window().map_err(|e| e.to_string())? as usize;
        let _ = w.run_on_main_thread(move || {
            let p = ptr as *mut std::ffi::c_void;
            macos::set_level(p, LEVEL_PANEL);
            macos::order_front(p);
        });
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CollapsedPayload {
    pub collapsed: bool,
}

#[tauri::command]
pub async fn set_panel_window_collapsed(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: CollapsedPayload,
) -> Result<(), String> {
    let Some(w) = app.get_webview_window(PANEL) else {
        return Ok(());
    };
    let prev = {
        let mut ui = state.ui.lock().unwrap();
        std::mem::replace(&mut ui.panel_collapsed, payload.collapsed)
    };
    if prev == payload.collapsed {
        return Ok(());
    }
    let scale = w.scale_factor().unwrap_or(1.0);
    let cur = w.inner_size().map_err(|e| e.to_string())?;
    let cur_w = cur.width as f64 / scale;
    let cur_h = cur.height as f64 / scale;
    let delta = PANEL_EXPANDED_W - PANEL_COLLAPSED_W;
    let new_w = if payload.collapsed { (cur_w - delta).max(PANEL_COLLAPSED_W) } else { cur_w + delta };
    let min_w = if payload.collapsed { PANEL_COLLAPSED_W } else { PANEL_EXPANDED_W };
    w.set_min_size(Some(LogicalSize::new(min_w, PANEL_H))).map_err(|e| e.to_string())?;
    w.set_size(LogicalSize::new(new_w, cur_h)).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------- login

#[tauri::command]
pub async fn open_login_window(
    app: AppHandle,
    intent: Option<String>,
    #[allow(unused_variables)] compact: Option<bool>,
) -> Result<(), String> {
    let intent = intent.unwrap_or_else(|| "login".to_string());
    let (win, created) = ensure(
        &app,
        Spec {
            label: LOGIN,
            route: "index.html#/login".into(),
            width: 520.0,
            height: 520.0,
            level: LEVEL_AUX,
            click_through: false,
            resizable: false,
            focusable: true,
            min_size: None,
        },
    )
    .map_err(|e| e.to_string())?;
    {
        let state = app.state::<AppState>();
        state.ui.lock().unwrap().login_intent = Some(intent.clone());
    }
    deliver(&app, LOGIN, "login-modal-open", serde_json::json!({ "intent": intent }));
    show_centered(&app, &win, created).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn close_login_window(app: AppHandle) -> Result<(), String> {
    hide_window(&app, LOGIN);
    Ok(())
}

#[derive(Deserialize)]
pub struct SizePayload {
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
pub async fn resize_login_window_to_content(app: AppHandle, payload: SizePayload) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(LOGIN) {
        if payload.width > 0.0 && payload.height > 0.0 {
            w.set_size(LogicalSize::new(payload.width, payload.height))
                .map_err(|e| e.to_string())?;
            center_on_main_monitor(&app, &w);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- update / maintenance

#[tauri::command]
pub async fn open_update_window(
    app: AppHandle,
    state: State<'_, AppState>,
    detail: serde_json::Value,
) -> Result<(), String> {
    state.ui.lock().unwrap().update_detail = Some(detail.clone());
    let (win, created) = ensure(
        &app,
        Spec {
            label: UPDATE,
            route: "index.html#/update".into(),
            width: 568.0,
            height: 620.0,
            level: LEVEL_MODAL,
            click_through: false,
            resizable: false,
            focusable: true,
            min_size: None,
        },
    )
    .map_err(|e| e.to_string())?;
    deliver(&app, UPDATE, "cat-app-update-available", detail);
    deliver(&app, UPDATE, "update-modal-open", serde_json::Value::Null);
    show_centered(&app, &win, created).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn close_update_window(app: AppHandle) -> Result<(), String> {
    hide_window(&app, UPDATE);
    Ok(())
}

#[tauri::command]
pub fn get_app_update_detail(state: State<'_, AppState>) -> serde_json::Value {
    state
        .ui
        .lock()
        .unwrap()
        .update_detail
        .clone()
        .unwrap_or(serde_json::Value::Null)
}

#[tauri::command]
pub async fn open_maintenance_notice(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: Option<serde_json::Value>,
) -> Result<(), String> {
    let payload = payload.unwrap_or(serde_json::Value::Null);
    {
        let mut ui = state.ui.lock().unwrap();
        ui.maintenance_payload = Some(payload.clone());
        ui.has_maintenance = true;
    }
    let (win, created) = ensure(
        &app,
        Spec {
            label: MAINTENANCE,
            route: "index.html#/update".into(),
            width: 568.0,
            height: 560.0,
            level: LEVEL_MODAL,
            click_through: false,
            resizable: false,
            focusable: true,
            min_size: None,
        },
    )
    .map_err(|e| e.to_string())?;
    deliver(&app, MAINTENANCE, "maintenance-notice-open", payload);
    show_centered(&app, &win, created).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_maintenance_notice_payload(state: State<'_, AppState>) -> serde_json::Value {
    state
        .ui
        .lock()
        .unwrap()
        .maintenance_payload
        .clone()
        .unwrap_or(serde_json::Value::Null)
}

#[tauri::command]
pub fn has_maintenance_notice(state: State<'_, AppState>) -> bool {
    state.ui.lock().unwrap().has_maintenance
}

#[tauri::command]
pub async fn hide_main_after_maintenance_notice_ready(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.ui.lock().unwrap().main_hidden_for_maintenance = true;
    for label in [MAIN, PANEL, LOGIN, ABOUT, OSS, UPDATE] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn exit_maintenance_app(_app: AppHandle) {
    log::info!("[APP] exit requested (maintenance)");
    std::process::exit(0);
}

// ---------------------------------------------------------------- about / oss

#[tauri::command]
pub async fn open_about_window(app: AppHandle) -> Result<(), String> {
    let (win, created) = ensure(
        &app,
        Spec {
            label: ABOUT,
            route: "index.html#/about".into(),
            width: 820.0,
            height: 720.0,
            level: LEVEL_AUX,
            click_through: false,
            resizable: false,
            focusable: true,
            min_size: None,
        },
    )
    .map_err(|e| e.to_string())?;
    deliver(&app, ABOUT, "about-open", serde_json::Value::Null);
    show_centered(&app, &win, created).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn close_about_window(app: AppHandle) -> Result<(), String> {
    hide_window(&app, ABOUT);
    Ok(())
}

#[tauri::command]
pub async fn open_oss_notice_window(app: AppHandle) -> Result<(), String> {
    let (win, created) = ensure(
        &app,
        Spec {
            label: OSS,
            route: "index.html#/oss-notice".into(),
            width: 820.0,
            height: 720.0,
            level: LEVEL_AUX,
            click_through: false,
            resizable: false,
            focusable: true,
            min_size: None,
        },
    )
    .map_err(|e| e.to_string())?;
    deliver(&app, OSS, "oss-notice-open", serde_json::Value::Null);
    show_centered(&app, &win, created).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn close_oss_notice_window(app: AppHandle) -> Result<(), String> {
    hide_window(&app, OSS);
    Ok(())
}

// ---------------------------------------------------------------- toast

#[tauri::command]
pub async fn show_toast_overlay(app: AppHandle) -> Result<(), String> {
    let win = create_toast(&app).map_err(|e| e.to_string())?;
    if !win.is_visible().unwrap_or(false) {
        win.show().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_toast_overlay(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(TOAST) {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------- main window geometry

#[tauri::command]
pub async fn set_main_window_fullscreen_shell(app: AppHandle) -> Result<(), String> {
    let main = app.get_webview_window(MAIN).ok_or("main window missing")?;
    cover_monitor(&app, &main).map_err(|e| e.to_string())?;
    if let Some(t) = app.get_webview_window(TOAST) {
        let _ = cover_monitor(&app, &t);
    }
    refresh_main_geometry(&app);
    input::reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub async fn sync_main_window_to_current_monitor(app: AppHandle) -> Result<(), String> {
    refresh_main_geometry(&app);
    input::reevaluate(&app);
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ApplyPositionPayload {
    #[serde(default)]
    pub always_on_top: Option<bool>,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub apply_position: Option<bool>,
}

#[tauri::command]
pub async fn apply_main_window_position(app: AppHandle, payload: ApplyPositionPayload) -> Result<(), String> {
    // Levels are managed natively (see macos::LEVEL_MAIN); tao's
    // set_always_on_top would drop the overlay to NSFloatingWindowLevel.
    if let Some(main) = app.get_webview_window(MAIN) {
        let ptr = main.ns_window().map_err(|e| e.to_string())? as usize;
        let on_top = payload.always_on_top.unwrap_or(true);
        let _ = main.run_on_main_thread(move || {
            macos::set_level(
                ptr as *mut std::ffi::c_void,
                if on_top { LEVEL_MAIN } else { 0 },
            );
        });
    }
    refresh_main_geometry(&app);
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ContentBoundsPayload {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    #[serde(default)]
    pub skip_passthrough_refresh: Option<bool>,
}

/// Unreachable in the 1.0.3 frontend (fullscreen shell short-circuits it);
/// kept so the invoke resolves if a future build calls it.
#[tauri::command]
pub async fn set_main_window_content_bounds(
    app: AppHandle,
    #[allow(unused_variables)] payload: ContentBoundsPayload,
) -> Result<(), String> {
    refresh_main_geometry(&app);
    Ok(())
}

#[derive(Deserialize)]
pub struct MonitorBoundsPayload {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[tauri::command]
pub async fn set_main_window_monitor_bounds(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: MonitorBoundsPayload,
) -> Result<(), String> {
    if payload.width <= 0.0 || payload.height <= 0.0 {
        return Err("invalid monitor bounds".into());
    }
    {
        let mut ui = state.ui.lock().unwrap();
        if ui.monitor_switch_in_progress {
            return Err("monitor switch already in progress".into());
        }
        ui.monitor_switch_in_progress = true;
    }
    let main = app.get_webview_window(MAIN).ok_or("main window missing")?;
    // Payload is physical px in Tauri's coordinate space; find the monitor to
    // get its scale factor, then set the frame in points.
    let scale = app
        .monitor_from_point(payload.x + 1.0, payload.y + 1.0)
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or_else(|| main.scale_factor().unwrap_or(1.0));
    let ptr = main.ns_window().map_err(|e| e.to_string())? as usize;
    let (x, y, w, h) = (
        payload.x / scale,
        payload.y / scale,
        payload.width / scale,
        payload.height / scale,
    );
    main.run_on_main_thread(move || {
        macos::set_frame_top_left(ptr as *mut std::ffi::c_void, x, y, w, h);
    })
    .map_err(|e| e.to_string())?;
    if let Some(t) = app.get_webview_window(TOAST) {
        let tptr = t.ns_window().map_err(|e| e.to_string())? as usize;
        let _ = t.run_on_main_thread(move || {
            macos::set_frame_top_left(tptr as *mut std::ffi::c_void, x, y, w, h);
        });
    }
    refresh_main_geometry(&app);
    Ok(())
}

#[tauri::command]
pub async fn sync_aux_windows_to_main_monitor(app: AppHandle) -> Result<(), String> {
    for label in [PANEL, LOGIN, UPDATE, MAINTENANCE, ABOUT, OSS] {
        if let Some(w) = app.get_webview_window(label) {
            center_on_main_monitor(&app, &w);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn finish_main_window_monitor_switch(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.ui.lock().unwrap().monitor_switch_in_progress = false;
    refresh_main_geometry(&app);
    input::reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub async fn get_main_desktop_safe_area(app: AppHandle) -> Result<macos::SafeArea, String> {
    let main = app.get_webview_window(MAIN).ok_or("main window missing")?;
    let ptr = main.ns_window().map_err(|e| e.to_string())? as usize;
    let (tx, rx) = std::sync::mpsc::channel();
    let app2 = app.clone();
    main.run_on_main_thread(move || {
        let p = ptr as *mut std::ffi::c_void;
        let insets = macos::screen_insets(p);
        let edge = macos::dock_auto_hide_edge();
        let fs = macos::foreground_app_fullscreen(p);
        if let Some(state) = app2.try_state::<AppState>() {
            let mut ui = state.ui.lock().unwrap();
            if ui.last_foreground_fullscreen != fs {
                ui.last_foreground_fullscreen = fs;
                log::info!(
                    "[desktop] foreground_fullscreen={fs} insets={insets:?} auto_hide_edge={edge:?} main={}",
                    macos::describe_window(p)
                );
            }
        }
        let _ = tx.send(macos::SafeArea {
            insets,
            auto_hide_edge: edge,
            foreground_fullscreen: fs,
            foreground_application_fullscreen: fs,
        });
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|e| format!("main thread did not answer: {e}"))
}

#[tauri::command]
pub async fn get_main_taskbar_insets(app: AppHandle) -> Result<macos::Insets, String> {
    let main = app.get_webview_window(MAIN).ok_or("main window missing")?;
    let ptr = main.ns_window().map_err(|e| e.to_string())? as usize;
    let (tx, rx) = std::sync::mpsc::channel();
    main.run_on_main_thread(move || {
        let _ = tx.send(macos::screen_insets(ptr as *mut std::ffi::c_void));
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|e| format!("main thread did not answer: {e}"))
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct PointPayload {
    pub x: f64,
    pub y: f64,
}

#[tauri::command]
pub async fn get_taskbar_insets_at_point(
    app: AppHandle,
    #[allow(unused_variables)] payload: Option<PointPayload>,
) -> Result<macos::Insets, String> {
    get_main_taskbar_insets(app).await
}

// ---------------------------------------------------------------- main window input

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct HitRegionPayload {
    #[serde(default)]
    pub regions: Vec<Rect>,
    #[serde(default)]
    pub input_regions: Vec<Rect>,
    #[serde(default)]
    pub update_native_input: bool,
    #[serde(default)]
    pub window_passthrough: bool,
    #[serde(default)]
    pub native_input: bool,
}

#[tauri::command]
pub fn set_main_hit_region(app: AppHandle, state: State<'_, AppState>, payload: HitRegionPayload) {
    refresh_main_geometry(&app);
    let mut s = state.input.lock().unwrap();
    if payload.update_native_input {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| log::info!("[hit] first native input regions received ({})", payload.input_regions.len()));
        log::debug!(
            "[hit] update_native_input regions={} passthrough={} first={:?} origin=({}, {}) scale={}",
            payload.input_regions.len(),
            payload.window_passthrough,
            payload.input_regions.first(),
            s.origin_x,
            s.origin_y,
            s.scale
        );
        s.input_regions = payload.input_regions;
    }
    if s.window_passthrough != payload.window_passthrough {
        log::debug!("[hit] window_passthrough={}", payload.window_passthrough);
    }
    s.window_passthrough = payload.window_passthrough;
    let _ = input::evaluate_capture_locked(&app, &mut s);
}

#[tauri::command]
pub fn set_main_force_cursor_ignore(app: AppHandle, state: State<'_, AppState>, enabled: bool) {
    let mut s = state.input.lock().unwrap();
    if enabled {
        s.swallow = false;
        let _ = input::evaluate_capture_locked(&app, &mut s);
    }
}

#[tauri::command]
pub fn set_main_swallow_mouse(app: AppHandle, state: State<'_, AppState>, enabled: bool) {
    log::debug!("[hit] swallow={enabled}");
    let mut s = state.input.lock().unwrap();
    s.swallow = enabled;
    let _ = input::evaluate_capture_locked(&app, &mut s);
}

#[tauri::command]
pub fn set_overlay_cursor(app: AppHandle, state: State<'_, AppState>, kind: String) {
    set_native_input_cursor(app, state, kind);
}

#[tauri::command]
pub fn set_native_input_cursor(app: AppHandle, state: State<'_, AppState>, kind: String) {
    let kind = CursorKind::parse(&kind);
    let capturing = {
        let mut s = state.input.lock().unwrap();
        if s.cursor == Some(kind) {
            return;
        }
        s.cursor = Some(kind);
        s.capturing
    };
    if capturing {
        if let Some(main) = app.get_webview_window(MAIN) {
            if let Ok(ptr) = main.ns_window() {
                let ptr = ptr as usize;
                let _ = main.run_on_main_thread(move || {
                    macos::set_cursor(ptr as *mut std::ffi::c_void, kind, true);
                });
            }
        }
    }
}

#[tauri::command]
pub async fn popup_main_dock_context_menu(
    app: AppHandle,
    state: State<'_, AppState>,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let main = app.get_webview_window(MAIN).ok_or("main window missing")?;
    let st = state.ui.lock().unwrap().menu.clone();
    let menu = crate::menu::build_menu(&app, &st, true).map_err(|e| e.to_string())?;
    log::info!("[menu] pre-popup x={x} y={y}");
    // Park the overlay below the NSMenu window level while the menu is open.
    let ptr = main.ns_window().map_err(|e| e.to_string())? as usize;
    let _ = main.run_on_main_thread(move || {
        macos::set_level(ptr as *mut std::ffi::c_void, macos::LEVEL_MENU_PARKED);
    });
    // Blocks until the menu is dismissed (NSMenu popup is synchronous).
    let r = main
        .popup_menu_at(&menu, tauri::LogicalPosition::new(x, y))
        .map_err(|e| e.to_string());
    let _ = main.run_on_main_thread(move || {
        macos::set_level(ptr as *mut std::ffi::c_void, LEVEL_MAIN);
    });
    log::info!("[menu] post-popup");
    r
}

#[tauri::command]
pub async fn set_main_skip_taskbar(app: AppHandle, skip: bool) -> Result<(), String> {
    app.set_dock_visibility(!skip).map_err(|e| e.to_string())
}
