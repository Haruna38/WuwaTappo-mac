//! Thin AppKit / CoreGraphics layer: window levels, click-through toggling,
//! an event-sink view that keeps WKWebView from seeing native mouse events,
//! cursor control, screen insets, Dock auto-hide and fullscreen detection.
//!
//! Everything here that touches AppKit objects must run on the main thread.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU8, Ordering};

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::geometry::CGRect;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly,
};
use objc2::rc::Retained;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly, Message};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::ClassType;
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSCursor, NSEvent, NSPanel, NSScreen, NSView,
    NSWindow, NSWindowCollectionBehavior, NSWindowOrderingMode, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSNumber, NSPoint, NSRect, NSString, NSUserDefaults};
use serde::Serialize;

use crate::state::CursorKind;

/// Window levels. NSScreenSaverWindowLevel+1 (1001) with
/// CanJoinAllSpaces|Stationary|FullScreenAuxiliary is what Bongo Cat's
/// TransparentWindow plugin uses so the pet stays visible over native
/// full-screen apps and games; NSFloatingWindowLevel (3) is not enough.
pub const LEVEL_MAIN: isize = 1001;
/// The panel ("tappo menu") and every dialog (about/login/oss/update) sit
/// just under the pet overlay so the pet is never covered by them; the
/// overlay is click-through outside the pet.
pub const LEVEL_PANEL: isize = 1000;
pub const LEVEL_AUX: isize = 1000;
pub const LEVEL_MODAL: isize = 1000;
/// Toasts share the pet's level (ordered after it, so they draw on top).
pub const LEVEL_TOAST: isize = 1001;
/// Level used while a context menu is open, so the NSMenu window (level 101)
/// is not covered by the overlay.
pub const LEVEL_MENU_PARKED: isize = 3;

static CURSOR_KIND: AtomicU8 = AtomicU8::new(0);

fn cursor_for(kind: CursorKind) -> Retained<NSCursor> {
    match kind {
        CursorKind::Default => NSCursor::arrowCursor(),
        CursorKind::Pointer => NSCursor::pointingHandCursor(),
        CursorKind::Grab => NSCursor::openHandCursor(),
    }
}

fn kind_from_u8(v: u8) -> CursorKind {
    match v {
        1 => CursorKind::Pointer,
        2 => CursorKind::Grab,
        _ => CursorKind::Default,
    }
}

fn kind_to_u8(k: CursorKind) -> u8 {
    match k {
        CursorKind::Default => 0,
        CursorKind::Pointer => 1,
        CursorKind::Grab => 2,
    }
}

define_class!(
    /// Transparent view layered above the WKWebView of the pet overlay.
    /// While the window accepts mouse events (pointer over an input region)
    /// this view swallows them so the page never sees DOM mouse events; the
    /// frontend routes clicks itself from the global event tap. It also
    /// prevents the click from activating/ordering the app, so the game
    /// underneath keeps keyboard focus.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "WuwaEventSinkView"]
    pub struct SinkView;

    impl SinkView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(shouldDelayWindowOrderingForEvent:))]
        fn should_delay_window_ordering(&self, _event: &NSEvent) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            let mtm = self.mtm();
            NSApplication::sharedApplication(mtm).preventWindowOrdering();
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {}

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {}

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            let mtm = self.mtm();
            NSApplication::sharedApplication(mtm).preventWindowOrdering();
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, _event: &NSEvent) {}

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, _event: &NSEvent) {}

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, _event: &NSEvent) {}

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, _event: &NSEvent) {}

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, _event: &NSEvent) {}

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, _event: &NSEvent) {}

        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            Some(Retained::into_super(self.retain()))
        }

        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            let kind = kind_from_u8(CURSOR_KIND.load(Ordering::Relaxed));
            self.addCursorRect_cursor(self.bounds(), &cursor_for(kind));
        }
    }
);

define_class!(
    /// NSPanel class the pet overlay and toast windows are converted to.
    /// Only a non-activating NSPanel is shown by macOS on other apps'
    /// full-screen Spaces (verified empirically: plain NSWindow, titled or
    /// borderless, is not). Never becomes key: input comes from the tap.
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "WuwaOverlayPanel"]
    pub struct OverlayPanel;

    impl OverlayPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            false
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

define_class!(
    /// NSPanel class for the panel/login/about/oss/update windows: same
    /// full-screen-Space behaviour as the overlay, but accepts keyboard
    /// focus (text fields) without activating the app.
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "WuwaAuxPanel"]
    pub struct AuxPanel;

    impl AuxPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

/// Re-class a tao-created NSWindow into one of our NSPanel subclasses.
///
/// Neither subclass declares instance variables, so the instance size does
/// not grow and the existing allocation (including tao's own ivars, which
/// tao keeps reading through its typed pointer) stays valid. Same technique
/// as the `tauri-nspanel` plugin.
pub fn convert_to_panel(ptr: *mut c_void, focusable: bool) {
    let obj: &AnyObject = unsafe { &*(ptr as *const AnyObject) };
    let cls: &AnyClass = if focusable { AuxPanel::class() } else { OverlayPanel::class() };
    unsafe { AnyObject::set_class(obj, cls) };
    let panel: &NSPanel = unsafe { &*(ptr as *const NSPanel) };
    let mask = panel.styleMask() | NSWindowStyleMask::NonactivatingPanel;
    panel.setStyleMask(mask);
    panel.setHidesOnDeactivate(false);
    panel.setFloatingPanel(false);
    panel.setBecomesKeyOnlyIfNeeded(!focusable);
}

/// # Safety
/// `ptr` must be a live NSWindow obtained from Tauri on the main thread.
pub unsafe fn window_ref<'a>(ptr: *mut c_void) -> &'a NSWindow {
    &*(ptr as *const NSWindow)
}

pub fn overlay_behavior() -> NSWindowCollectionBehavior {
    NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::Stationary
}

/// Configure a Tauri-created window as an overlay: level, Spaces behaviour,
/// shadow, click-through.
pub fn configure_overlay(ptr: *mut c_void, level: isize, click_through: bool, shadow: bool) {
    let w = unsafe { window_ref(ptr) };
    w.setLevel(level);
    w.setCollectionBehavior(overlay_behavior());
    w.setHasShadow(shadow);
    w.setIgnoresMouseEvents(click_through);
}

/// Diagnostic snapshot of the native window state.
pub fn describe_window(ptr: *mut c_void) -> String {
    let w = unsafe { window_ref(ptr) };
    format!(
        "level={} behavior={:#x} visible={} on_active_space={} ignores_mouse={} frame={:?}",
        w.level(),
        w.collectionBehavior().0,
        w.isVisible(),
        w.isOnActiveSpace(),
        w.ignoresMouseEvents(),
        {
            let f = w.frame();
            (f.origin.x, f.origin.y, f.size.width, f.size.height)
        }
    )
}

pub fn set_level(ptr: *mut c_void, level: isize) {
    unsafe { window_ref(ptr) }.setLevel(level);
}

pub fn set_ignores_mouse(ptr: *mut c_void, ignore: bool) {
    unsafe { window_ref(ptr) }.setIgnoresMouseEvents(ignore);
}

pub fn order_front(ptr: *mut c_void) {
    let w = unsafe { window_ref(ptr) };
    w.orderFrontRegardless();
}

/// Install the event-sink view above the webview of `ptr`.
pub fn install_sink_view(ptr: *mut c_void) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::error!("[macos] install_sink_view called off the main thread");
        return;
    };
    let w = unsafe { window_ref(ptr) };
    let Some(content) = w.contentView() else {
        return;
    };
    let frame = content.bounds();
    let view: Retained<SinkView> = unsafe { msg_send![SinkView::alloc(mtm), initWithFrame: frame] };
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Above, None);
}

/// Update the cursor shown while the pointer is over an input region.
pub fn set_cursor(ptr: *mut c_void, kind: CursorKind, apply_now: bool) {
    CURSOR_KIND.store(kind_to_u8(kind), Ordering::Relaxed);
    let w = unsafe { window_ref(ptr) };
    if let Some(content) = w.contentView() {
        w.invalidateCursorRectsForView(&content);
        for sub in content.subviews().iter() {
            w.invalidateCursorRectsForView(&sub);
        }
    }
    if apply_now {
        cursor_for(kind).set();
    }
}

pub fn reset_cursor_to_arrow() {
    NSCursor::arrowCursor().set();
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Insets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeArea {
    pub insets: Insets,
    pub auto_hide_edge: Option<String>,
    pub foreground_fullscreen: bool,
    pub foreground_application_fullscreen: bool,
}

fn screen_of_window(ptr: *mut c_void, mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
    let w = unsafe { window_ref(ptr) };
    w.screen().or_else(|| NSScreen::mainScreen(mtm))
}

/// Menu-bar / Dock insets of the screen hosting the window, in points.
pub fn screen_insets(ptr: *mut c_void) -> Insets {
    let Some(mtm) = MainThreadMarker::new() else {
        return Insets::default();
    };
    let Some(screen) = screen_of_window(ptr, mtm) else {
        return Insets::default();
    };
    let f = screen.frame();
    let v = screen.visibleFrame();
    Insets {
        left: (v.origin.x - f.origin.x).max(0.0),
        bottom: (v.origin.y - f.origin.y).max(0.0),
        right: ((f.origin.x + f.size.width) - (v.origin.x + v.size.width)).max(0.0),
        top: ((f.origin.y + f.size.height) - (v.origin.y + v.size.height)).max(0.0),
    }
}

/// Returns the Dock edge if the Dock is set to auto-hide.
pub fn dock_auto_hide_edge() -> Option<String> {
    let defaults = NSUserDefaults::standardUserDefaults();
    let domain: Option<Retained<NSDictionary<NSString, objc2::runtime::AnyObject>>> =
        defaults.persistentDomainForName(&NSString::from_str("com.apple.dock"));
    let dict = domain?;
    let autohide = dict
        .objectForKey(&NSString::from_str("autohide"))
        .and_then(|v| v.downcast::<NSNumber>().ok())
        .map(|n| n.boolValue())
        .unwrap_or(false);
    if !autohide {
        return None;
    }
    let orientation = dict
        .objectForKey(&NSString::from_str("orientation"))
        .and_then(|v| v.downcast::<NSString>().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "bottom".to_string());
    Some(match orientation.as_str() {
        "left" => "left",
        "right" => "right",
        _ => "bottom",
    }
    .to_string())
}

/// True when the frontmost application (not us) has a layer-0 window whose
/// bounds cover the screen hosting the pet overlay: native full-screen
/// Spaces and borderless full-screen games both match. Full-screen windows
/// on other displays are ignored, matching the Windows build's
/// "foreground window on the pet's monitor" semantics.
pub fn foreground_app_fullscreen(window: *mut c_void) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Some(target_screen) = screen_of_window(window, mtm) else {
        return false;
    };
    let ws = NSWorkspace::sharedWorkspace();
    let front_opt = ws.frontmostApplication();
    let Some(front) = front_opt else {
        return false;
    };
    let pid = front.processIdentifier();
    if pid == std::process::id() as i32 {
        return false;
    }
    let screens = NSScreen::screens(mtm);
    let Some(primary) = screens.iter().next() else {
        return false;
    };
    let primary_h = primary.frame().size.height;
    // The pet's screen frame in CG (top-left origin) coordinates.
    let f = target_screen.frame();
    let cg_screens = [CGRect::new(
        &core_graphics::geometry::CGPoint::new(f.origin.x, primary_h - (f.origin.y + f.size.height)),
        &core_graphics::geometry::CGSize::new(f.size.width, f.size.height),
    )];

    let Some(list) = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    ) else {
        return false;
    };
    let list: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CFArray::wrap_under_get_rule(list.as_concrete_TypeRef()) };
    let k_pid = CFString::from_static_string("kCGWindowOwnerPID");
    let k_layer = CFString::from_static_string("kCGWindowLayer");
    let k_bounds = CFString::from_static_string("kCGWindowBounds");
    for item in list.iter() {
        let owner = item
            .find(&k_pid)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        if owner != pid as i64 {
            continue;
        }
        let layer = item
            .find(&k_layer)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        if layer != 0 {
            continue;
        }
        let Some(bounds) = item
            .find(&k_bounds)
            .and_then(|v| v.downcast::<CFDictionary>())
        else {
            continue;
        };
        let bounds: CFDictionary =
            unsafe { CFDictionary::wrap_under_get_rule(bounds.as_concrete_TypeRef()) };
        let Some(rect) = CGRect::from_dict_representation(&bounds) else {
            continue;
        };
        for s in &cg_screens {
            if (rect.size.width - s.size.width).abs() <= 2.0
                && (rect.size.height - s.size.height).abs() <= 2.0
                && (rect.origin.x - s.origin.x).abs() <= 2.0
                && (rect.origin.y - s.origin.y).abs() <= 2.0
            {
                log::info!(
                    "[desktop] fullscreen window owner={} pid={pid} bounds={:?}",
                    front.localizedName().map(|n| n.to_string()).unwrap_or_default(),
                    (rect.origin.x, rect.origin.y, rect.size.width, rect.size.height)
                );
                return true;
            }
        }
    }
    false
}

/// Window number of an NSWindow (CGWindowID), for CGWindowList lookups.
pub fn window_number(ptr: *mut c_void) -> i64 {
    unsafe { window_ref(ptr) }.windowNumber() as i64
}

/// True when one of this process's on-screen windows, other than the ones
/// listed in `exclude`, contains the point (CG top-left coordinates).
/// Thread-safe (CoreGraphics only); used from the event tap so clicks on
/// the panel/dialogs are not double-counted by the pet.
pub fn own_window_at_point(x: f64, y: f64, exclude: &[i64]) -> bool {
    let pid = std::process::id() as i64;
    let Some(list) = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    ) else {
        return false;
    };
    let list: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CFArray::wrap_under_get_rule(list.as_concrete_TypeRef()) };
    let k_pid = CFString::from_static_string("kCGWindowOwnerPID");
    let k_num = CFString::from_static_string("kCGWindowNumber");
    let k_bounds = CFString::from_static_string("kCGWindowBounds");
    for item in list.iter() {
        let owner = item
            .find(&k_pid)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        if owner != pid {
            continue;
        }
        let num = item
            .find(&k_num)
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(-1);
        if exclude.contains(&num) {
            continue;
        }
        let Some(bounds) = item.find(&k_bounds).and_then(|v| v.downcast::<CFDictionary>()) else {
            continue;
        };
        let bounds: CFDictionary =
            unsafe { CFDictionary::wrap_under_get_rule(bounds.as_concrete_TypeRef()) };
        let Some(r) = CGRect::from_dict_representation(&bounds) else {
            continue;
        };
        if x >= r.origin.x
            && y >= r.origin.y
            && x < r.origin.x + r.size.width
            && y < r.origin.y + r.size.height
        {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Serialize)]
pub struct DisplayName {
    pub device_name: String,
    pub name: String,
}

pub fn display_names() -> Vec<DisplayName> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    NSScreen::screens(mtm)
        .iter()
        .map(|s| {
            let n = s.localizedName().to_string();
            DisplayName {
                device_name: n.clone(),
                name: n,
            }
        })
        .collect()
}

/// Current pointer position in logical, top-left-origin global coordinates
/// (same space Tauri uses for monitor/window positions).
pub fn mouse_location_top_left() -> Option<(f64, f64)> {
    let mtm = MainThreadMarker::new()?;
    let p = NSEvent::mouseLocation();
    let primary = NSScreen::screens(mtm).iter().next()?;
    let h = primary.frame().size.height;
    Some((p.x, h - p.y))
}

/// Set the window frame (points) directly, bypassing AppKit's title-bar
/// constraint, so a borderless window can cover a whole screen including
/// the menu-bar strip. `x,y,w,h` are top-left-origin logical coordinates.
pub fn set_frame_top_left(ptr: *mut c_void, x: f64, y: f64, w: f64, h: f64) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(primary) = NSScreen::screens(mtm).iter().next() else {
        return;
    };
    let ph = primary.frame().size.height;
    // A borderless window that exactly matches the screen frame is treated by
    // AppKit as full-screen content and is excluded from other apps'
    // full-screen Spaces; keep it one point short so it stays an overlay.
    let h = h - 1.0;
    let rect = NSRect::new(NSPoint::new(x, ph - (y + h)), objc2_foundation::NSSize::new(w, h));
    let win = unsafe { window_ref(ptr) };
    win.setFrame_display(rect, true);
}
