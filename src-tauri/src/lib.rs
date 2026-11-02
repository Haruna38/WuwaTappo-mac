mod commands;
mod input;
mod keys;
mod macos;
mod menu;
mod server;
mod state;
mod storage;
mod windows;

use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;

use state::AppState;

/// Detects a stalled main thread and writes a `sample` of the process next
/// to the log so a deadlock can be diagnosed from the field.
fn start_main_thread_watchdog(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("wuwa-watchdog".into())
        .spawn(move || {
            let mut sampled = false;
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let (tx, rx) = std::sync::mpsc::channel::<()>();
                if app.run_on_main_thread(move || {
                    let _ = tx.send(());
                })
                .is_err()
                {
                    continue;
                }
                match rx.recv_timeout(std::time::Duration::from_secs(4)) {
                    Ok(()) => sampled = false,
                    Err(_) => {
                        log::error!("[watchdog] main thread stalled for >4s");
                        if !sampled {
                            sampled = true;
                            let out = app
                                .path()
                                .app_log_dir()
                                .map(|d| d.join("main-thread-stall.txt"))
                                .unwrap_or_else(|_| std::env::temp_dir().join("wuwa-stall.txt"));
                            let _ = std::process::Command::new("/usr/bin/sample")
                                .arg(std::process::id().to_string())
                                .arg("1")
                                .arg("-file")
                                .arg(&out)
                                .status();
                            log::error!("[watchdog] sample written to {}", out.display());
                        }
                    }
                }
            }
        })
        .expect("spawn watchdog");
}

pub fn run() {
    std::panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        log::error!("[panic] {info}\n{bt}");
        eprintln!("[panic] {info}\n{bt}");
    }));
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window(windows::MAIN) {
                let _ = w.show();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(
                    if cfg!(debug_assertions) || std::env::var_os("WUWA_LOG_DEBUG").is_some() {
                        log::LevelFilter::Debug
                    } else {
                        log::LevelFilter::Info
                    },
                )
                .level_for("tao", log::LevelFilter::Info)
                .level_for("wry", log::LevelFilter::Info)
                .level_for("hyper", log::LevelFilter::Info)
                .level_for("reqwest", log::LevelFilter::Info)
                .level_for("tiny_http", log::LevelFilter::Info)
                .level_for("tauri", log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("wuwatappo".into()),
                    }),
                ])
                .max_file_size(5 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(AppState::default())
        .on_menu_event(|app, event| menu::handle_menu_event(app, event))
        .setup(|app| {
            let handle = app.handle().clone();
            {
                let state = handle.state::<AppState>();
                state.input.lock().unwrap().shortcuts = storage::load_shortcuts(&handle);
            }
            if let Err(e) = server::start(handle.clone()) {
                log::error!("[http] {e}; multiplayer sockets will be rejected by the server");
            }
            windows::create_main(&handle)?;
            windows::create_toast(&handle)?;
            menu::create_tray(&handle)?;
            input::start(handle.clone());
            start_main_thread_watchdog(handle.clone());

            // Forward deep links (OAuth callback) that arrived at launch.
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let dl = handle.deep_link();
                if let Ok(Some(urls)) = dl.get_current() {
                    log::info!("[deep-link] launched with {} url(s)", urls.len());
                }
            }

            log::info!(
                "[APP] started version={} pid={}",
                handle.package_info().version,
                std::process::id()
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            windows::listener_ready,
            windows::open_panel_window,
            windows::close_panel_window,
            windows::is_panel_window_open,
            windows::raise_panel_window_above_pets,
            windows::set_panel_window_collapsed,
            windows::open_login_window,
            windows::close_login_window,
            windows::resize_login_window_to_content,
            windows::open_update_window,
            windows::close_update_window,
            windows::get_app_update_detail,
            windows::open_maintenance_notice,
            windows::get_maintenance_notice_payload,
            windows::has_maintenance_notice,
            windows::hide_main_after_maintenance_notice_ready,
            windows::exit_maintenance_app,
            windows::open_about_window,
            windows::close_about_window,
            windows::open_oss_notice_window,
            windows::close_oss_notice_window,
            windows::show_toast_overlay,
            windows::hide_toast_overlay,
            windows::set_main_window_fullscreen_shell,
            windows::sync_main_window_to_current_monitor,
            windows::apply_main_window_position,
            windows::set_main_window_content_bounds,
            windows::set_main_window_monitor_bounds,
            windows::sync_aux_windows_to_main_monitor,
            windows::finish_main_window_monitor_switch,
            windows::get_main_desktop_safe_area,
            windows::get_main_taskbar_insets,
            windows::get_taskbar_insets_at_point,
            windows::set_main_hit_region,
            windows::set_main_force_cursor_ignore,
            windows::set_main_swallow_mouse,
            windows::set_overlay_cursor,
            windows::set_native_input_cursor,
            windows::popup_main_dock_context_menu,
            windows::set_main_skip_taskbar,
            commands::write_app_log,
            commands::get_track_os_info,
            commands::get_boot_diagnostics,
            commands::is_internal_testing_build,
            commands::is_devtools_hotkeys_build,
            commands::load_key_press_count,
            commands::save_key_press_count,
            commands::load_cumulative_click_count,
            commands::save_cumulative_click_count,
            commands::get_app_shortcuts,
            commands::set_app_shortcuts,
            commands::set_shortcut_capture,
            commands::get_installer_launch_locale,
            commands::save_app_ui_locale,
            commands::get_installer_launch_mode,
            commands::get_preferred_system_ui_languages,
            commands::sync_native_menu,
            commands::check_app_update,
            commands::is_force_update_locked,
            commands::download_and_open_installer,
            commands::is_elevated_autostart_enabled,
            commands::set_elevated_autostart,
            commands::exit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
                // Closing the last window must not quit a tray app.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
            if let tauri::RunEvent::Exit = event {
                let _ = app.emit("cat-persist-clicks-before-exit", ());
            }
        });
}
