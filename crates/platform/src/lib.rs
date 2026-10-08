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
use screenstitch_core::{Layout, Monitor, RectF, RectI, Side, auto_arrange_around};

/// The desk rectangles for the connected displays, in `displays` order.
///
/// A layout the user placed for this exact set of monitors is used as is.
/// Otherwise it's guessed: screens the user already lined up in another set
/// (say, the two that were there before a third was plugged in) keep that
/// placement, and only the rest come from the Windows arrangement.
///
/// Either way, screens that were rearranged in the Windows display settings
/// since (moved to another side of a neighbour) are placed again from the
/// Windows arrangement, so both always agree on which screen is where.
///
/// The result is stored in `config`; returns true when it changed and needs
/// saving.
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
        let fixed = still_in_place(p, displays);
        if fixed.iter().all(Option::is_some) {
            let rects: Vec<RectF> = fixed.into_iter().flatten().collect();
            let unchanged = displays.iter().all(|d| p.windows.get(&d.id) == Some(&px_array(d)));
            if !unchanged {
                // Same arrangement, new pixel positions: remember them.
                store(config, displays, &rects, false);
            }
            return (rects, !unchanged);
        }
        // Rearranged in Windows: what still sits the same way stays put, and
        // the rest keeps the size the user gave it.
        let sizes: Vec<_> = rects_from(p, displays).iter().map(|r| (r.w, r.h)).collect();
        let rects = arrange_sized(displays, &fixed, &sizes);
        store(config, displays, &rects, false);
        return (rects, true);
    }

    let fixed = match placed_source(config, displays, &key, false) {
        Some(src) => still_in_place(&config.profiles[&src], displays),
        None => vec![None; displays.len()],
    };
    let rects = arrange(displays, &fixed);
    if saved.is_some_and(|p| {
        p.guessed == Some(true)
            && rects_from(p, displays) == rects
            && displays.iter().all(|d| p.windows.get(&d.id) == Some(&px_array(d)))
    }) {
        return (rects, false);
    }
    store(config, displays, &rects, true);
    (rects, true)
}

/// The desk rects `p` holds for `displays`, keeping only screens that still
/// sit on the same side of each other as they do in Windows now. Screens the
/// user moved around in the Windows display settings are left out (`None`)
/// to be placed again.
fn still_in_place(p: &Profile, displays: &[Display]) -> Vec<Option<RectF>> {
    let mut kept: Vec<bool> = displays.iter().map(|d| p.monitors.contains_key(&d.id)).collect();
    let agrees = |i: usize, j: usize| {
        let (a, b) = (&displays[i], &displays[j]);
        let now = touching_side(&a.px, &b.px);
        match (p.windows.get(&a.id), p.windows.get(&b.id)) {
            (Some(pa), Some(pb)) => touching_side(&from_array(pa), &from_array(pb)) == now,
            // Saved before Windows positions were recorded: compare with the desk.
            _ => now.is_none_or(|s| {
                desk_side(&upgrade(&p.monitors[&a.id], a), &upgrade(&p.monitors[&b.id], b)).is_none_or(|d| d == s)
            }),
        }
    };
    loop {
        let mut disagreements = vec![0; displays.len()];
        for i in 0..displays.len() {
            for j in i + 1..displays.len() {
                if kept[i] && kept[j] && !agrees(i, j) {
                    disagreements[i] += 1;
                    disagreements[j] += 1;
                }
            }
        }
        // Drop the screen that disagrees with the most others, then look again.
        let worst = (0..displays.len()).filter(|&i| disagreements[i] > 0).max_by_key(|&i| disagreements[i]);
        match worst {
            Some(i) => kept[i] = false,
            None => break,
        }
    }
    displays.iter().zip(kept).map(|(d, k)| k.then(|| upgrade(&p.monitors[&d.id], d))).collect()
}

/// Which side of `a` the screen `b` touches in Windows, if they share an edge.
fn touching_side(a: &RectI, b: &RectI) -> Option<Side> {
    let y_overlap = a.bottom.min(b.bottom) - a.top.max(b.top);
    let x_overlap = a.right.min(b.right) - a.left.max(b.left);
    if y_overlap > 0 && b.left == a.right {
        Some(Side::Right)
    } else if y_overlap > 0 && b.right == a.left {
        Some(Side::Left)
    } else if x_overlap > 0 && b.top == a.bottom {
        Some(Side::Bottom)
    } else if x_overlap > 0 && b.bottom == a.top {
        Some(Side::Top)
    } else {
        None
    }
}

/// Which side of `a` the screen `b` sits on on the desk, if clearly one.
fn desk_side(a: &RectF, b: &RectF) -> Option<Side> {
    const TOLERANCE_MM: f64 = 1.0;
    let y_overlap = a.bottom().min(b.bottom()) - a.y.max(b.y);
    let x_overlap = a.right().min(b.right()) - a.x.max(b.x);
    if y_overlap > 0.0 && b.x >= a.right() - TOLERANCE_MM {
        Some(Side::Right)
    } else if y_overlap > 0.0 && b.right() <= a.x + TOLERANCE_MM {
        Some(Side::Left)
    } else if x_overlap > 0.0 && b.y >= a.bottom() - TOLERANCE_MM {
        Some(Side::Bottom)
    } else if x_overlap > 0.0 && b.bottom() <= a.y + TOLERANCE_MM {
        Some(Side::Top)
    } else {
        None
    }
}

fn px_array(d: &Display) -> [i32; 4] {
    [d.px.left, d.px.top, d.px.right, d.px.bottom]
}

fn from_array(a: &[i32; 4]) -> RectI {
    RectI::new(a[0], a[1], a[2], a[3])
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
    let mm: Vec<_> = displays.iter().map(|d| d.size_mm).collect();
    arrange_sized(displays, fixed, &mm)
}

fn arrange_sized(displays: &[Display], fixed: &[Option<RectF>], size_mm: &[(f64, f64)]) -> Vec<RectF> {
    let px: Vec<_> = displays.iter().map(|d| d.px).collect();
    let primary = displays.iter().position(|d| d.primary).unwrap_or(0);
    auto_arrange_around(&px, size_mm, primary, fixed)
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
    let windows = displays.iter().map(|d| (d.id.clone(), px_array(d))).collect();
    config.profiles.insert(key, Profile { monitors, guessed: Some(guessed), windows });
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

    fn placed_pair() -> [RectF; 2] {
        [RectF::new(0.0, 95.5, 597.0, 336.0), RectF::new(597.0, 0.0, 296.0, 527.0)]
    }

    #[test]
    fn moving_a_screen_to_the_other_side_in_windows_moves_it_on_the_desk() {
        let mut config = Config::default();
        save_rects(&mut config, &pair(), &placed_pair());
        let mut moved = pair();
        moved[1].px = RectI::new(-1080, 0, 0, 1920);

        let (r, changed) = desk_rects(&mut config, &moved);
        assert!(changed);
        assert!((r[1].right() - r[0].x).abs() < 1e-9, "B should now be left of A: {r:?}");
        // And it stays that way.
        assert_eq!(desk_rects(&mut config, &moved), (r, false));
    }

    #[test]
    fn nudging_a_screen_up_or_down_in_windows_keeps_the_desk() {
        let mut config = Config::default();
        save_rects(&mut config, &pair(), &placed_pair());
        let mut nudged = pair();
        nudged[1].px = RectI::new(2560, -300, 3640, 1620);

        let (r, changed) = desk_rects(&mut config, &nudged);
        assert_eq!(r, placed_pair().to_vec());
        assert!(changed, "the new pixel positions are remembered");
        assert_eq!(desk_rects(&mut config, &nudged), (r, false));
    }

    #[test]
    fn only_the_rearranged_screen_is_placed_again() {
        let mut config = Config::default();
        let mine = [
            RectF::new(527.0, 95.5, 597.0, 336.0),
            RectF::new(1124.0, 0.0, 296.0, 527.0),
            RectF::new(0.0, 200.0, 520.0, 300.0),
        ];
        save_rects(&mut config, &with_third(), &mine);
        // C goes from the left of A to the right of B.
        let mut moved = with_third();
        moved[2].px = RectI::new(3640, 0, 5560, 1080);

        let (r, _) = desk_rects(&mut config, &moved);
        assert_eq!(offset(&r), offset(&mine), "{r:?}");
        assert!((r[2].x - r[1].right()).abs() < 1e-9, "{r:?}");
        assert_eq!((r[2].w, r[2].h), (520.0, 300.0), "the size the user typed stays");
    }

    #[test]
    fn old_layout_that_contradicts_windows_is_placed_again() {
        // Saved by 0.2.2 or earlier, so no Windows positions; the desk has B
        // right of A but Windows now has it on the left.
        let mut config = Config::default();
        save_rects(&mut config, &pair(), &placed_pair());
        for p in config.profiles.values_mut() {
            p.windows.clear();
            p.guessed = None;
        }
        let mut moved = pair();
        moved[1].px = RectI::new(-1080, 0, 0, 1920);
        let (r, _) = desk_rects(&mut config, &moved);
        assert!((r[1].right() - r[0].x).abs() < 1e-9, "{r:?}");
    }
}
