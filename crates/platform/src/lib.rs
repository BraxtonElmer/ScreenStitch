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
use screenstitch_core::{Layout, Monitor, RectF, auto_arrange_around};

/// The desk rectangles for the connected displays, in `displays` order.
///
/// A layout the user placed for this exact set of monitors is used as is.
/// Otherwise it's guessed: screens the user already lined up in another set
/// (say, the two that were there before a third was plugged in) keep that
/// placement, and only the rest come from the Windows arrangement. The guess
/// is stored in `config`; returns true when it changed and needs saving.
pub fn desk_rects(config: &mut Config, displays: &[Display]) -> (Vec<RectF>, bool) {
    let key = config::profile_key(displays.iter().map(|d| d.id.as_str()));
    let saved = config.profiles.get(&key).filter(|p| displays.iter().all(|d| p.monitors.contains_key(&d.id)));
    let placed_by_user = match saved.map(|p| p.guessed) {
        Some(Some(false)) => true,
        // Saved before guesses were marked: trust it unless a smaller set the
        // user arranged knows better.
        Some(None) => placed_source(config, displays, &key, true).is_none(),
        _ => false,
    };
    if placed_by_user && let Some(p) = saved {
        return (rects_from(p, displays), false);
    }

    let fixed: Vec<Option<RectF>> = match placed_source(config, displays, &key, false) {
        Some(src) => {
            let p = &config.profiles[&src];
            displays.iter().map(|d| p.monitors.get(&d.id).map(|r| upgrade(r, d))).collect()
        }
        None => vec![None; displays.len()],
    };
    let rects = arrange(displays, &fixed);
    if saved.is_some_and(|p| p.guessed == Some(true) && rects_from(p, displays) == rects) {
        return (rects, false);
    }
    store(config, displays, &rects, true);
    (rects, true)
}

/// The profile the user arranged that shares the most screens (at least two)
/// with `displays`, preferring the one with the fewest screens not connected
/// now. With `subset_only`, every screen in it must be connected now.
fn placed_source(config: &Config, displays: &[Display], key: &str, subset_only: bool) -> Option<String> {
    config
        .profiles
        .iter()
        .filter(|(k, p)| *k != key && p.guessed != Some(true))
        .filter_map(|(k, p)| {
            let shared = displays.iter().filter(|d| p.monitors.contains_key(&d.id)).count();
            let extra = p.monitors.len() - shared;
            (shared >= 2 && !(subset_only && extra > 0)).then_some((shared, extra, k))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, _, k)| k.clone())
}

fn rects_from(p: &Profile, displays: &[Display]) -> Vec<RectF> {
    displays.iter().map(|d| upgrade(&p.monitors[&d.id], d)).collect()
}

fn upgrade(r: &DeskRect, d: &Display) -> RectF {
    upgrade_rounded_size(RectF::new(r.x, r.y, r.w, r.h), d.size_mm)
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
    arrange(displays, &vec![None; displays.len()])
}

fn arrange(displays: &[Display], fixed: &[Option<RectF>]) -> Vec<RectF> {
    let px: Vec<_> = displays.iter().map(|d| d.px).collect();
    let mm: Vec<_> = displays.iter().map(|d| d.size_mm).collect();
    let primary = displays.iter().position(|d| d.primary).unwrap_or(0);
    auto_arrange_around(&px, &mm, primary, fixed)
}

/// Store `rects`, placed by the user, as this monitor set's profile (not yet
/// written to disk).
pub fn save_rects(config: &mut Config, displays: &[Display], rects: &[RectF]) {
    store(config, displays, rects, false);
}

fn store(config: &mut Config, displays: &[Display], rects: &[RectF], guessed: bool) {
    let key = config::profile_key(displays.iter().map(|d| d.id.as_str()));
    let monitors =
        displays.iter().zip(rects).map(|(d, r)| (d.id.clone(), DeskRect { x: r.x, y: r.y, w: r.w, h: r.h })).collect();
    config.profiles.insert(key, Profile { monitors, guessed: Some(guessed) });
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

#[cfg(test)]
mod tests {
    use super::*;
    use screenstitch_core::RectI;

    fn display(id: &str, px: RectI, mm: (f64, f64), primary: bool) -> Display {
        Display {
            id: id.into(),
            name: id.into(),
            px,
            size_mm: mm,
            size_from_edid: true,
            primary,
            scale: 100,
            number: 1,
        }
    }

    fn pair() -> Vec<Display> {
        vec![
            display("A", RectI::new(0, 0, 2560, 1440), (597.0, 336.0), true),
            display("B", RectI::new(2560, 0, 3640, 1920), (296.0, 527.0), false),
        ]
    }

    fn with_third() -> Vec<Display> {
        let mut d = pair();
        d.push(display("C", RectI::new(-1920, 0, 0, 1080), (527.0, 296.0), false));
        d
    }

    fn offset(r: &[RectF]) -> (f64, f64) {
        (r[1].x - r[0].x, r[1].y - r[0].y)
    }

    #[test]
    fn plugging_in_a_screen_keeps_the_ones_already_lined_up() {
        let mut config = Config::default();
        let placed = [RectF::new(0.0, 95.5, 597.0, 336.0), RectF::new(597.0, 0.0, 296.0, 527.0)];
        save_rects(&mut config, &pair(), &placed);

        let (r, changed) = desk_rects(&mut config, &with_third());
        assert!(changed);
        assert_eq!(offset(&r), offset(&placed), "{r:?}");

        // Unplugging it again gives back exactly what the user placed.
        assert_eq!(desk_rects(&mut config, &pair()), (placed.to_vec(), false));
    }

    #[test]
    fn a_layout_placed_for_all_screens_wins() {
        let mut config = Config::default();
        save_rects(&mut config, &pair(), &[RectF::new(0.0, 95.5, 597.0, 336.0), RectF::new(597.0, 0.0, 296.0, 527.0)]);
        let mine = [
            RectF::new(527.0, 0.0, 597.0, 336.0),
            RectF::new(1124.0, 10.0, 296.0, 527.0),
            RectF::new(0.0, 40.0, 527.0, 296.0),
        ];
        save_rects(&mut config, &with_third(), &mine);
        assert_eq!(desk_rects(&mut config, &with_third()), (mine.to_vec(), false));
    }

    #[test]
    fn old_guess_for_the_bigger_set_is_replaced() {
        // 0.2.0 saved the guess for all three without marking it as one.
        let mut config = Config::default();
        let placed = [RectF::new(0.0, 95.5, 597.0, 336.0), RectF::new(597.0, 0.0, 296.0, 527.0)];
        save_rects(&mut config, &pair(), &placed);
        store(&mut config, &with_third(), &auto_rects(&with_third()), false);
        for p in config.profiles.values_mut() {
            p.guessed = None;
        }
        let (r, _) = desk_rects(&mut config, &with_third());
        assert_eq!(offset(&r), offset(&placed), "{r:?}");
        assert_eq!(desk_rects(&mut config, &pair()).0, placed.to_vec());
    }
}
