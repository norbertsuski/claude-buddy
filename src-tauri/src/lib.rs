pub mod bridge;
pub mod commands;
pub mod usage;
pub mod usage_api;
pub mod watcher;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::watcher::registry::registry_dir;
use crate::watcher::watch::{spawn_watcher, UPDATE_EVENT};
use buddy_core::watcher::liveness::SysLiveness;

pub fn run() {
    // Core cannot know which app it is inside, and macOS keys Application
    // Support by bundle identifier, so this has to come before anything at all
    // reads or writes settings — core panics rather than guessing, because a
    // guess would mean one buddy silently overwriting another's settings.
    //
    // The legacy spelling is a typo that shipped up to 0.4.0. It must not be
    // "fixed": it names a directory that exists on every 0.4.0 machine, and
    // migrate_legacy_config is the one-time carry-over out of it.
    buddy_core::config::set_bundle_ids("com.claude.buddy", Some("com.clawde.buddy"));
    buddy_core::config::migrate_legacy_config();

    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            buddy_core::window::clamp_to_screen,
            buddy_core::window::resize_widget,
            commands::get_usage,
            buddy_core::window::list_displays,
            buddy_core::cursor::set_hover_rect,
            buddy_core::cursor::set_hover_rects,
            buddy_core::notch::notch_layout,
            bridge::transcript::session_detail,
            buddy_core::bridge::raise::raise_session,
            commands::get_sessions,
            commands::get_config,
            commands::set_config
        ])
        .setup(|app| {
            // No Dock icon, no app-switcher entry. Info.plist carries
            // LSUIElement for the bundled app; this covers `tauri dev`, which
            // runs the bare binary.
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Only when a signing key is configured; see `buddy_core::update::is_configured`.
            // The registration is allowed to fail rather than taking the app
            // down with it — an app that will not start because it cannot
            // check for its own update is worse than one that never checks.
            if buddy_core::update::is_configured(app.config().plugins.0.get("updater")) {
                let _ = app
                    .handle()
                    .plugin(tauri_plugin_updater::Builder::new().build());
            }

            let widget = app
                .get_webview_window("widget")
                .expect("widget window missing from tauri.conf.json");
            buddy_core::window::configure_panel(&widget)
                .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;

            // Before the first placement: notch mode derives where the window
            // goes from this, and a command cannot probe for it because NSScreen
            // needs the main thread and a command is not promised one.
            buddy_core::notch::refresh();
            buddy_core::notch::spawn_geometry_watcher(app.handle().clone());

            buddy_core::window::restore_position(&widget);
            buddy_core::tray::build(app.handle())?;

            // After the settings migration and before anything can save over
            // it: the rename moved the LaunchAgent's name, so an upgrading
            // user's login item still points at the old bundle while the
            // renamed build has none of its own. See `buddy_core::autostart::reconcile`.
            buddy_core::autostart::reconcile(
                app.handle(),
                buddy_core::config::load(&buddy_core::config::config_path()).launch_at_login,
            );

            // A non-activating panel gets no mousemove in its webview, so the
            // cursor is sampled here and pushed to the page instead.
            buddy_core::cursor::spawn_cursor_watcher(widget.clone());

            // Asks the API for the five-hour window on its own schedule, so
            // the meter is not limited to what Claude Code last cached. Its own
            // thread: reading the token can block on a Keychain dialog.
            crate::usage_api::start();

            app.manage(buddy_core::watcher::store::SnapshotStore::default());
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
                Arc::new(crate::watcher::working::TranscriptWork::new(
                    crate::bridge::transcript::projects_dir(),
                )),
                Arc::new(crate::watcher::tasks::TranscriptTasks::new(
                    crate::bridge::transcript::projects_dir(),
                )),
                Arc::new(crate::watcher::title::TranscriptTitle::new(
                    crate::bridge::transcript::projects_dir(),
                )),
                Arc::new(crate::watcher::question::TranscriptQuestion::new(
                    crate::bridge::transcript::projects_dir(),
                )),
                move |update| {
                    handle
                        .state::<buddy_core::watcher::store::SnapshotStore>()
                        .set(update.sessions.clone());
                    buddy_core::notify::deliver(&handle, &update.alerts);

                    // Stays on the watcher thread, unlike the visibility call
                    // below: a power assertion is thread-safe and there is no
                    // AppKit call to marshal.
                    if buddy_core::awake::apply(buddy_core::awake::should_stay_awake(
                        &update.sessions,
                        buddy_core::config::cached().keep_awake,
                    )) {
                        // The tray item reads "Keeping screen awake now" only
                        // while the hold is real, so a hold starting or ending
                        // is exactly when that label goes stale. Menu rebuilds
                        // are AppKit calls, unlike the assertion itself.
                        let label_handle = handle.clone();
                        let _ = handle
                            .run_on_main_thread(move || buddy_core::tray::refresh(&label_handle));
                    }

                    let visibility_handle = handle.clone();
                    // Panel calls must run on the main thread; the watcher is
                    // its own thread. The decision is made there too, against
                    // the store the line above has just written, so the tray's
                    // "Hide widget" and this path cannot disagree.
                    let _ = handle.run_on_main_thread(move || {
                        buddy_core::window::apply_visibility(&visibility_handle);
                    });

                    let _ = handle.emit(UPDATE_EVENT, &update);
                },
            );

            // Keep the handle alive for the process lifetime.
            app.manage(watcher);

            buddy_core::update::check_on_launch(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running claude-buddy");
}
