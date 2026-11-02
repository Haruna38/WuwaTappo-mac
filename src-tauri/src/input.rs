//! Global keyboard/mouse observation via a listen-only CGEventTap.
//!
//! The pet overlay ignores the cursor; every interaction the frontend sees
//! comes from here as Tauri events (`mouse-move`, `mouse-button`,
//! `key-press`, `key-release`, `app-shortcut`). The tap also decides when
//! the overlay must stop ignoring mouse events (pointer over an input
//! region, or the frontend asked to swallow a gesture) so clicks on the pet
//! do not fall through to the app underneath.
//!
//! Requires the Input Monitoring permission (System Settings → Privacy &
//! Security). Without it the tap cannot be created; we retry every few
//! seconds so granting it at runtime works without a restart.

use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventField, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventType, EventField,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::keys;
use crate::macos;
use crate::state::{AppState, CursorKind};

static TAP_PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

extern "C" {
    fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MouseMovePayload {
    x: f64,
    y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    member_id: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MouseButtonPayload {
    button: &'static str,
    action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    member_id: Option<i64>,
}

const MOVE_EMIT_INTERVAL: Duration = Duration::from_millis(8);

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("wuwa-input-tap".into())
        .spawn(move || loop {
            match run_tap(app.clone()) {
                Ok(()) => {
                    log::warn!("[input] event tap run loop exited; restarting");
                }
                Err(()) => {
                    log::warn!(
                        "[input] could not create event tap (Input Monitoring permission missing?); retrying in 3s"
                    );
                    if let Some(state) = app.try_state::<AppState>() {
                        state.input.lock().unwrap().tap_alive = false;
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        })
        .expect("spawn input thread");
}

fn run_tap(app: AppHandle) -> Result<(), ()> {
    let types = vec![
        CGEventType::LeftMouseDown,
        CGEventType::LeftMouseUp,
        CGEventType::RightMouseDown,
        CGEventType::RightMouseUp,
        CGEventType::OtherMouseDown,
        CGEventType::OtherMouseUp,
        CGEventType::MouseMoved,
        CGEventType::LeftMouseDragged,
        CGEventType::RightMouseDragged,
        CGEventType::OtherMouseDragged,
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
    ];
    let app_cb = app.clone();
    let start = Instant::now();
    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        types,
        move |_proxy, ty, ev| {
            handle_event(&app_cb, ty, ev, &start);
            None
        },
    )?;
    let source = tap.mach_port.create_runloop_source(0).map_err(|_| ())?;
    TAP_PORT.store(
        tap.mach_port.as_concrete_TypeRef() as *mut std::ffi::c_void,
        Ordering::Relaxed,
    );
    let rl = CFRunLoop::get_current();
    rl.add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    if let Some(state) = app.try_state::<AppState>() {
        state.input.lock().unwrap().tap_alive = true;
    }
    log::info!("[input] global event tap active");
    CFRunLoop::run_current();
    Ok(())
}

fn re_enable_tap() {
    let p = TAP_PORT.load(Ordering::Relaxed);
    if !p.is_null() {
        unsafe { CGEventTapEnable(p, true) };
    }
}

fn button_name(ev: &CGEvent, ty: CGEventType) -> &'static str {
    match ty {
        CGEventType::LeftMouseDown | CGEventType::LeftMouseUp => "left",
        CGEventType::RightMouseDown | CGEventType::RightMouseUp => "right",
        _ => match ev.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER as CGEventField) {
            2 => "middle",
            3 => "back",
            4 => "forward",
            _ => "other",
        },
    }
}

static FIRST_MOUSE: std::sync::Once = std::sync::Once::new();
static FIRST_KEY: std::sync::Once = std::sync::Once::new();

fn handle_event(app: &AppHandle, ty: CGEventType, ev: &CGEvent, start: &Instant) {
    let ty_u32 = ty as u32;
    match ty {
        CGEventType::KeyDown => FIRST_KEY.call_once(|| log::info!("[input] first key event received")),
        CGEventType::LeftMouseDown => {
            FIRST_MOUSE.call_once(|| log::info!("[input] first mouse click received"))
        }
        _ => {}
    }
    // kCGEventTapDisabledByTimeout / ByUserInput
    if ty_u32 == 0xFFFF_FFFE || ty_u32 == 0xFFFF_FFFF {
        re_enable_tap();
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    match ty {
        CGEventType::MouseMoved
        | CGEventType::LeftMouseDragged
        | CGEventType::RightMouseDragged
        | CGEventType::OtherMouseDragged => {
            let loc = ev.location();
            let (member, emit, px, py) = {
                let mut s = state.input.lock().unwrap();
                s.pointer_x = loc.x;
                s.pointer_y = loc.y;
                let now = start.elapsed().as_nanos();
                let emit = now.saturating_sub(s.last_move_emit_ns) >= MOVE_EMIT_INTERVAL.as_nanos();
                if emit {
                    s.last_move_emit_ns = now;
                }
                let member = evaluate_capture_locked(app, &mut s);
                (member, emit, loc.x * s.scale, loc.y * s.scale)
            };
            if emit {
                let _ = app.emit(
                    "mouse-move",
                    MouseMovePayload {
                        x: px,
                        y: py,
                        member_id: member,
                    },
                );
            }
        }
        CGEventType::LeftMouseDown
        | CGEventType::RightMouseDown
        | CGEventType::OtherMouseDown
        | CGEventType::LeftMouseUp
        | CGEventType::RightMouseUp
        | CGEventType::OtherMouseUp => {
            let loc = ev.location();
            let action = match ty {
                CGEventType::LeftMouseDown
                | CGEventType::RightMouseDown
                | CGEventType::OtherMouseDown => "press",
                _ => "release",
            };
            let (member, capturing, overlays) = {
                let mut s = state.input.lock().unwrap();
                s.pointer_x = loc.x;
                s.pointer_y = loc.y;
                let member = evaluate_capture_locked(app, &mut s);
                (member, s.capturing, s.overlay_window_numbers.clone())
            };
            let button = button_name(ev, ty);
            log::debug!("[input] mouse-button {button} {action} at ({:.0},{:.0}) member={member:?} capturing={capturing}", loc.x, loc.y);
            // Windows-build semantics: `mouse-button` is routed input on the
            // pet overlay; a press anywhere else is a bare `global-clap-mouse`
            // (counted) unless it lands on one of our own windows, whose DOM
            // already counts it (panel `pet-clap`). Releases always go out so
            // drags end correctly.
            if capturing || action == "release" {
                let _ = app.emit(
                    "mouse-button",
                    MouseButtonPayload {
                        button,
                        action,
                        member_id: member,
                    },
                );
            } else if !macos::own_window_at_point(loc.x, loc.y, &overlays) {
                let _ = app.emit("global-clap-mouse", ());
            }
        }
        CGEventType::KeyDown => {
            let code = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE as CGEventField) as u16;
            let autorepeat =
                ev.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT as CGEventField) != 0;
            let flags = ev.get_flags().bits();
            let shortcut = {
                let mut s = state.input.lock().unwrap();
                if autorepeat || !s.held_keys.insert(code) {
                    return;
                }
                if s.shortcut_capture || s.force_update_locked {
                    None
                } else {
                    match_shortcut(&s.shortcuts, code, keys::flags_to_modifiers(flags))
                }
            };
            if let Some(action) = shortcut {
                let _ = app.emit("app-shortcut", action);
            }
            log::debug!("[input] key-press {}", keys::mac_key_name(code));
            let _ = app.emit("key-press", keys::mac_key_name(code));
        }
        CGEventType::KeyUp => {
            let code = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE as CGEventField) as u16;
            {
                let mut s = state.input.lock().unwrap();
                s.held_keys.remove(&code);
            }
            let _ = app.emit("key-release", keys::mac_key_name(code));
        }
        CGEventType::FlagsChanged => {
            let flags = ev.get_flags().bits();
            let prev = {
                let mut s = state.input.lock().unwrap();
                let prev = s.last_flags;
                s.last_flags = flags;
                prev
            };
            for (bit, name) in keys::MODIFIER_FLAG_NAMES {
                let was = prev & bit != 0;
                let now = flags & bit != 0;
                if !was && now {
                    let _ = app.emit("key-press", *name);
                } else if was && !now {
                    let _ = app.emit("key-release", *name);
                }
            }
        }
        _ => {}
    }
}

fn match_shortcut(sc: &crate::state::AppShortcuts, code: u16, mods: u32) -> Option<&'static str> {
    let check = |b: &crate::state::Binding| -> bool {
        b.key != 0 && keys::vk_to_mac(b.key) == Some(code) && b.modifiers == mods
    };
    if check(&sc.toggle_pet) {
        Some("togglePet")
    } else if check(&sc.game_mode) {
        Some("gameMode")
    } else if check(&sc.switch_screen) {
        Some("switchScreen")
    } else {
        None
    }
}

/// Decide whether the overlay should capture mouse events at the current
/// pointer position; apply the change on the main thread when it flips.
/// Returns the memberId of the input region under the pointer, if any.
pub fn evaluate_capture_locked(app: &AppHandle, s: &mut crate::state::InputState) -> Option<i64> {
    let lx = s.pointer_x - s.origin_x;
    let ly = s.pointer_y - s.origin_y;
    let mut member = None;
    let mut inside = false;
    if !s.window_passthrough {
        for r in &s.input_regions {
            if r.contains(lx, ly) {
                inside = true;
                if r.member_id.is_some() {
                    member = r.member_id;
                    break;
                }
            }
        }
    }
    let want = inside || (s.swallow && !s.window_passthrough);
    if want != s.capturing {
        log::debug!(
            "[hit] capture={want} pointer=({:.0},{:.0}) local=({lx:.0},{ly:.0}) inside={inside} swallow={}",
            s.pointer_x, s.pointer_y, s.swallow
        );
        s.capturing = want;
        let cursor = s.cursor.unwrap_or(CursorKind::Default);
        apply_capture(app, want, cursor);
    }
    if inside {
        member
    } else {
        None
    }
}

pub fn apply_capture(app: &AppHandle, capture: bool, cursor: CursorKind) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let Some(win) = app.get_webview_window("main") else {
            return;
        };
        let Ok(ptr) = win.ns_window() else {
            return;
        };
        macos::set_ignores_mouse(ptr, !capture);
        if capture {
            macos::set_cursor(ptr, cursor, true);
        } else {
            macos::set_cursor(ptr, CursorKind::Default, false);
            macos::reset_cursor_to_arrow();
        }
    });
}

/// Re-run the capture decision with the last known pointer (after region
/// or geometry updates).
pub fn reevaluate(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut s = state.input.lock().unwrap();
    let _ = evaluate_capture_locked(app, &mut s);
}
