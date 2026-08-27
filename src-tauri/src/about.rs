//! The About panel.
//!
//! Shown by hand rather than with `PredefinedMenuItem::about`, which cannot
//! work here. That item is handled entirely inside muda: it calls
//! `orderFrontStandardAboutPanelWithOptions:` and returns *without* sending a
//! `MenuEvent`, so `tray::on_menu_event` never hears about the click and has
//! nowhere to hook.
//!
//! The hook is the whole point. AppKit builds the panel at
//! `NSNormalWindowLevel`, and macOS draws an inactive application's
//! normal-level windows behind the frontmost app — so in a menu-bar app that
//! never activates, the panel opens *underneath* whatever the user is looking
//! at. It is fully realised and `isVisible` is true; it is simply behind
//! something. The panel has to be followed by an explicit activation, and that
//! is what this module exists to do.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAboutPanelOptionApplicationName, NSAboutPanelOptionApplicationVersion,
    NSAboutPanelOptionCredits, NSAboutPanelOptionVersion, NSApplication,
};
use objc2_foundation::{NSAttributedString, NSDictionary, NSString};
use tauri::AppHandle;

/// Author and repository, shown in the panel's info area.
///
/// Plain text: the panel will not turn a URL into a link. `website`, which
/// would have, is one of the fields macOS ignores.
const CREDITS: &str = "Created by Norbert Suski\ngithub.com/norbertsuski/claude-buddy";

/// Open the standard About panel and bring it to the front.
///
/// Must run on the main thread, like every other AppKit call reached from the
/// tray menu.
pub fn show(app: &AppHandle) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let ns_app = NSApplication::sharedApplication(mtm);

    // The version comes from `tauri.conf.json`, the same single source
    // `chore: bump` moves. Nothing about the version is restated here.
    let version = NSString::from_str(&app.package_info().version.to_string());
    let name = NSString::from_str("claude-buddy");
    let credits = NSAttributedString::from_nsstring(&NSString::from_str(CREDITS));
    let copyright = NSString::from_str("MIT licensed");
    // Blank on purpose. AppKit renders this one in parentheses after the
    // version, and both it and `ApplicationVersion` resolve to the same number
    // here — so left alone the panel reads "Version 0.6.0 (0.6.0)".
    let build = NSString::from_str("");

    // Only the keys macOS honours. There is no `NSAboutPanelOptionCopyright`
    // in the bindings — AppKit never exported one — so it goes in by its
    // literal name, exactly as muda does it.
    //
    // The icon key is deliberately absent: with nothing specified AppKit uses
    // `NSApplicationIcon` from the bundle, which is the app's own logo and one
    // fewer thing to keep in step. An unbundled `tauri dev` run gets the
    // generic icon instead, which is a dev-only cosmetic difference.
    let keys: [&NSString; 5] = [
        unsafe { NSAboutPanelOptionApplicationName },
        unsafe { NSAboutPanelOptionApplicationVersion },
        unsafe { NSAboutPanelOptionVersion },
        unsafe { NSAboutPanelOptionCredits },
        &NSString::from_str("Copyright"),
    ];
    let values: [Retained<AnyObject>; 5] = [
        Retained::into_super(Retained::into_super(name)),
        Retained::into_super(Retained::into_super(version)),
        Retained::into_super(Retained::into_super(build)),
        Retained::into_super(Retained::into_super(credits)),
        Retained::into_super(Retained::into_super(copyright)),
    ];
    let options = NSDictionary::from_retained_objects(&keys, &values);

    // SAFETY: every value matches the type its key documents — NSString for
    // the name, version and copyright, NSAttributedString for the credits.
    unsafe { ns_app.orderFrontStandardAboutPanelWithOptions(&options) };

    // The reason this module exists. Without it the panel sits at normal window
    // level behind the frontmost application and the click looks like it did
    // nothing at all.
    //
    // `ignoringOtherApps` rather than a plain activate: the user clicked a
    // menu-bar item belonging to an app that is not, and does not become,
    // frontmost, so there is nothing gentler that would raise it.
    #[allow(deprecated)]
    ns_app.activateIgnoringOtherApps(true);
}
