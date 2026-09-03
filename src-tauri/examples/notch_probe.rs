//! Ask AppKit what it thinks about this display. `cargo run --example notch_probe`.
//!
//! An example rather than a test: `probe` needs the real main thread, and cargo
//! runs each test on a spawned one, where `MainThreadMarker::new()` is None.
fn main() {
    match buddy_core::notch::probe() {
        Some(geo) => {
            println!("{geo:#?}");
            println!("left_flank  = {}", geo.left_flank());
            println!("right_flank = {}", geo.right_flank());
            let (origin, size) = buddy_core::notch::window_frame(
                &geo,
                buddy_core::notch::FLANK_BUDGET,
                buddy_core::notch::POPOVER_ALLOWANCE,
            );
            println!("window origin = {origin:?} size = {size:?}");
        }
        None => println!("no notch found"),
    }
}
