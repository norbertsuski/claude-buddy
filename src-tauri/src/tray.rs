//! The menu-bar icon and its menu.
//!
//! With `LSUIElement` there is no Dock icon and no app-switcher entry, so this
//! menu is the only route to quitting — and the only place the widget can be
//! reached from once it has hidden itself.
//!
//! The items here are the settings worth reaching without opening a window:
//! ones toggled in the middle of doing something else. Putting the widget away
//! for a screen share, silencing alerts for a meeting, and dropping subagent
//! noise while one run spawns six of them are all decisions made *while* the
//! thing is happening. Everything set once and forgotten — placement, display,
//! launch at login, which events raise an alert — stays in Settings, where a
//! longer list costs nothing.

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Wry};

use crate::config::{self, Config, MuteFor};
use crate::watcher::watch::now_ms;

/// Tray icon id, so the menu can be found again and rebuilt.
const TRAY_ID: &str = "widget-menu";

/// Install the menu-bar icon.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let menu = menu_for(app, &config::cached())?;

    TrayIconBuilder::with_id(TRAY_ID)
        // A tray icon without an image fails to build on macOS.
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            tauri::Error::Anyhow(anyhow::anyhow!("no default window icon in the bundle"))
        })?)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .build(app)?;

    Ok(())
}

/// Rebuild the menu from the settings as they stand.
///
/// Rebuilt wholesale rather than mutated in place. Four of these items carry
/// state — two ticks, the mute submenu's wording, and whether "Unmute now" can
/// be clicked — and holding handles to them means every future write to
/// settings has to remember to update each one. A rebuild cannot drift: it
/// reads the file once and the menu says what the file says.
///
/// Must run on the main thread, like every other AppKit call here.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = menu_for(app, &config::cached()) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn menu_for(app: &AppHandle, config: &Config) -> tauri::Result<Menu<Wry>> {
    let muted = config.alerts_muted(now_ms());

    let hide = CheckMenuItem::with_id(
        app,
        "hide",
        "Hide widget",
        true,
        config.hidden,
        None::<&str>,
    )?;

    // Durations rather than a plain toggle, and no absolute deadline in any
    // label: there is no date library here, and a menu reading "muted until
    // 15:04" would be a lie from 15:05 until whenever the user next opened it.
    let mute_hour = MenuItem::with_id(app, "mute:hour", "For 1 hour", true, None::<&str>)?;
    let mute_eight = MenuItem::with_id(app, "mute:eight", "For 8 hours", true, None::<&str>)?;
    let mute_open = MenuItem::with_id(app, "mute:open", "Until I unmute", true, None::<&str>)?;
    let mute_separator = PredefinedMenuItem::separator(app)?;
    // Disabled rather than absent while nothing is muted, so the submenu keeps
    // one shape and the item's greyed state is itself the answer to "am I
    // muted right now".
    let unmute = MenuItem::with_id(app, "unmute", "Unmute now", muted, None::<&str>)?;
    let mute = Submenu::with_items(
        app,
        if muted { "Alerts muted" } else { "Mute alerts" },
        true,
        &[
            &mute_hour as &dyn IsMenuItem<Wry>,
            &mute_eight,
            &mute_open,
            &mute_separator,
            &unmute,
        ],
    )?;

    let background = CheckMenuItem::with_id(
        app,
        "background",
        "Show background jobs",
        true,
        config.show_background_jobs,
        None::<&str>,
    )?;

    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let update = MenuItem::with_id(app, "update", "Check for updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit claude-buddy", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    // Omitted rather than greyed out when there is no signing key. With the
    // updater plugin unregistered there is nothing behind this item: clicking
    // it reached for plugin state that was never managed, which does not even
    // fail quietly — it panics on the spawned task, so the click looked like it
    // did nothing at all. Anyone who has configured a key gets the item back.
    let mut items: Vec<&dyn IsMenuItem<Wry>> =
        vec![&hide, &mute, &background, &separator, &settings];
    if crate::update::is_configured(app.config().plugins.0.get("updater")) {
        items.push(&update);
    }
    items.push(&quit);

    Menu::with_items(app, &items)
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "hide" => {
            edit(app, |config| config.hidden = !config.hidden);
            // Nothing else would act on this. The watcher re-evaluates
            // visibility only when a session changes, and on a quiet machine
            // that is not for hours — the widget would stay put until something
            // unrelated happened.
            crate::window::apply_visibility(app);
        }
        "mute:hour" => mute_for(app, MuteFor::Hour),
        "mute:eight" => mute_for(app, MuteFor::EightHours),
        "mute:open" => mute_for(app, MuteFor::UntilUnmuted),
        "unmute" => edit(app, |config| config.mute_until_ms = 0),
        "background" => edit(app, |config| {
            config.show_background_jobs = !config.show_background_jobs
        }),
        "settings" => crate::window::open_settings(app),
        "update" => crate::update::check_and_install(app.clone()),
        "quit" => app.exit(0),
        _ => {}
    }
}

fn mute_for(app: &AppHandle, choice: MuteFor) {
    let until = config::mute_until(now_ms(), choice);
    edit(app, |config| config.mute_until_ms = until);
    schedule_expiry_refresh(app, until);
}

/// Load, change, save, and tell everyone.
///
/// Load-modify-save rather than writing a cached copy back: the settings window
/// may be open and writing the same file, and this must not carry a stale
/// version of the fields it is not touching back over the top of it.
fn edit(app: &AppHandle, change: impl FnOnce(&mut Config)) {
    let path = config::config_path();
    let mut config = config::load(&path);
    change(&mut config);
    // `save` refreshes the in-memory copy that the alert path and placement
    // both read, so it has to succeed before anything is announced.
    if config::save(&path, &config).is_err() {
        return;
    }
    // The same event the settings window's own writes raise, so a menu toggle
    // and a form toggle are indistinguishable to the widget.
    let _ = app.emit(crate::commands::CONFIG_EVENT, config);
    refresh(app);
}

/// Wake once, when a timed mute runs out, to take "Alerts muted" back off the
/// menu.
///
/// One scheduled wake rather than a poll. Nothing but the clock changes this
/// answer, so a timer that fires every minute to relabel a menu item would be a
/// battery cost with a single, predictable payoff. An indefinite mute schedules
/// nothing at all: it ends when the user says so, and that path refreshes the
/// menu itself.
fn schedule_expiry_refresh(app: &AppHandle, until_ms: i64) {
    if until_ms == config::MUTE_INDEFINITE_MS {
        return;
    }
    let Ok(delay) = u64::try_from(until_ms - now_ms()) else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        // A second past the deadline, so `alerts_muted` has certainly turned
        // over by the time the menu is rebuilt against it.
        std::thread::sleep(std::time::Duration::from_millis(delay + 1_000));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || refresh(&handle));
    });
}
