//! Windows side of ScreenStitch shared by the tray app and the settings
//! window: what's connected, how big it really is, and the saved layouts.

pub mod autostart;
pub mod config;
pub mod display;
pub mod edid;
pub mod tray;
pub mod wide;

use config::{Config, DeskRect, Profile};
use display::Display;
use screenstitch_core::{Layout, Monitor, RectF, auto_arrange};

/// The desk rectangles for the connected displays, in `displays` order: the
/// saved profile for this exact set of monitors, or a fresh guess from the
/// Windows arrangement (stored in `config`; returns true when it guessed).
pub fn desk_rects(config: &mut Config, displays: &[Display]) -> (Vec<RectF>, bool) {
    let key = config::profile_key(displays.iter().map(|d| d.id.as_str()));
    if let Some(p) = config.profiles.get(&key)
        && displays.iter().all(|d| p.monitors.contains_key(&d.id))
    {
        let rects = displays
            .iter()
            .map(|d| {
                let r = p.monitors[&d.id];
                upgrade_rounded_size(RectF::new(r.x, r.y, r.w, r.h), d.size_mm)
            })
            .collect();
        return (rects, false);
    }
    let rects = auto_rects(displays);
    save_rects(config, displays, &rects);
    (rects, true)
}

/// Layouts saved before whole-centimetre EDID sizes were refined still hold the
/// rounded size (e.g. 600 x 340 mm). Swap in the precise one, keeping the
/// screen centred where the user put it. A size the user typed differs by more
/// than rounding and is left alone.
fn upgrade_rounded_size(r: RectF, detected: (f64, f64)) -> RectF {
    let rounded = r.w % 10.0 == 0.0 && r.h % 10.0 == 0.0;
    let close = (r.w - detected.0).abs() <= 6.0 && (r.h - detected.1).abs() <= 6.0;
    let differs = (r.w - detected.0).abs() > 0.01 || (r.h - detected.1).abs() > 0.01;
    if !(rounded && close && differs) {
        return r;
    }
    let (w, h) = detected;
    RectF::new(r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0, w, h)
}

/// Layout guessed purely from Windows' arrangement and EDID sizes.
pub fn auto_rects(displays: &[Display]) -> Vec<RectF> {
    let px: Vec<_> = displays.iter().map(|d| d.px).collect();
    let mm: Vec<_> = displays.iter().map(|d| d.size_mm).collect();
    let primary = displays.iter().position(|d| d.primary).unwrap_or(0);
    auto_arrange(&px, &mm, primary)
}

/// Store `rects` as this monitor set's profile (not yet written to disk).
pub fn save_rects(config: &mut Config, displays: &[Display], rects: &[RectF]) {
    let key = config::profile_key(displays.iter().map(|d| d.id.as_str()));
    let monitors =
        displays.iter().zip(rects).map(|(d, r)| (d.id.clone(), DeskRect { x: r.x, y: r.y, w: r.w, h: r.h })).collect();
    config.profiles.insert(key, Profile { monitors });
}

/// Desk height (mm from the top of the layout) of the "Check alignment" line:
/// the middle of the band every screen covers, or of the widest overlap.
pub fn alignment_line_mm(rects: &[RectF]) -> f64 {
    let top = rects.iter().map(|r| r.y).fold(f64::NEG_INFINITY, f64::max);
    let bottom = rects.iter().map(|r| r.bottom()).fold(f64::INFINITY, f64::min);
    if top < bottom {
        return (top + bottom) / 2.0;
    }
    // No band shared by all: use the middle of the tallest screen.
    rects.iter().max_by(|a, b| a.h.total_cmp(&b.h)).map_or(0.0, |r| r.y + r.h / 2.0)
}

pub fn layout(displays: &[Display], rects: &[RectF], stop_at_gaps: bool) -> Layout {
    Layout::with_gaps(displays.iter().zip(rects).map(|(d, r)| Monitor::new(d.px, *r)).collect(), stop_at_gaps)
}
