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

/// The slab's width as a share of the display, and the bounds on it.
///
/// One width at rest and open both, so only the height ever animates. The band
/// in the menu bar and the list below it are therefore the same width too, and
/// there is no join to treat — no flare, no concave fillets where a wide panel
/// meets a narrow slot.
///
/// A third of the display, so it travels between panels. On 1470pt that is 490,
/// spanning 490 to 980 about a notch centred at 735.
///
/// This does reach under the menu bar extras — the leftmost starts at 910 on the
/// panel this was measured on. Accepted, because the width applies only while
/// the cursor is on the widget: the slab is over the extras for as long as it is
/// open and off them the moment it closes, which is the same trade the flanking
/// chips already make against the app's menu titles. The resting band is
/// content-hugging and stays clear of them, so nothing sits there permanently.
///
/// A narrower share was tried first — 4.3, the widest that clears 910 — and read
/// as broken rather than tidy. It put the open width within 30pt of the resting
/// one, so the slab appeared to shift sideways instead of growing out of the
/// notch: too small a change to look like an expansion, big enough to look like
/// a jump. The animation needs the two widths to be visibly different.
///
/// `SLAB_MAX` bounds the share on a large display, where a third would be far
/// wider than any row needs, and `SLAB_MIN` keeps a row legible on a small one.
pub const SLAB_DIVISOR: f64 = 3.0;
pub const SLAB_MIN: f64 = 260.0;
pub const SLAB_MAX: f64 = 560.0;

/// The slab's width on this display.
pub fn slab_width(geo: &NotchGeometry) -> f64 {
    (geo.screen_width / SLAB_DIVISOR).clamp(SLAB_MIN, SLAB_MAX)
}

/// How far a chip may sit from the notch, in points.
///
/// `auxiliaryTopLeftArea` reports the flank's *total* width, not the part of it
/// that is free — where the frontmost app's menu titles end is not observable
/// without Accessibility, and it changes on every app switch. So expansion is
/// capped rather than clamped, and a chip occludes whatever is under it for as
/// long as the cursor is on it.
///
/// Only the resting chips live here now — the counts on the left and 70pt of
/// limit on the right, measured. The chips retract into the notch on hover
/// rather than expanding outward, so nothing needs room to grow and this no
/// longer bounds the window.
///
/// It does still bound the *counts*, which is easy to miss: `NotchFlanks`
/// places the resting band at `notchLeft - restWidths.left`, and `notchLeft`
/// is this figure, so a counts chip measuring wider than this gets a negative
/// left edge and the window clips it. It was 160 while there were five states
/// and 96pt of counts; the sixth, `tasking`, took them past it and cut the
/// leading waiting count in half. Raised with headroom for a count reaching
/// double digits rather than to the measured width of six single digits.
pub const FLANK_BUDGET: f64 = 200.0;

/// Height reserved below the menu bar for the slab, whether or not it is open.
///
/// Reserved for the reason `useWidgetSize.POPOVER_ALLOWANCE` is: resizing a
/// transparent panel shows one unpainted frame, so opening must not change the
/// window. The reserved area stays transparent and click-through, because the
/// chips are the only rects reported as the widget.
///
/// Larger than the free-mode figure, and no longer a mirror of it. That one
/// sizes a different window around a popover that has not grown; this one has
/// to hold `MAX_ROWS` rows, the usage row and one open row detail, and the
/// detail now carries the popover's whole field list. Eight rows and the
/// footer come to roughly 344, which left about 56 for a detail that is 150.
pub const POPOVER_ALLOWANCE: f64 = 560.0;

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
/// plus one budget either side — about 670pt — so it never covers the Apple menu
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
    let width = half_width(geo, budget) * 2.0;
    let notch_centre = geo.notch_x + geo.notch_width / 2.0;
    let origin = (
        geo.screen_origin.0 + notch_centre - width / 2.0,
        geo.screen_origin.1,
    );
    (origin, (width, geo.bar_height + popover_allowance))
}

/// Half the window's width, and the single place that decides it.
///
/// Whichever is larger: room for a resting chip, or room for the detail card
/// reaching out from the notch-wide panel. Both `window_frame` and
/// `notch_edges` derive from this, because two callers computing it separately
/// is two callers that can disagree about where the notch is.
fn half_width(geo: &NotchGeometry, budget: f64) -> f64 {
    (geo.notch_width / 2.0 + budget).max(slab_width(geo) / 2.0)
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
    let (notch_left, notch_right) = notch_edges(geo, budget);
    let left = Rect {
        x: (notch_left - budget).max(0.0),
        y: 0.0,
        width: budget,
        height: geo.bar_height,
    };
    let right = Rect {
        x: notch_right,
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
    let centre = half_width(geo, budget);
    (
        centre - geo.notch_width / 2.0,
        centre + geo.notch_width / 2.0,
    )
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
    /// Width of the slab, so the page and Rust agree on one number.
    pub slab_width: f64,
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
pub fn spawn_geometry_watcher(
    app: tauri::AppHandle,
) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
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
        slab_width: slab_width(&geo),
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
    fn the_reserve_holds_a_full_list_and_an_open_detail() {
        // Eight rows, the usage row and the bar come to roughly 344, and a row
        // detail carrying the popover's whole field list is about 150. The old
        // 400 left about 56 for it, which clipped the detail it was reserved
        // for. Deliberately not the free-mode figure any more.
        assert!(POPOVER_ALLOWANCE >= 344.0 + 150.0);
    }

    #[test]
    fn the_window_reserves_the_allowance_below_the_bar() {
        let geo = built_in();
        let (_, size) = window_frame(&geo, FLANK_BUDGET, POPOVER_ALLOWANCE);
        assert_eq!(size.1, geo.bar_height + POPOVER_ALLOWANCE);
    }

    #[test]
    fn the_window_is_wide_enough_for_the_detail_card() {
        let geo = built_in();
        let (origin, size) = window_frame(&geo, FLANK_BUDGET, 400.0);
        // Half the notch plus a resting chip, doubled — the chips reach further
        // from the notch's centre than half the slab does.
        assert_eq!(
            size,
            (
                (geo.notch_width / 2.0 + FLANK_BUDGET) * 2.0,
                geo.bar_height + 400.0
            )
        );
        assert!(size.0 >= slab_width(&geo));
        // Centred on the notch centre, which is the screen centre here.
        let notch_centre = geo.notch_x + geo.notch_width / 2.0;
        assert_eq!(origin, (notch_centre - size.0 / 2.0, 0.0));
    }

    #[test]
    fn the_slab_fits_inside_the_window_around_the_notch() {
        // The slab is centred on the notch, and the window is too, so the slab
        // is inside it whichever of the two set the width.
        let geo = built_in();
        let (_, size) = window_frame(&geo, FLANK_BUDGET, 0.0);
        assert!(size.0 >= slab_width(&geo));
        let (notch_left, notch_right) = notch_edges(&geo, FLANK_BUDGET);
        let centre = (notch_left + notch_right) / 2.0;
        assert!(centre - slab_width(&geo) / 2.0 >= 0.0);
        assert!(centre + slab_width(&geo) / 2.0 <= size.0);
    }

    #[test]
    fn the_slab_is_a_share_of_the_display_within_bounds() {
        let geo = built_in();
        assert_eq!(slab_width(&geo), 1512.0 / SLAB_DIVISOR);
    }

    #[test]
    fn the_slab_is_visibly_wider_than_the_resting_band() {
        // The resting band hugs its content: measured at 313pt on this panel,
        // with the counts and the limit's bar either side of a 179pt notch. An
        // open width close to that reads as a sideways jump rather than as an
        // expansion, which is why a narrower share was rejected.
        let geo = built_in();
        assert!(slab_width(&geo) > 313.0 * 1.4);
    }

    #[test]
    fn the_share_is_bounded_at_both_ends() {
        let narrow = NotchGeometry {
            screen_width: 600.0,
            ..built_in()
        };
        assert_eq!(slab_width(&narrow), SLAB_MIN);
        let wide = NotchGeometry {
            screen_width: 6016.0,
            ..built_in()
        };
        assert_eq!(slab_width(&wide), SLAB_MAX);
    }

    #[test]
    fn a_chip_wider_than_the_slab_still_gets_its_room() {
        // The chips win when they reach further from the notch's centre than
        // half the slab does, which is the usual case on real hardware.
        let geo = NotchGeometry {
            notch_width: 10.0,
            ..built_in()
        };
        let (_, size) = window_frame(&geo, 900.0, 0.0);
        assert_eq!(size.0, (10.0 / 2.0 + 900.0) * 2.0);
        assert!(size.0 >= slab_width(&geo));
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
        let geo = NotchGeometry {
            notch_x: 500.0,
            ..built_in()
        };
        let (origin, size) = window_frame(&geo, FLANK_BUDGET, 400.0);
        assert_eq!(origin.0, 500.0 + geo.notch_width / 2.0 - size.0 / 2.0);
    }

    #[test]
    fn a_secondary_display_origin_is_carried_into_the_placement() {
        // A display left of and above the primary one sits at a negative global
        // origin, and a local value passed straight to set_position would land
        // in the dead space between displays.
        let geo = NotchGeometry {
            screen_origin: (-1512.0, -400.0),
            ..built_in()
        };
        let (local, _) = window_frame(&built_in(), FLANK_BUDGET, 400.0);
        let (origin, _) = window_frame(&geo, FLANK_BUDGET, 400.0);
        assert_eq!(origin, (-1512.0 + local.0, -400.0));
    }

    #[test]
    fn the_budget_rects_sit_either_side_of_the_notch() {
        let geo = built_in();
        let (left, right) = flank_rects(&geo, FLANK_BUDGET);
        let (notch_left, notch_right) = notch_edges(&geo, FLANK_BUDGET);
        assert_eq!(
            (left.x, left.width),
            (notch_left - FLANK_BUDGET, FLANK_BUDGET)
        );
        assert_eq!((right.x, right.width), (notch_right, FLANK_BUDGET));
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
        // Symmetric about the notch, so both flanks have the same room.
        assert_eq!(size.0 - notch_right, notch_left);
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
        // A notch wider than the slab wins, which is what keeps the notch inside
        // the window on hardware nobody has shipped yet.
        assert_eq!(size.0, (300.0 / 2.0 + FLANK_BUDGET) * 2.0);
        let (left, right) = flank_rects(&geo, FLANK_BUDGET);
        assert!(left.width > 0.0 && right.width > 0.0);
        assert!(right.x > left.x + left.width);
    }

    #[test]
    fn a_notch_wider_than_the_screen_reports_no_right_flank() {
        // Guards the subtraction rather than the probe: `right_flank` must not
        // go negative and hand a negative width to a rect.
        let geo = NotchGeometry {
            notch_x: 1400.0,
            notch_width: 190.0,
            ..built_in()
        };
        assert_eq!(geo.right_flank(), 0.0);
    }
}
