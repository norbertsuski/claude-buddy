//! The menu-bar icon and its menu.
//!
//! With `LSUIElement` there is no Dock icon and no app-switcher entry, so this
//! menu is the only route to quitting — and the only place the widget can be
//! reached from once it has hidden itself.
//!
//! The middle group is the settings worth reaching without opening a window:
//! ones toggled in the middle of doing something else. Putting the widget away
//! for a screen share, holding the display on for a long run, silencing alerts
//! for a meeting, and dropping subagent noise while one run spawns six of them
//! are all decisions made *while* the thing is happening. Everything set once
//! and forgotten — placement, display, launch at login, which events raise an
//! alert — stays in Settings, where a longer list costs nothing.
//!
//! Above them sits what the app *is* — its identity and its version — where a
//! Mac app menu would put them, and below them the two ways out.

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::clock::now_ms;
use crate::config::{self, Config, MuteFor};

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

    // Beside "Hide widget" rather than in Settings: both are decisions about
    // what the machine does while the user is looking somewhere else, and this
    // one is taken per-run — "not this time, it matters".
    //
    // The label comes from whether the display is actually being held, not from
    // the setting: a tick alone reads as "keep the screen awake, always", and
    // this setting does nothing whatsoever on an idle machine. Kept honest by
    // `lib.rs` rebuilding the menu whenever the hold starts or stops, so the
    // wording cannot go stale the way a baked-in deadline would.
    let keep_awake = CheckMenuItem::with_id(
        app,
        "keepawake",
        crate::awake::menu_label(crate::awake::is_holding()),
        true,
        config.keep_awake,
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

    // With `LSUIElement` there is no app menu, so this is the only place the
    // app states its own name, version and author.
    //
    // Deliberately not `PredefinedMenuItem::about`. That item is handled inside
    // muda, which shows the panel and returns without sending a `MenuEvent` —
    // so this handler never sees the click, and the panel is left sitting at
    // normal window level behind the frontmost app, looking for all the world
    // like a menu item that does nothing. `about::show` does the same call and
    // then activates. See that module for the full story.
    let about = MenuItem::with_id(app, "about", "About claude-buddy", true, None::<&str>)?;

    // Named after whatever the last check found, so the item stops saying
    // "check" once the answer is known and the launch notification has already
    // told the user there is something waiting.
    let update = MenuItem::with_id(
        app,
        "update",
        crate::update::update_label(crate::update::available().as_deref()),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit claude-buddy", true, None::<&str>)?;
    // Two, not one reused twice: a separator is a real `NSMenuItem`, and the
    // same instance cannot occupy two positions in one menu.
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;

    // Three groups: what this app *is*, what to change about it right now, and
    // the two ways out. The mid-task toggles are the middle group because they
    // are what the menu is opened for; identity and version sit above them
    // where a Mac app menu would put them.
    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&about];
    // Omitted rather than greyed out when there is no signing key. With the
    // updater plugin unregistered there is nothing behind this item: clicking
    // it reached for plugin state that was never managed, which does not even
    // fail quietly — it panics on the spawned task, so the click looked like it
    // did nothing at all. Anyone who has configured a key gets the item back.
    //
    // Its absence leaves About alone above the first separator, which is a
    // group of one rather than an empty one — so no separator has to be made
    // conditional on it.
    if crate::update::is_configured(app.config().plugins.0.get("updater")) {
        items.push(&update);
    }
    items.extend([
        &first_separator as &dyn IsMenuItem<Wry>,
        &hide,
        &keep_awake,
        &background,
        &mute,
        &second_separator,
        &settings,
        &quit,
    ]);

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
        "keepawake" => {
            edit(app, |config| config.keep_awake = !config.keep_awake);
            // Same reason "hide" calls straight into visibility: the watcher
            // re-evaluates only when a session changes, so ticking this
            // mid-run would otherwise do nothing until the run ended.
            //
            // Rebuilt again afterwards because `edit` rebuilt the menu before
            // the hold existed, leaving the label a step behind the tick.
            if apply_keep_awake(app) {
                refresh(app);
            }
        }
        "mute:hour" => mute_for(app, MuteFor::Hour),
        "mute:eight" => mute_for(app, MuteFor::EightHours),
        "mute:open" => mute_for(app, MuteFor::UntilUnmuted),
        "unmute" => edit(app, |config| config.mute_until_ms = 0),
        "background" => edit(app, |config| {
            config.show_background_jobs = !config.show_background_jobs
        }),
        "settings" => crate::window::open_settings(app),
        "about" => crate::about::show(app),
        "update" => crate::update::check_and_install(app.clone()),
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Engage or release the display-sleep hold against the sessions as they stand.
/// Returns whether the hold changed, and so whether the menu needs rebuilding.
///
/// `try_state` rather than `state`: the tray is built before the watcher's
/// snapshot store is managed, and a panic during setup would take the whole app
/// with it. No store yet means no sessions, which is the right answer that early
/// anyway.
fn apply_keep_awake(app: &AppHandle) -> bool {
    let sessions = app
        .try_state::<crate::watcher::store::SnapshotStore>()
        .map(|store| store.get())
        .unwrap_or_default();
    crate::awake::apply(crate::awake::should_stay_awake(
        &sessions,
        config::cached().keep_awake,
    ))
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
    let _ = app.emit(crate::config::CONFIG_EVENT, config);
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
