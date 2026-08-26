//! Geometry of the notch on a built-in MacBook display.
//!
//! One AppKit probe, and pure arithmetic on top of it. The probe needs a
//! display and the main thread; everything derived from it is a free function
//! over [`NotchGeometry`], so the placement maths is tested without either.
//!
//! Nothing here touches a window. `window::place_in_notch` is the only caller
//! that turns [`window_frame`] into a real frame, and the frontend turns
//! [`flank_rects`] into the chips it actually draws.

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;

use crate::cursor::Rect;

/// How far a chip may expand away from the notch, in points.
///
/// `auxiliaryTopLeftArea` reports the flank's *total* width, not the part of it
/// that is free — where the frontmost app's menu titles end is not observable
/// without Accessibility, and it changes on every app switch. So expansion is
/// capped rather than clamped, and a chip occludes whatever is under it for as
/// long as the cursor is on it.
pub const FLANK_BUDGET: f64 = 200.0;

/// The notched display, in the top-left-origin, y-down space that Tauri's
/// `set_position` uses.
///
/// `screen_width` is kept rather than the two flank widths: the flanks are
/// derived from it and the notch, so storing all three would let them disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotchGeometry {
    /// The display's top-left corner in global coordinates. Not the origin of
    /// the global space unless this display happens to be the primary one.
    pub screen_origin: (f64, f64),
    pub screen_width: f64,
    /// Left edge of the notch, relative to the display's own left edge.
    pub notch_x: f64,
    pub notch_width: f64,
    /// `safeAreaInsets.top`: the menu bar is taller on a notched display than
    /// the usual 24pt, precisely so the notch fits inside it.
    pub bar_height: f64,
}

impl NotchGeometry {
    /// Menu bar strip to the left of the notch, where app menu titles live.
    pub fn left_flank(&self) -> f64 {
        self.notch_x
    }

    /// Menu bar strip to the right of the notch, where the menu bar extras live.
    pub fn right_flank(&self) -> f64 {
        (self.screen_width - self.notch_x - self.notch_width).max(0.0)
    }
}

/// Where the widget window goes, and how big it is.
///
/// Deliberately *not* the full width of the menu bar. The window spans the notch
/// plus one budget either side — about 590pt — so it never covers the Apple menu
/// or the clock at all, whatever it does to the menu titles nearest the notch.
///
/// The width does not depend on how much content there is, for the same reason
/// `useWidgetSize::widgetWindowSize` holds the free-mode window at its widest
/// state: resizing a transparent panel shows one unpainted frame, and it lands
/// on the start of the morph.
pub fn window_frame(
    geo: &NotchGeometry,
    budget: f64,
    popover_allowance: f64,
) -> ((f64, f64), (f64, f64)) {
    let width = geo.notch_width + budget * 2.0;
    let notch_centre = geo.notch_x + geo.notch_width / 2.0;
    let origin = (
        geo.screen_origin.0 + notch_centre - width / 2.0,
        geo.screen_origin.1,
    );
    (origin, (width, geo.bar_height + popover_allowance))
}

/// The two menu-bar rects the chips may occupy, in window-local coordinates.
///
/// These are the *budget* rects, not the drawn ones: the chips are sized from
/// their own content, and the frontend reports what it actually drew through
/// `set_hover_rects`. This is the fallback used until that first report lands,
/// which mirrors how `cursor::to_window_local` falls back to the whole window.
///
/// The window is centred on the notch, so the left budget starts at the window's
/// own left edge and the right budget starts one notch-width later.
pub fn flank_rects(geo: &NotchGeometry, budget: f64) -> (Rect, Rect) {
    let left = Rect { x: 0.0, y: 0.0, width: budget, height: geo.bar_height };
    let right = Rect {
        x: budget + geo.notch_width,
        y: 0.0,
        width: budget,
        height: geo.bar_height,
    };
    (left, right)
}

/// Window-local x of the notch's left and right edges.
///
/// The frontend needs these to sit its chips flush against the notch. It could
/// derive them from the budget, but that would duplicate the centring rule in
/// two languages, and the centring is the part that would be wrong.
pub fn notch_edges(geo: &NotchGeometry, budget: f64) -> (f64, f64) {
    (budget, budget + geo.notch_width)
}

/// The notched display, or `None` when there is not one.
///
/// `safeAreaInsets.top > 0` is true only on a notched built-in panel, so it
/// doubles as the built-in check and no `CGDisplayIsBuiltin` call is needed.
///
/// Returns `None` off the main thread rather than risking it: `NSScreen` is
/// main-thread-only, and AppKit geometry accessors fail silently elsewhere —
/// which is how the widget would end up placed at (0, 0) with no error.
pub fn probe() -> Option<NotchGeometry> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);

    // AppKit's global space has its origin at the bottom-left of whichever
    // display sits at (0, 0), with y increasing upwards; Tauri's has it at the
    // top-left of the same display with y increasing downwards. Flipping needs
    // that display's height, found by its origin rather than by `mainScreen`,
    // which follows the key window and is not necessarily this one.
    let flip_height = screens
        .iter()
        .find(|s| {
            let f = s.frame();
            f.origin.x == 0.0 && f.origin.y == 0.0
        })
        .or_else(|| screens.iter().next())
        .map(|s| s.frame().size.height)?;

    screens.iter().find_map(|screen| {
        let bar_height = screen.safeAreaInsets().top;
        if bar_height <= 0.0 {
            return None;
        }

        let frame = screen.frame();
        let left = screen.auxiliaryTopLeftArea().size.width;
        let right = screen.auxiliaryTopRightArea().size.width;
        let notch_width = frame.size.width - left - right;

        // A notched screen that reports no auxiliary areas would give a notch as
        // wide as the display, which would place the chips off both edges. Treat
        // it as no notch rather than trusting it.
        if notch_width <= 0.0 || notch_width >= frame.size.width {
            return None;
        }

        Some(NotchGeometry {
            screen_origin: (
                frame.origin.x,
                flip_height - (frame.origin.y + frame.size.height),
            ),
            screen_width: frame.size.width,
            notch_x: left,
            notch_width,
            bar_height,
        })
    })
}

/// What the frontend needs to draw the chips, in window-local points.
///
/// Deliberately not the whole [`NotchGeometry`]: the page has no business
/// knowing about global screen coordinates, and handing it any would invite a
/// second placement calculation in a second language.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotchLayout {
    /// Window-local x where the notch begins; the left chip's right edge.
    pub notch_left: f64,
    /// Window-local x where the notch ends; the right chip's left edge.
    pub notch_right: f64,
    pub bar_height: f64,
    pub budget: f64,
}

/// Last probed geometry.
///
/// Probing needs the main thread, and a `#[tauri::command]` carries no promise
/// of running there — `MainThreadMarker::new()` would return `None` and the
/// notch would read as absent. So the probe happens where the thread is known
/// (setup, and the display-parameters observer) and everything else reads this.
static GEOMETRY: std::sync::OnceLock<std::sync::Mutex<Option<NotchGeometry>>> =
    std::sync::OnceLock::new();

fn geometry() -> &'static std::sync::Mutex<Option<NotchGeometry>> {
    GEOMETRY.get_or_init(|| std::sync::Mutex::new(None))
}

/// Re-probe and store. Call from the main thread, on startup and whenever the
/// display configuration changes.
pub fn refresh() -> Option<NotchGeometry> {
    let found = probe();
    *geometry().lock().expect("notch geometry poisoned") = found;
    found
}

/// The geometry as last probed, without touching AppKit.
pub fn cached() -> Option<NotchGeometry> {
    *geometry().lock().expect("notch geometry poisoned")
}

/// How often the display configuration is re-read.
///
/// Nothing in the app observes displays appearing or disappearing, and notch
/// mode cannot do without it: closing the lid takes the notched display away,
/// and the window would be parked on a screen that no longer exists with drag
/// suppressed and no way to recover it.
///
/// A poll rather than `NSApplicationDidChangeScreenParametersNotification`,
/// which is the mechanism this wants: observing it needs a retained Objective-C
/// block, and the payoff over one `NSScreen` read every two seconds is latency
/// nobody can perceive on an event that happens when a lid opens. The cursor is
/// sampled on the same reasoning, sixty times as often.
pub const GEOMETRY_POLL: std::time::Duration = std::time::Duration::from_secs(2);

/// Re-probed geometry, pushed to the frontend so it can redraw or stop drawing.
pub const NOTCH_EVENT: &str = "notch://layout";

/// Watch for the display configuration changing, and announce it when it does.
///
/// Emits only on change, so an unchanging setup costs one `NSScreen` read per
/// tick and nothing downstream.
pub fn spawn_geometry_watcher(app: tauri::AppHandle) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    use std::sync::atomic::{AtomicBool, Ordering};

    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    std::thread::spawn(move || {
        while !stop_thread.load(Ordering::Relaxed) {
            std::thread::sleep(GEOMETRY_POLL);

            let app = app.clone();
            // NSScreen is main-thread only, and AppKit geometry accessors fail
            // silently elsewhere rather than erroring.
            let _ = app.clone().run_on_main_thread(move || {
                let before = cached();
                let after = refresh();
                if before != after {
                    use tauri::Emitter;
                    let _ = app.emit(NOTCH_EVENT, notch_layout());
                }
            });
        }
    });

    stop
}

/// Chip placement for the frontend, or `None` where there is no notch.
#[tauri::command]
pub fn notch_layout() -> Option<NotchLayout> {
    let geo = cached()?;
    let (notch_left, notch_right) = notch_edges(&geo, FLANK_BUDGET);
    Some(NotchLayout {
        notch_left,
        notch_right,
        bar_height: geo.bar_height,
        budget: FLANK_BUDGET,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 14-inch MacBook Pro: 1512pt wide, 37pt menu bar, notch centred.
    fn built_in() -> NotchGeometry {
        NotchGeometry {
            screen_origin: (0.0, 0.0),
            screen_width: 1512.0,
            notch_x: 661.0,
            notch_width: 190.0,
            bar_height: 37.0,
        }
    }

    #[test]
    fn the_flanks_are_what_the_notch_leaves_over() {
        let geo = built_in();
        assert_eq!(geo.left_flank(), 661.0);
        assert_eq!(geo.right_flank(), 661.0);
        assert_eq!(
            geo.left_flank() + geo.notch_width + geo.right_flank(),
            geo.screen_width
        );
    }

    #[test]
    fn the_window_is_the_notch_plus_a_budget_either_side() {
        let (origin, size) = window_frame(&built_in(), FLANK_BUDGET, 400.0);
        assert_eq!(size, (590.0, 437.0));
        // Centred on the notch centre at 756, so 756 - 295.
        assert_eq!(origin, (461.0, 0.0));
    }

    #[test]
    fn the_window_never_reaches_the_screen_edges() {
        // The whole point of budgeting rather than spanning the bar: the Apple
        // menu and the clock are never covered, only the titles near the notch.
        let geo = built_in();
        let (origin, size) = window_frame(&geo, FLANK_BUDGET, 400.0);
        assert!(origin.0 > 0.0);
        assert!(origin.0 + size.0 < geo.screen_width);
    }

    #[test]
    fn the_window_follows_the_notch_when_it_is_off_centre() {
        // Symmetry is not assumed anywhere: notch_x comes from the auxiliary
        // area rather than from screen_width / 2.
        let geo = NotchGeometry { notch_x: 500.0, ..built_in() };
        let (origin, _) = window_frame(&geo, FLANK_BUDGET, 400.0);
        assert_eq!(origin.0, 500.0 + 95.0 - 295.0);
    }

    #[test]
    fn a_secondary_display_origin_is_carried_into_the_placement() {
        // A display left of and above the primary one sits at a negative global
        // origin, and a local value passed straight to set_position would land
        // in the dead space between displays.
        let geo = NotchGeometry { screen_origin: (-1512.0, -400.0), ..built_in() };
        let (origin, _) = window_frame(&geo, FLANK_BUDGET, 400.0);
        assert_eq!(origin, (-1512.0 + 461.0, -400.0));
    }

    #[test]
    fn the_budget_rects_sit_either_side_of_the_notch() {
        let (left, right) = flank_rects(&built_in(), FLANK_BUDGET);
        assert_eq!((left.x, left.width), (0.0, 200.0));
        assert_eq!((right.x, right.width), (390.0, 200.0));
        // Both live in the menu bar and nowhere below it.
        assert_eq!((left.y, left.height), (0.0, 37.0));
        assert_eq!((right.y, right.height), (0.0, 37.0));
    }

    #[test]
    fn nothing_in_the_budget_rects_overlaps_the_notch() {
        let geo = built_in();
        let (left, right) = flank_rects(&geo, FLANK_BUDGET);
        let (notch_left, notch_right) = notch_edges(&geo, FLANK_BUDGET);
        assert_eq!(left.x + left.width, notch_left);
        assert_eq!(right.x, notch_right);
    }

    #[test]
    fn the_notch_edges_are_where_the_chips_meet_it() {
        let geo = built_in();
        let (window_origin, size) = window_frame(&geo, FLANK_BUDGET, 0.0);
        let (notch_left, notch_right) = notch_edges(&geo, FLANK_BUDGET);

        // Converting both edges back to screen-local must land on the notch.
        assert_eq!(window_origin.0 + notch_left, geo.notch_x);
        assert_eq!(window_origin.0 + notch_right, geo.notch_x + geo.notch_width);
        assert_eq!(notch_right - notch_left, geo.notch_width);
        assert_eq!(size.0 - notch_right, FLANK_BUDGET);
    }

    #[test]
    fn a_budget_wider_than_the_flank_still_produces_a_sane_window() {
        // A future model with a wider notch or narrower flanks must not produce
        // a negative width or an inverted rect, even where the window then
        // extends past the screen edge.
        let geo = NotchGeometry {
            screen_width: 600.0,
            notch_x: 150.0,
            notch_width: 300.0,
            ..built_in()
        };
        let (_, size) = window_frame(&geo, FLANK_BUDGET, 0.0);
        assert_eq!(size.0, 700.0);
        let (left, right) = flank_rects(&geo, FLANK_BUDGET);
        assert!(left.width > 0.0 && right.width > 0.0);
        assert!(right.x > left.x + left.width);
    }

    #[test]
    fn a_notch_wider_than_the_screen_reports_no_right_flank() {
        // Guards the subtraction rather than the probe: `right_flank` must not
        // go negative and hand a negative width to a rect.
        let geo = NotchGeometry { notch_x: 1400.0, notch_width: 190.0, ..built_in() };
        assert_eq!(geo.right_flank(), 0.0);
    }
}
