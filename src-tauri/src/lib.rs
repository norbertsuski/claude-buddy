pub mod bridge;
pub mod commands;
pub mod cursor;
pub mod config;
pub mod notch;
pub mod notify;
pub mod update;
pub mod usage;
pub mod usage_api;
pub mod visibility;
pub mod watcher;
pub mod window;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::watcher::liveness::SysLiveness;
use crate::watcher::registry::registry_dir;
use crate::watcher::watch::{spawn_watcher, UPDATE_EVENT};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            window::clamp_to_screen,
            window::resize_widget,
            commands::get_usage,
            window::list_displays,
            cursor::set_hover_rect,
            cursor::set_hover_rects,
            notch::notch_layout,
            bridge::transcript::session_detail,
            bridge::raise::raise_session,
            commands::get_sessions,
            commands::get_config,
            commands::set_config
        ])
        .setup(|app| {
            // No Dock icon, no app-switcher entry. Info.plist carries
            // LSUIElement for the bundled app; this covers `tauri dev`, which
            // runs the bare binary.
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Only when a signing key is configured; see `update::is_configured`.
            // The registration is allowed to fail rather than taking the app
            // down with it — an app that will not start because it cannot
            // check for its own update is worse than one that never checks.
            if crate::update::is_configured(app.config().plugins.0.get("updater")) {
                let _ = app.handle().plugin(tauri_plugin_updater::Builder::new().build());
            }

            let widget = app
                .get_webview_window("widget")
                .expect("widget window missing from tauri.conf.json");
            window::configure_panel(&widget)
                .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;

            // Before the first placement: notch mode derives where the window
            // goes from this, and a command cannot probe for it because NSScreen
            // needs the main thread and a command is not promised one.
            crate::notch::refresh();
            crate::notch::spawn_geometry_watcher(app.handle().clone());

            window::restore_position(&widget);
            window::build_tray_menu(app.handle())?;

            // A non-activating panel gets no mousemove in its webview, so the
            // cursor is sampled here and pushed to the page instead.
            crate::cursor::spawn_cursor_watcher(widget.clone());

            // Asks the API for the five-hour window on its own schedule, so
            // the meter is not limited to what Claude Code last cached. Its own
            // thread: reading the token can block on a Keychain dialog.
            crate::usage_api::start();

            app.manage(crate::watcher::watch::SnapshotStore::default());
            let handle = app.handle().clone();

            let watcher = spawn_watcher(
                registry_dir(),
                Arc::new(SysLiveness),
                Arc::new(crate::watcher::activity::TranscriptActivity::new(
                    crate::bridge::transcript::projects_dir(),
                )),
                Arc::new(crate::watcher::blocked::TranscriptBlocked::new(
                    crate::bridge::transcript::projects_dir(),
                )),
                Arc::new(crate::watcher::question::TranscriptQuestion::new(
                    crate::bridge::transcript::projects_dir(),
                )),
                move |update| {
                    handle
                        .state::<crate::watcher::watch::SnapshotStore>()
                        .set(update.sessions.clone());
                    crate::notify::deliver(&handle, &update.alerts);

                    let hide = crate::visibility::should_hide(
                        &update.sessions,
                        &crate::config::cached().hide_when,
                    );
                    let visibility_handle = handle.clone();
                    // Panel calls must run on the main thread; the watcher is
                    // its own thread.
                    let _ = handle.run_on_main_thread(move || {
                        crate::window::set_widget_visible(&visibility_handle, !hide);
                    });

                    let _ = handle.emit(UPDATE_EVENT, &update);
                },
            );

            // Keep the handle alive for the process lifetime.
            app.manage(watcher);

            crate::update::check_on_launch(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running clawde-buddy");
}
