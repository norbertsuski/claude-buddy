use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use serde::Serialize;
use tauri::{Emitter, Manager, WebviewWindow};

/// How often the cursor is sampled. 60ms is below the threshold where hover
/// feels laggy, and the work per tick is one syscall plus a comparison.
pub const POLL: Duration = Duration::from_millis(60);

/// Cursor position relative to the widget, pushed to the frontend.
pub const CURSOR_EVENT: &str = "ui://cursor";

/// A left-button press that landed on the widget.
pub const CLICK_EVENT: &str = "ui://click";

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
}

/// Whether the left mouse button is currently held.
///
/// Read the same way as the cursor position, and for the same reason: the
/// swizzled panel delivers no mouse events to WKWebView, so the page cannot
/// observe its own clicks.
fn left_button_down() -> bool {
    // 0 = kCGEventSourceStateCombinedSessionState, 0 = left button.
    unsafe { CGEventSourceButtonState(0, 0) }
}

/// Whether this sample is the leading edge of a press on the widget.
pub fn is_click_edge(was_down: bool, is_down: bool, inside: bool) -> bool {
    inside && is_down && !was_down
}

/// How far the cursor must travel while held before a press counts as a drag
/// rather than a click. Without a threshold every click would nudge the widget.
pub const DRAG_THRESHOLD: f64 = 3.0;

/// Whether a held press has moved far enough to be a drag.
pub fn exceeds_drag_threshold(from: (f64, f64), to: (f64, f64)) -> bool {
    (to.0 - from.0).abs() > DRAG_THRESHOLD || (to.1 - from.1).abs() > DRAG_THRESHOLD
}

/// Where the window should sit so the grabbed point stays under the cursor.
pub fn dragged_origin(cursor: (f64, f64), grab_offset: (f64, f64)) -> (f64, f64) {
    (cursor.0 - grab_offset.0, cursor.1 - grab_offset.1)
}

/// What a sample means for the button gesture in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressAction {
    /// Nothing to do — including a button held down outside the widget.
    Ignore,
    /// A press just started on the widget.
    Begin,
    /// The press has moved far enough to be dragging the widget.
    Drag,
    /// Released without moving: a click.
    Click,
    /// Released after dragging.
    DragEnd,
}

/// State carried across samples while the button is held.
#[derive(Debug, Default)]
struct PressState {
    /// Raw button state last sample, used only for edge detection.
    raw_down: bool,
    /// Whether a press that began *on the widget* is still in progress.
    active: bool,
    /// Cursor position when the press began, in global coordinates.
    origin: (f64, f64),
    /// Cursor offset within the window when the press began.
    grab: (f64, f64),
    dragging: bool,
}

/// Decide what a sample means.
///
/// A gesture is only ever owned once it *begins* on the widget. Mirroring the
/// raw button state instead meant a click anywhere else on screen looked like a
/// drag already in progress, and the widget teleported to the cursor with a
/// grab offset of (0,0).
pub fn press_step(
    raw_was_down: bool,
    active: bool,
    dragging: bool,
    is_down: bool,
    inside: bool,
    moved_enough: bool,
) -> PressAction {
    if is_click_edge(raw_was_down, is_down, inside) {
        return PressAction::Begin;
    }
    if !active {
        return PressAction::Ignore;
    }
    if is_down {
        if dragging || moved_enough {
            return PressAction::Drag;
        }
        return PressAction::Ignore;
    }
    if dragging {
        PressAction::DragEnd
    } else {
        PressAction::Click
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPosition {
    /// Window-local CSS pixels.
    pub x: f64,
    pub y: f64,
    pub inside: bool,
}

/// The region of the window that counts as "the widget", in window-local
/// coordinates, as last reported by the frontend.
///
/// The window is deliberately larger than the pill — it is sized to the widest
/// state so that hovering never resizes it — so the window rect is no longer a
/// usable proxy for "the cursor is on the widget".
static HOVER_RECT: std::sync::OnceLock<std::sync::Mutex<Option<Rect>>> =
    std::sync::OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn hover_rect() -> &'static std::sync::Mutex<Option<Rect>> {
    HOVER_RECT.get_or_init(|| std::sync::Mutex::new(None))
}

/// Report which part of the window is the widget itself.
#[tauri::command]
pub fn set_hover_rect(x: f64, y: f64, width: f64, height: f64) {
    *hover_rect().lock().expect("hover rect poisoned") = Some(Rect { x, y, width, height });
}

pub fn contains(rect: Rect, x: f64, y: f64) -> bool {
    x >= rect.x && y >= rect.y && x <= rect.x + rect.width && y <= rect.y + rect.height
}

/// Convert a global cursor point into window-local coordinates.
///
/// This exists because a non-activating `NSPanel` never becomes the key window,
/// and WKWebView installs its tracking areas as `activeInKeyWindow` — so the
/// page receives no `mousemove` and CSS `:hover` never fires. Feeding it
/// coordinates lets the page hit-test for itself without the panel ever taking
/// focus.
pub fn to_window_local(
    cursor: (f64, f64),
    window_origin: (f64, f64),
    window_size: (f64, f64),
    widget: Option<Rect>,
) -> CursorPosition {
    let x = cursor.0 - window_origin.0;
    let y = cursor.1 - window_origin.1;
    // Before the frontend has measured itself, the whole window stands in.
    let bounds = widget.unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        width: window_size.0,
        height: window_size.1,
    });
    CursorPosition { x, y, inside: contains(bounds, x, y) }
}

fn global_cursor() -> Option<(f64, f64)> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
    let event = CGEvent::new(source).ok()?;
    let point = event.location();
    Some((point.x, point.y))
}

/// Sample the cursor and push its window-local position whenever it changes.
///
/// Emits only on change so an idle cursor costs nothing downstream.
///
/// The sampling itself runs on a background thread — the Core Graphics call is
/// thread-safe — but reading the window's frame is hopped onto the main thread,
/// because AppKit geometry accessors are main-thread only and silently fail
/// elsewhere.
pub fn spawn_cursor_watcher(window: WebviewWindow) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let previous: Arc<std::sync::Mutex<Option<CursorPosition>>> =
        Arc::new(std::sync::Mutex::new(None));
    let press = Arc::new(std::sync::Mutex::new(PressState::default()));

    std::thread::spawn(move || {
        while !stop_thread.load(Ordering::Relaxed) {
            std::thread::sleep(POLL);

            let Some(cursor) = global_cursor() else { continue };
            let is_down = left_button_down();
            let window = window.clone();
            let previous = previous.clone();
            let press = press.clone();

            let _ = window.clone().run_on_main_thread(move || {
                let Ok(scale) = window.scale_factor() else { return };
                let Ok(pos) = window.outer_position() else { return };
                let Ok(size) = window.outer_size() else { return };
                let origin = pos.to_logical::<f64>(scale);
                let dims = size.to_logical::<f64>(scale);

                let widget = *hover_rect().lock().expect("hover rect poisoned");
                let next = to_window_local(
                    cursor,
                    (origin.x, origin.y),
                    (dims.width, dims.height),
                    widget,
                );

                let mut slot = previous.lock().expect("cursor state poisoned");
                // Leaving matters as much as entering, so an outside sample is
                // still emitted once — but repeated outside samples are dropped.
                let changed = match *slot {
                    None => true,
                    Some(prev) => {
                        prev.inside != next.inside
                            || (next.inside
                                && ((prev.x - next.x).abs() >= 1.0
                                    || (prev.y - next.y).abs() >= 1.0))
                    }
                };

                if changed {
                    // The window's transparent margin would otherwise swallow
                    // clicks meant for whatever is behind it, so it is only
                    // opaque to the mouse while the cursor is on the widget.
                    if slot.map(|p| p.inside) != Some(next.inside) {
                        if let Ok(panel) =
                            tauri_nspanel::ManagerExt::get_webview_panel(window.app_handle(), "widget")
                        {
                            panel.set_ignore_mouse_events(!next.inside);
                        }
                    }

                    // Emitted through the AppHandle, not the window: a
                    // WebviewWindow emit is scoped to that window's own
                    // listeners and does not reach the page's global `listen`.
                    let _ = window.app_handle().emit(CURSOR_EVENT, next);
                    *slot = Some(next);
                }

                let mut press = press.lock().expect("press state poisoned");

                let moved_enough = exceeds_drag_threshold(press.origin, cursor);
                let action = press_step(
                    press.raw_down,
                    press.active,
                    press.dragging,
                    is_down,
                    next.inside,
                    moved_enough,
                );

                match action {
                    PressAction::Begin => {
                        press.active = true;
                        press.dragging = false;
                        press.origin = cursor;
                        press.grab = (next.x, next.y);
                    }
                    PressAction::Drag => {
                        press.dragging = true;
                        let (x, y) = dragged_origin(cursor, press.grab);
                        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
                    }
                    PressAction::DragEnd => {
                        crate::window::persist_position(&window);
                        press.active = false;
                        press.dragging = false;
                    }
                    PressAction::Click => {
                        let _ = window.app_handle().emit(CLICK_EVENT, next);
                        press.active = false;
                    }
                    PressAction::Ignore => {}
                }

                press.raw_down = is_down;
            });
        }
    });

    stop
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: (f64, f64) = (-2020.0, -1252.0);
    const SIZE: (f64, f64) = (101.0, 53.0);

    fn local(cursor: (f64, f64)) -> CursorPosition {
        to_window_local(cursor, ORIGIN, SIZE, None)
    }

    #[test]
    fn the_widget_rect_narrows_what_counts_as_inside() {
        // The window is sized to the widest state, so most of it is empty
        // transparent margin that must not read as hovering the widget.
        let widget = Some(Rect { x: 30.0, y: 30.0, width: 40.0, height: 20.0 });
        let big = (600.0, 200.0);

        let on_pill = to_window_local((ORIGIN.0 + 50.0, ORIGIN.1 + 40.0), ORIGIN, big, widget);
        assert!(on_pill.inside);
        assert_eq!((on_pill.x, on_pill.y), (50.0, 40.0));

        // Well inside the window, but in the margin beside the pill.
        let in_margin = to_window_local((ORIGIN.0 + 400.0, ORIGIN.1 + 40.0), ORIGIN, big, widget);
        assert!(!in_margin.inside);
        // Coordinates are still window-local, for the page's own hit-testing.
        assert_eq!((in_margin.x, in_margin.y), (400.0, 40.0));
    }

    #[test]
    fn a_press_starting_on_the_widget_begins_a_gesture() {
        assert_eq!(
            press_step(false, false, false, true, true, false),
            PressAction::Begin
        );
    }

    #[test]
    fn a_button_held_down_elsewhere_is_ignored() {
        // Regression: mirroring the raw button state made any click in another
        // app look like a drag in progress, teleporting the widget to the
        // cursor with a grab offset of (0,0).
        assert_eq!(
            press_step(true, false, false, true, false, true),
            PressAction::Ignore
        );
        assert_eq!(
            press_step(true, false, false, true, true, true),
            PressAction::Ignore
        );
    }

    #[test]
    fn an_active_press_that_moves_far_enough_drags() {
        assert_eq!(
            press_step(true, true, false, true, true, true),
            PressAction::Drag
        );
    }

    #[test]
    fn an_active_press_keeps_dragging_once_it_has_started() {
        assert_eq!(
            press_step(true, true, true, true, false, false),
            PressAction::Drag
        );
    }

    #[test]
    fn an_active_press_that_has_not_moved_is_still_pending() {
        assert_eq!(
            press_step(true, true, false, true, true, false),
            PressAction::Ignore
        );
    }

    #[test]
    fn releasing_without_moving_is_a_click() {
        assert_eq!(
            press_step(true, true, false, false, true, false),
            PressAction::Click
        );
    }

    #[test]
    fn releasing_after_moving_ends_the_drag() {
        assert_eq!(
            press_step(true, true, true, false, true, false),
            PressAction::DragEnd
        );
    }

    #[test]
    fn a_release_with_no_active_press_does_nothing() {
        assert_eq!(
            press_step(true, false, false, false, true, false),
            PressAction::Ignore
        );
    }

    #[test]
    fn a_small_movement_while_held_is_not_a_drag() {
        assert!(!exceeds_drag_threshold((100.0, 100.0), (102.0, 101.0)));
    }

    #[test]
    fn a_movement_past_the_threshold_is_a_drag() {
        assert!(exceeds_drag_threshold((100.0, 100.0), (110.0, 100.0)));
        assert!(exceeds_drag_threshold((100.0, 100.0), (100.0, 90.0)));
    }

    #[test]
    fn dragging_keeps_the_grabbed_point_under_the_cursor() {
        // Grabbed 28px into a window that started at -2020: moving the cursor
        // to -1900 must put the window at -1928.
        assert_eq!(dragged_origin((-1900.0, -1200.0), (28.0, 14.0)), (-1928.0, -1214.0));
    }

    #[test]
    fn a_press_beginning_on_the_widget_is_a_click() {
        assert!(is_click_edge(false, true, true));
    }

    #[test]
    fn a_held_button_is_not_a_repeated_click() {
        assert!(!is_click_edge(true, true, true));
    }

    #[test]
    fn a_press_outside_the_widget_is_not_a_click() {
        assert!(!is_click_edge(false, true, false));
    }

    #[test]
    fn a_release_is_not_a_click() {
        assert!(!is_click_edge(true, false, true));
    }

    #[test]
    fn a_point_inside_the_window_maps_to_local_coordinates() {
        let p = local((-1980.0, -1237.0));
        assert_eq!((p.x, p.y), (40.0, 15.0));
        assert!(p.inside);
    }

    #[test]
    fn a_point_left_of_the_window_is_outside() {
        assert!(!local((-2100.0, -1237.0)).inside);
    }

    #[test]
    fn a_point_right_of_the_window_is_outside() {
        assert!(!local((-1900.0, -1237.0)).inside);
    }

    #[test]
    fn a_point_above_the_window_is_outside() {
        assert!(!local((-1980.0, -1300.0)).inside);
    }

    #[test]
    fn a_point_below_the_window_is_outside() {
        assert!(!local((-1980.0, -1100.0)).inside);
    }

    #[test]
    fn the_window_edges_count_as_inside() {
        assert!(local(ORIGIN).inside);
        assert!(local((ORIGIN.0 + SIZE.0, ORIGIN.1 + SIZE.1)).inside);
    }

    #[test]
    fn a_window_at_the_global_origin_needs_no_translation() {
        let p = to_window_local((40.0, 15.0), (0.0, 0.0), SIZE, None);
        assert_eq!((p.x, p.y), (40.0, 15.0));
        assert!(p.inside);
    }
}
