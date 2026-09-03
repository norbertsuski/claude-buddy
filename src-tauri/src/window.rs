use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::Manager;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
use tauri_nspanel::{ManagerExt, WebviewWindowExt};

/// Above every ordinary window, including fullscreen ones. `NSStatusWindowLevel`
/// is 25; one above it keeps the widget clear of menu-bar extras.
const PANEL_LEVEL: i32 = 26;

/// Window label for the settings window.
pub const SETTINGS_LABEL: &str = "settings";

/// `NSWindowStyleMaskNonactivatingPanel`. The cocoa crate's `NSWindowStyleMask`
/// bitflags omit this constant, so the AppKit value is written out directly.
const NONACTIVATING_PANEL_MASK: i32 = 1 << 7;

/// Gap from the screen edge for a first-run placement.
pub const WIDGET_MARGIN: f64 = 12.0;

/// Convert the widget window into a non-activating panel that follows the user
/// across Spaces and never takes focus.
pub fn configure_panel(window: &WebviewWindow) -> Result<(), String> {
    let panel = window
        .to_panel()
        .map_err(|e| format!("to_panel failed: {e:?}"))?;

    panel.set_level(PANEL_LEVEL);

    // NonactivatingPanel: clicking the widget does not make claude-buddy the
    // active application, so the user's editor keeps focus and keyboard input.
    panel.set_style_mask(NONACTIVATING_PANEL_MASK);

    // CanJoinAllSpaces so the widget follows the user rather than living on one
    // Space; FullScreenAuxiliary so it draws over fullscreen apps.
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary,
    );

    // A non-activating panel receives no mouse-moved events by default, so the
    // webview never sees mouseover and the pill never morphs on hover.
    panel.set_accepts_mouse_moved_events(true);

    // Required. Swizzling the NSWindow to an NSPanel and rewriting its style
    // mask leaves it unordered, so the window config's `visible: true` does not
    // survive the conversion — without this the process runs with nothing on
    // screen.
    panel.show();

    Ok(())
}

/// Put the widget on or off screen.
///
/// `order_out` rather than closing: the panel keeps its configuration, its
/// level and its collection behaviour, so coming back is a single call rather
/// than a rebuild. Re-showing restores the saved position, because a widget
/// that reappears somewhere else reads as a bug.
pub fn set_widget_visible(app: &AppHandle, visible: bool) {
    let Ok(panel) = app.get_webview_panel("widget") else {
        return;
    };
    if visible {
        if !panel.is_visible() {
            panel.show();
            if let Some(widget) = app.get_webview_window("widget") {
                restore_position(&widget);
            }
        }
    } else if panel.is_visible() {
        panel.order_out(None);
    }
}

/// Re-decide whether the widget belongs on screen, and act on it.
///
/// The watcher does this after every snapshot, but a tray toggle changes the
/// answer with no session having moved, and the watcher only calls back when
/// something actually changed — on a quiet machine, not for hours. Reads the
/// sessions from the store rather than taking them as an argument so both
/// callers ask the same question of the same state.
///
/// Main thread only: panel calls are AppKit calls.
pub fn apply_visibility(app: &AppHandle) {
    let config = crate::config::cached();
    let sessions = app.state::<crate::watcher::store::SnapshotStore>().get();
    let hide = crate::visibility::should_hide(&sessions, &config.hide_when, config.hidden);
    set_widget_visible(app, !hide);
}

/// Open the settings window, or focus it if it is already open.
///
/// Settings lives in an ordinary window rather than inside the widget: the
/// widget is a non-activating panel that never becomes the key window, so a
/// form drawn there receives neither clicks nor keystrokes — including the one
/// that would close it again.
pub fn open_settings(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = existing.show();
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        return;
    }

    // An Accessory-policy app does not activate when it opens a window, so the
    // form would sit behind whatever was in front and take no keystrokes.
    // Become a regular app for as long as settings is open.
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);

    match WebviewWindowBuilder::new(
        app,
        SETTINGS_LABEL,
        WebviewUrl::App("index.html#settings".into()),
    )
    .title("claude-buddy Settings")
    // Sized for the grouped layout rather than for the old single column: the
    // rows put a label and a popup button on one line, and at 360 wide the
    // button truncated its own label on a window with room to spare. The panel
    // scrolls, so a window shrunk past its content — or a setting added later —
    // still cannot put a control out of reach.
    .inner_size(480.0, 560.0)
    .min_inner_size(400.0, 380.0)
    .resizable(true)
    .focused(true)
    // The window's background is AppKit's own material rather than a colour
    // painted by the page. A flat fill is the thing that gives a web-built
    // settings window away: it does not pick up the desktop tint behind it, it
    // does not desaturate when the window loses focus, and it does not follow
    // the appearance the way every other window on the machine does.
    //
    // `WindowBackground` is the material AppKit uses for an ordinary window,
    // which is what this is. `FollowsWindowActiveState` is the second half of
    // it — the material goes flat and grey when the window is not frontmost,
    // exactly like the About panel beside it.
    .transparent(true)
    .effects(
        EffectsBuilder::new()
            .effect(Effect::WindowBackground)
            .state(EffectState::FollowsWindowActiveState)
            .build(),
    )
    .build()
    {
        Ok(settings) => {
            let handle = app.clone();
            settings.on_window_event(move |event| {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    // Back to a menu-bar-only app once the form is gone.
                    let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            });
            let _ = settings.set_focus();
        }
        Err(_) => {
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }
}

/// Identify a display by name and resolution together. Name alone collides
/// across identical external monitors; resolution alone changes on scaling.
pub fn display_key(name: Option<&str>, width: u32, height: u32) -> String {
    format!("{}@{}x{}", name.unwrap_or("unknown"), width, height)
}

/// Where to put the widget on a given display.
///
/// A saved position is honoured only where it still fits: a position stored
/// against a wide external monitor would otherwise place the widget off-screen
/// on the laptop panel, where it cannot be dragged back.
pub fn resolve_position(
    saved: Option<[f64; 2]>,
    display: (f64, f64),
    widget: (f64, f64),
    margin: f64,
) -> (f64, f64) {
    let max_x = (display.0 - widget.0).max(0.0);
    let max_y = (display.1 - widget.1).max(0.0);

    match saved {
        Some([x, y]) => (x.clamp(0.0, max_x), y.clamp(0.0, max_y)),
        // Top centre: the widget sits where the eye already goes for status,
        // clear of the menu-bar extras crowding the right-hand corner.
        None => (
            ((display.0 - widget.0) / 2.0).clamp(0.0, max_x),
            margin.min(max_y),
        ),
    }
}

/// Translate a monitor-local placement into the global desktop coordinates that
/// `set_position` expects.
///
/// `resolve_position` works in monitor-local space so it can reason about fit;
/// macOS positions windows in one global space spanning every display, whose
/// origin is the main display's top-left. A secondary monitor can therefore sit
/// at a large negative origin, and passing a local value straight to
/// `set_position` puts the window in dead space between displays.
pub fn to_global(local: (f64, f64), monitor_origin: (f64, f64)) -> (f64, f64) {
    (monitor_origin.0 + local.0, monitor_origin.1 + local.1)
}

/// Inverse of [`to_global`].
pub fn to_local(global: (f64, f64), monitor_origin: (f64, f64)) -> (f64, f64) {
    (global.0 - monitor_origin.0, global.1 - monitor_origin.1)
}

/// One attached display, for the settings picker.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub key: String,
    pub label: String,
    pub primary: bool,
}

/// Displays currently attached, in the order macOS reports them.
#[tauri::command]
pub fn list_displays(window: WebviewWindow) -> Vec<DisplayInfo> {
    let primary_key = window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| monitor_key(&m));

    window
        .available_monitors()
        .map(|monitors| {
            monitors
                .iter()
                .map(|m| {
                    let key = monitor_key(m);
                    let scale = m.scale_factor();
                    let size = m.size().to_logical::<f64>(scale);
                    DisplayInfo {
                        label: format!(
                            "{} ({}\u{00d7}{})",
                            m.name().map(|s| s.as_str()).unwrap_or("Display"),
                            size.width as u32,
                            size.height as u32
                        ),
                        primary: Some(&key) == primary_key.as_ref(),
                        key,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn monitor_key(monitor: &tauri::Monitor) -> String {
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    display_key(
        monitor.name().map(|s| s.as_str()),
        size.width as u32,
        size.height as u32,
    )
}

/// Which display the widget belongs on.
///
/// An explicit choice wins. Otherwise a display the user has already dragged
/// the widget to wins, so moving it to a second screen sticks. Failing both,
/// the primary display.
pub fn choose_display_key(
    preferred: Option<&str>,
    attached: &[String],
    current: Option<&str>,
    current_has_saved_position: bool,
    primary: Option<&str>,
) -> Option<String> {
    if let Some(pref) = preferred {
        if attached.iter().any(|k| k == pref) {
            return Some(pref.to_string());
        }
    }
    if current_has_saved_position {
        if let Some(current) = current {
            return Some(current.to_string());
        }
    }
    primary.map(str::to_string)
}

/// Place the widget: where it was last left on this display, or top centre of
/// the primary display when it has never been placed.
///
/// The default deliberately targets the primary display rather than
/// `current_monitor()`. On a multi-display setup macOS may open the window on
/// whichever monitor it likes, and "top centre of some monitor you are not
/// looking at" is indistinguishable from a bug.
pub fn restore_position(window: &WebviewWindow) {
    let settings = crate::config::cached();

    // Notch placement is derived from the display, so it bypasses the saved
    // positions and the display picker entirely — including the NSScreen to
    // Tauri-Monitor name matching that `choose_display_key` would otherwise
    // need, which would have been the most fragile part of the feature.
    //
    // Falling through on failure is the documented fallback: with the lid shut
    // there is no notch to place against, and the widget returns to free
    // placement without the config value being rewritten, so reopening the lid
    // restores notch mode on its own.
    if settings.wants_notch() && place_in_notch(window) {
        return;
    }

    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    let attached: Vec<String> = monitors.iter().map(monitor_key).collect();
    let current = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|m| monitor_key(&m));
    let primary = window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| monitor_key(&m));

    let chosen = choose_display_key(
        settings.preferred_display.as_deref(),
        &attached,
        current.as_deref(),
        current
            .as_deref()
            .is_some_and(|k| settings.positions.contains_key(k)),
        primary.as_deref(),
    );

    let Some(chosen) = chosen else { return };
    let Some(monitor) = monitors.iter().find(|m| monitor_key(m) == chosen) else {
        return;
    };

    place_on(window, monitor, settings.positions.get(&chosen).copied());
}

/// Put the widget in the menu bar, flanking the notch. False when there is no
/// notch to place against.
///
/// Position *and* size both come from Rust here, unlike free placement where the
/// frontend measures itself and calls `resize_widget`. The chips are laid out
/// against the notch's edges, so the window has to be the size the geometry says
/// before the page can place anything inside it.
fn place_in_notch(window: &WebviewWindow) -> bool {
    let Some(geo) = crate::notch::cached() else {
        return false;
    };
    let (origin, size) = crate::notch::window_frame(
        &geo,
        crate::notch::FLANK_BUDGET,
        crate::notch::POPOVER_ALLOWANCE,
    );

    // The swizzled NSPanel ignores Tauri's `set_size`, which is why
    // `resize_widget` reaches for the panel's own `setContentSize:` too.
    if let Ok(panel) = window.app_handle().get_webview_panel("widget") {
        panel.set_content_size(size.0, size.1);
    }
    let _ = window.set_position(LogicalPosition::new(origin.0, origin.1));
    true
}

/// Position the window on `monitor`, honouring a saved monitor-local point or
/// falling back to the default placement.
fn place_on(window: &WebviewWindow, monitor: &tauri::Monitor, saved: Option<[f64; 2]>) {
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let origin = monitor.position().to_logical::<f64>(scale);
    // The window's own scale, not the target monitor's. `outer_size` reports
    // physical pixels on the display the window is currently on, and the window
    // has not moved yet — so dividing them by the destination's scale sized the
    // widget wrongly whenever the two displays differ, which is exactly the case
    // this function exists to handle. A 383pt widget on a 2x panel measures
    // 766px, and read against a 1x display that is a 766pt widget: centring it
    // then pushed it well off centre.
    let Ok(window_scale) = window.scale_factor() else {
        return;
    };
    let widget: LogicalSize<f64> = match window.outer_size() {
        Ok(s) => s.to_logical(window_scale),
        Err(_) => return,
    };

    let local = resolve_position(
        saved,
        (size.width, size.height),
        (widget.width, widget.height),
        WIDGET_MARGIN,
    );
    let (x, y) = to_global(local, (origin.x, origin.y));

    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// Persist the widget's position against the display it now sits on.
///
/// Called once when a drag ends rather than on every `Moved` event: a drag
/// produces dozens of intermediate positions, and the app's own repositioning
/// would otherwise be recorded as if the user had chosen it.
pub fn persist_position(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let logical = pos.to_logical::<f64>(scale);

    let key = display_key(
        monitor.name().map(|s| s.as_str()),
        size.width as u32,
        size.height as u32,
    );
    let origin = monitor.position().to_logical::<f64>(scale);
    let (local_x, local_y) = to_local((logical.x, logical.y), (origin.x, origin.y));

    let path = crate::config::config_path();
    let mut settings = crate::config::load(&path);
    settings.positions.insert(key, [local_x, local_y]);
    let _ = crate::config::save(&path, &settings);
}

/// Resize the panel to the size the frontend measured.
///
/// Uses the panel's own `setContentSize:` rather than Tauri's `set_size`, which
/// the swizzled NSPanel ignores. Keeping the window snug to the pill matters
/// twice over: a larger window leaves transparent margin that swallows clicks,
/// and mouse tracking is installed against the window's bounds.
#[tauri::command]
pub fn resize_widget(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let panel = app
        .get_webview_panel("widget")
        .map_err(|e| format!("widget panel not found: {e:?}"))?;

    let (width, height) = (width.max(1.0), height.max(1.0));

    // A resize on a transparent panel costs a window-server round trip and shows
    // one unpainted frame, so resizing to the size the window already has is not
    // free — it is a dropped frame for nothing. The frontend skips the call in
    // the case it can see; this catches the rest.
    if !record_size(width, height) {
        return Ok(());
    }

    panel.set_content_size(width, height);

    // Reposition in the same command rather than leaving it to a second call
    // from the frontend: two window-server updates around a resize produced a
    // second hitch, and the later one landed mid-animation.
    if let Some(widget) = app.get_webview_window("widget") {
        if has_saved_position(&widget) {
            let _ = clamp_to_screen(widget);
        } else {
            // Centring runs before the frontend has measured itself, so the
            // first placement is centred against the window's initial size.
            // Re-centre now that the real size is known.
            restore_position(&widget);
        }
    }
    Ok(())
}

/// The last size handed to `set_content_size`.
///
/// Deliberately remembered rather than read back off the window: the widget is
/// a fixed-size panel with no resize handles, so this command is the only thing
/// that ever changes its size, and a remembered value cannot be stale in the
/// way an `inner_size` read taken before AppKit has applied the change can.
static LAST_SIZE: std::sync::OnceLock<std::sync::Mutex<Option<(f64, f64)>>> =
    std::sync::OnceLock::new();

/// Record a requested size, reporting whether it differs from the last one.
fn record_size(width: f64, height: f64) -> bool {
    let mut slot = LAST_SIZE
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .expect("widget size cache poisoned");
    if *slot == Some((width, height)) {
        return false;
    }
    *slot = Some((width, height));
    true
}

/// Whether the user has already placed the widget on its current display.
fn has_saved_position(window: &WebviewWindow) -> bool {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return false;
    };
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let key = display_key(
        monitor.name().map(|s| s.as_str()),
        size.width as u32,
        size.height as u32,
    );
    crate::config::cached().positions.contains_key(&key)
}

/// Nudge the widget back onto its display.
///
/// The panel resizes to its own content, so opening the popover can push its
/// right or bottom edge past the screen. Clamping after a resize is what the
/// design calls edge-flipping: the widget stays wholly visible wherever it is
/// parked.
#[tauri::command]
pub fn clamp_to_screen(window: WebviewWindow) -> Result<(), String> {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size().to_logical::<f64>(scale);
    let size = window
        .outer_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let pos = window
        .outer_position()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);

    let origin = monitor.position().to_logical::<f64>(scale);
    let local = to_local((pos.x, pos.y), (origin.x, origin.y));

    let (x, y) = to_global(
        resolve_position(
            Some([local.0, local.1]),
            (screen.width, screen.height),
            (size.width, size.height),
            WIDGET_MARGIN,
        ),
        (origin.x, origin.y),
    );

    if (x - pos.x).abs() > 0.5 || (y - pos.y).abs() > 0.5 {
        window
            .set_position(LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: (f64, f64) = (1920.0, 1080.0);
    const WIDGET: (f64, f64) = (200.0, 40.0);

    #[test]
    fn a_widget_is_centred_by_its_size_in_points_not_its_pixels() {
        // Regression: `place_on` divided the window's physical size by the
        // *destination* monitor's scale. Moving a 383pt widget from a 2x panel
        // to a 1x display read its 766 physical pixels as 766 points, and it
        // landed far left of centre instead of on it.
        let display = (3840.0, 2160.0);
        let physical_on_a_2x_panel = 766.0;
        let points = physical_on_a_2x_panel / 2.0;

        let right = resolve_position(None, display, (points, 45.0), WIDGET_MARGIN);
        let wrong = resolve_position(None, display, (physical_on_a_2x_panel, 45.0), WIDGET_MARGIN);

        assert_eq!(right.0, (3840.0 - 383.0) / 2.0);
        assert_ne!(
            right.0, wrong.0,
            "the two scales must not agree, or this proves nothing"
        );
        assert!(
            wrong.0 < right.0,
            "the old maths pushed the widget left of centre"
        );
    }

    /// The one test touching `LAST_SIZE`, since it is process-wide: splitting
    /// it up would let the cases race each other.
    #[test]
    fn a_repeated_size_is_not_applied_twice() {
        // Resizing a transparent panel shows one unpainted frame, so resizing
        // to the size the window already has is a dropped frame for nothing.
        assert!(record_size(383.0, 460.0), "first size is applied");
        assert!(!record_size(383.0, 460.0), "the same size is skipped");
        assert!(record_size(383.0, 461.0), "a taller box is applied");
        assert!(record_size(384.0, 461.0), "a wider box is applied");
        assert!(!record_size(384.0, 461.0), "and then skipped again");
    }

    #[test]
    fn an_explicit_display_choice_wins() {
        let attached = vec!["A@1470x956".to_string(), "B@3840x2160".to_string()];
        assert_eq!(
            choose_display_key(
                Some("B@3840x2160"),
                &attached,
                Some("A@1470x956"),
                true,
                Some("A@1470x956")
            ),
            Some("B@3840x2160".to_string())
        );
    }

    #[test]
    fn a_choice_for_a_detached_display_is_ignored() {
        let attached = vec!["A@1470x956".to_string()];
        assert_eq!(
            choose_display_key(Some("GONE@1x1"), &attached, None, false, Some("A@1470x956")),
            Some("A@1470x956".to_string())
        );
    }

    #[test]
    fn a_display_the_widget_was_dragged_to_is_kept() {
        let attached = vec!["A@1470x956".to_string(), "B@3840x2160".to_string()];
        assert_eq!(
            choose_display_key(
                None,
                &attached,
                Some("B@3840x2160"),
                true,
                Some("A@1470x956")
            ),
            Some("B@3840x2160".to_string())
        );
    }

    #[test]
    fn without_a_choice_or_a_drag_the_primary_display_is_used() {
        let attached = vec!["A@1470x956".to_string(), "B@3840x2160".to_string()];
        assert_eq!(
            choose_display_key(
                None,
                &attached,
                Some("B@3840x2160"),
                false,
                Some("A@1470x956")
            ),
            Some("A@1470x956".to_string())
        );
    }

    #[test]
    fn display_key_combines_name_and_resolution() {
        assert_eq!(
            display_key(Some("Built-in Retina Display"), 3024, 1964),
            "Built-in Retina Display@3024x1964"
        );
    }

    #[test]
    fn display_key_tolerates_an_unnamed_display() {
        assert_eq!(display_key(None, 1920, 1080), "unknown@1920x1080");
    }

    #[test]
    fn no_saved_position_defaults_to_the_top_centre() {
        let (x, y) = resolve_position(None, DISPLAY, WIDGET, WIDGET_MARGIN);
        assert_eq!(x, (1920.0 - 200.0) / 2.0);
        assert_eq!(y, WIDGET_MARGIN);
    }

    #[test]
    fn the_default_stays_on_screen_when_the_widget_is_wider_than_the_display() {
        let (x, y) = resolve_position(None, (150.0, 30.0), WIDGET, WIDGET_MARGIN);
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn a_local_placement_becomes_global_by_adding_the_monitor_origin() {
        // Regression: `set_position` takes global desktop coordinates, but
        // `resolve_position` reasons in monitor-local space. Passing the local
        // value straight through put the widget at global (1820, 12) — off the
        // right edge of the 1470-wide main display, in dead space. Observed on
        // a laptop at origin (0,0) plus a 3840x2160 external at (-3840,-1264).
        let external = (-3840.0, -1264.0);
        let local = resolve_position(None, (3840.0, 2160.0), WIDGET, WIDGET_MARGIN);
        assert_eq!(local, (1820.0, WIDGET_MARGIN));

        assert_eq!(to_global(local, external), (-2020.0, -1252.0));
    }

    #[test]
    fn a_main_display_at_the_origin_is_unaffected_by_the_translation() {
        let local = resolve_position(None, DISPLAY, WIDGET, WIDGET_MARGIN);
        assert_eq!(to_global(local, (0.0, 0.0)), local);
    }

    #[test]
    fn global_and_local_round_trip() {
        let origin = (-3840.0, -1264.0);
        let global = (-2020.0, -1252.0);
        assert_eq!(to_global(to_local(global, origin), origin), global);
    }

    #[test]
    fn a_saved_position_inside_the_display_is_honoured() {
        let (x, y) = resolve_position(Some([300.0, 120.0]), DISPLAY, WIDGET, WIDGET_MARGIN);
        assert_eq!((x, y), (300.0, 120.0));
    }

    #[test]
    fn a_position_saved_for_a_larger_display_is_clamped_back_on_screen() {
        // Saved while a 3440-wide monitor was attached, restored on the laptop.
        let (x, y) = resolve_position(Some([3200.0, 900.0]), DISPLAY, WIDGET, WIDGET_MARGIN);
        assert!(x + WIDGET.0 <= DISPLAY.0, "x={x} runs off the right edge");
        assert!(y + WIDGET.1 <= DISPLAY.1, "y={y} runs off the bottom edge");
    }

    #[test]
    fn a_negative_saved_position_is_clamped_to_the_origin() {
        let (x, y) = resolve_position(Some([-500.0, -80.0]), DISPLAY, WIDGET, WIDGET_MARGIN);
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn a_widget_wider_than_the_display_pins_to_the_origin() {
        let (x, y) = resolve_position(Some([10.0, 10.0]), (150.0, 30.0), WIDGET, WIDGET_MARGIN);
        assert_eq!((x, y), (0.0, 0.0));
    }
}
