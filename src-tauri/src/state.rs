use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_id: Option<i64>,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left && y >= self.top && x < self.left + self.width && y < self.top + self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    Default,
    Pointer,
    Grab,
}

impl CursorKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "pointer" => CursorKind::Pointer,
            "grab" => CursorKind::Grab,
            _ => CursorKind::Default,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Binding {
    pub modifiers: u32,
    pub key: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppShortcuts {
    pub game_mode: Binding,
    pub switch_screen: Binding,
    pub toggle_pet: Binding,
}

impl Default for AppShortcuts {
    fn default() -> Self {
        Self {
            game_mode: Binding { modifiers: 0, key: 118 },
            switch_screen: Binding { modifiers: 0, key: 119 },
            toggle_pet: Binding { modifiers: 0, key: 120 },
        }
    }
}

/// State shared with the global input tap thread. Kept tiny and lock-cheap.
#[derive(Debug, Default)]
pub struct InputState {
    /// Input regions in logical px relative to the main window's top-left.
    pub input_regions: Vec<Rect>,
    /// Full passthrough (game mode): never capture, only observe.
    pub window_passthrough: bool,
    /// Frontend is handling a gesture; keep capturing until released.
    pub swallow: bool,
    /// Main window origin (logical, top-left, global) and scale factor.
    pub origin_x: f64,
    pub origin_y: f64,
    pub scale: f64,
    /// Last known pointer position (logical global).
    pub pointer_x: f64,
    pub pointer_y: f64,
    /// Whether the main window currently captures mouse events.
    pub capturing: bool,
    pub cursor: Option<CursorKind>,
    pub shortcuts: AppShortcuts,
    pub shortcut_capture: bool,
    pub force_update_locked: bool,
    /// Keys currently held (by keycode), to ignore autorepeat.
    pub held_keys: HashSet<u16>,
    pub last_flags: u64,
    pub last_move_emit_ns: u128,
    pub tap_alive: bool,
    /// CGWindowIDs of the overlay windows (main, toast): clicks over any
    /// *other* window of ours (panel, dialogs) are handled by that window's
    /// DOM and must not be counted again.
    pub overlay_window_numbers: Vec<i64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeMenuLabels {
    #[serde(default)]
    pub open_bag: String,
    #[serde(default)]
    pub open_atlas: String,
    #[serde(default)]
    pub open_exchange: String,
    #[serde(default)]
    pub open_room: String,
    #[serde(default)]
    pub hide_pet: String,
    #[serde(default)]
    pub show_pet: String,
    #[serde(default)]
    pub tray_open_settings: String,
    #[serde(default)]
    pub dock_settings: String,
    #[serde(default)]
    pub window_size: String,
    #[serde(default)]
    pub window_passthrough: String,
    #[serde(default)]
    pub game_mode: String,
    #[serde(default)]
    pub window_opacity: String,
    #[serde(default)]
    pub restart: String,
    #[serde(default)]
    pub quit: String,
    #[serde(default)]
    pub tray_tooltip: String,
}

impl NativeMenuLabels {
    /// Neutral fallbacks; the frontend always supplies localized labels via
    /// `sync_native_menu` before the menu is shown.
    pub fn english() -> Self {
        Self {
            open_bag: "Bag".into(),
            open_atlas: "Atlas".into(),
            open_exchange: "Exchange".into(),
            open_room: "Rooms".into(),
            hide_pet: "Hide Pet".into(),
            show_pet: "Show Pet".into(),
            tray_open_settings: "Settings".into(),
            dock_settings: "Settings".into(),
            window_size: "Pet Size".into(),
            window_passthrough: "Disable Dragging".into(),
            game_mode: "Game Mode".into(),
            window_opacity: "Pet Opacity".into(),
            restart: "Restart".into(),
            quit: "Quit".into(),
            tray_tooltip: "Wuwa Tappo".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MenuState {
    pub labels: NativeMenuLabels,
    pub pet_dock_visible: bool,
    pub pet_scale: i64,
    pub window_opacity: i64,
    pub window_passthrough: bool,
    pub game_mode: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            labels: NativeMenuLabels::english(),
            pet_dock_visible: true,
            pet_scale: 100,
            window_opacity: 100,
            window_passthrough: false,
            game_mode: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct UiState {
    pub menu: MenuState,
    pub update_detail: Option<serde_json::Value>,
    pub maintenance_payload: Option<serde_json::Value>,
    pub has_maintenance: bool,
    pub login_intent: Option<String>,
    /// (label, event) pairs whose listener is registered in a live window.
    pub ready: HashSet<(String, String)>,
    /// Pending payloads to deliver once a window registers a listener.
    pub pending: HashMap<(String, String), serde_json::Value>,
    pub panel_collapsed: bool,
    pub monitor_switch_in_progress: bool,
    pub main_hidden_for_maintenance: bool,
    pub last_foreground_fullscreen: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub input: Arc<Mutex<InputState>>,
    pub ui: Arc<Mutex<UiState>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            input: Arc::new(Mutex::new(InputState {
                scale: 1.0,
                ..Default::default()
            })),
            ui: Arc::new(Mutex::new(UiState::default())),
        }
    }
}
