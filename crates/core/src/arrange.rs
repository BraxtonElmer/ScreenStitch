use crate::geom::{RectF, RectI};

/// Snap distance for "these edges line up" in the Windows arrangement.
const ALIGN_PX: i32 = 2;

/// Guess where the screens sit on the desk from the Windows arrangement and
/// each panel's physical size (`(width_mm, height_mm)`, already rotated).
///
/// Screens keep Windows' order and touch each other. Along the shared edge:
/// Windows' default top alignment is taken as "not arranged yet" and becomes
/// bottoms aligned (what most desks look like); bottom or centre alignment is
/// kept; anything else keeps the offset the user dragged in, converted to mm.
pub fn auto_arrange(px: &[RectI], size_mm: &[(f64, f64)], primary: usize) -> Vec<RectF> {
    let n = px.len();
    assert_eq!(n, size_mm.len());
    if n == 0 {
        return Vec::new();
    }
    let mut placed: Vec<Option<RectF>> = vec![None; n];
    placed[primary] = Some(RectF::new(0.0, 0.0, size_mm[primary].0, size_mm[primary].1));

    while let Some((i, j)) = closest_pair(px, &placed) {
        let a = placed[i].expect("i is placed");
        let (w, h) = size_mm[j];
        let r = place_next_to(&px[i], &a, &px[j], w, h);
        placed[j] = Some(push_out_of_overlaps(r, px, &placed, j));
    }

    let mut out: Vec<RectF> = placed.into_iter().map(|r| r.expect("all placed")).collect();
    let min_x = out.iter().map(|r| r.x).fold(f64::INFINITY, f64::min);
    let min_y = out.iter().map(|r| r.y).fold(f64::INFINITY, f64::min);
    for r in &mut out {
        r.x -= min_x;
        r.y -= min_y;
    }
    out
}

/// Two screens that share one neighbour's edge (say, two stacked beside a
/// wide one) can land on top of each other. Slide the new one off whatever it
/// hits, in the direction Windows has it, so it ends up touching instead.
fn push_out_of_overlaps(mut r: RectF, px: &[RectI], placed: &[Option<RectF>], j: usize) -> RectF {
    const MIN_OVERLAP_MM: f64 = 0.5;
    for _ in 0..placed.len() {
        let hit = placed.iter().enumerate().find_map(|(k, o)| {
            let o = o.as_ref()?;
            let ow = r.right().min(o.right()) - r.x.max(o.x);
            let oh = r.bottom().min(o.bottom()) - r.y.max(o.y);
            (ow > MIN_OVERLAP_MM && oh > MIN_OVERLAP_MM).then_some((k, *o))
        });
        let Some((k, o)) = hit else { break };
        let (pb, pk) = (&px[j], &px[k]);
        if pb.top >= pk.bottom - ALIGN_PX {
            r.y = o.bottom();
        } else if pb.bottom <= pk.top + ALIGN_PX {
            r.y = o.y - r.h;
        } else if pb.left >= pk.right - ALIGN_PX {
            r.x = o.right();
        } else if pb.right <= pk.left + ALIGN_PX {
            r.x = o.x - r.w;
        } else {
            break; // overlapping in Windows too: duplicated displays
        }
    }
    r
}

/// The unplaced screen closest (in Windows pixels) to any placed one; among
/// touching pairs, the one sharing the longest edge (corners don't count).
fn closest_pair(px: &[RectI], placed: &[Option<RectF>]) -> Option<(usize, usize)> {
    let mut best: Option<((i64, i64), usize, usize)> = None;
    for (i, _) in placed.iter().enumerate().filter(|(_, p)| p.is_some()) {
        for (j, _) in placed.iter().enumerate().filter(|(_, p)| p.is_none()) {
            let key = rect_gap(&px[i], &px[j]);
            if best.is_none_or(|(bk, ..)| key < bk) {
                best = Some((key, i, j));
            }
        }
    }
    best.map(|(_, i, j)| (i, j))
}

/// (distance, minus the length of the shared edge) — smaller is closer.
fn rect_gap(a: &RectI, b: &RectI) -> (i64, i64) {
    let dx = (a.left - b.right).max(b.left - a.right);
    let dy = (a.top - b.bottom).max(b.top - a.bottom);
    let shared = (-dx).max(-dy).max(0);
    (i64::from(dx.max(0)) + i64::from(dy.max(0)), -i64::from(shared))
}

fn place_next_to(pa: &RectI, a: &RectF, pb: &RectI, w: f64, h: f64) -> RectF {
    let mmpp_x = a.w / f64::from(pa.width());
    let mmpp_y = a.h / f64::from(pa.height());
    let x_overlap = pa.right.min(pb.right) - pa.left.max(pb.left);
    let y_overlap = pa.bottom.min(pb.bottom) - pa.top.max(pb.top);

    let beside = pb.left >= pa.right - ALIGN_PX || pb.right <= pa.left + ALIGN_PX;
    let stacked = pb.top >= pa.bottom - ALIGN_PX || pb.bottom <= pa.top + ALIGN_PX;

    if beside && (y_overlap > 0 || !stacked) {
        let x = if pb.left >= pa.right - ALIGN_PX { a.right() } else { a.x - w };
        let y = align(pa.top, pa.bottom, pb.top, pb.bottom, a.y, a.h, h, mmpp_y, h / f64::from(pb.height()));
        RectF::new(x, y, w, h)
    } else if stacked && x_overlap > 0 {
        let y = if pb.top >= pa.bottom - ALIGN_PX { a.bottom() } else { a.y - h };
        let x =
            align_centre_default(pa.left, pa.right, pb.left, pb.right, a.x, a.w, w, mmpp_x, w / f64::from(pb.width()));
        RectF::new(x, y, w, h)
    } else if stacked {
        // Diagonal: stack it, keeping Windows' horizontal offset.
        let y = if pb.top >= pa.bottom - ALIGN_PX { a.bottom() } else { a.y - h };
        let x = a.x + f64::from(pb.left - pa.left) * mmpp_x;
        RectF::new(x, y, w, h)
    } else {
        // Overlapping (duplicated / mirrored displays): same spot.
        RectF::new(a.x, a.y, w, h)
    }
}

/// Position along a vertical shared edge (side by side screens).
#[allow(clippy::too_many_arguments)]
fn align(a0: i32, a1: i32, b0: i32, b1: i32, am0: f64, am_len: f64, len: f64, mmpp: f64, mmpp_b: f64) -> f64 {
    let centre_a = a0 + a1;
    let centre_b = b0 + b1;
    if (a0 - b0).abs() <= ALIGN_PX || (a1 - b1).abs() <= ALIGN_PX {
        am0 + am_len - len // tops (Windows default) or bottoms: bottoms on the desk
    } else if (centre_a - centre_b).abs() <= 2 * ALIGN_PX {
        am0 + (am_len - len) * 0.5
    } else {
        keep_offset(a0, a1, b0, b1, am0, mmpp, mmpp_b)
    }
}

/// Position along a horizontal shared edge (stacked screens): centred unless
/// the user clearly offset them in Windows.
#[allow(clippy::too_many_arguments)]
fn align_centre_default(
    a0: i32,
    a1: i32,
    b0: i32,
    b1: i32,
    am0: f64,
    am_len: f64,
    len: f64,
    mmpp: f64,
    mmpp_b: f64,
) -> f64 {
    if (a0 - b0).abs() <= ALIGN_PX && (a1 - b1).abs() > ALIGN_PX {
        am0 // left edges lined up on purpose
    } else if (a1 - b1).abs() <= ALIGN_PX && (a0 - b0).abs() > ALIGN_PX {
        am0 + am_len - len
    } else if ((a0 + a1) - (b0 + b1)).abs() <= 2 * ALIGN_PX || (a0 - b0).abs() <= ALIGN_PX {
        am0 + (am_len - len) * 0.5
    } else {
        keep_offset(a0, a1, b0, b1, am0, mmpp, mmpp_b)
    }
}

/// Keep the offset the user dragged in: the middle of the pixel range both
/// screens share lands at the same desk position on both. Unlike scaling the
/// raw offset, this can never push the screens apart.
fn keep_offset(a0: i32, a1: i32, b0: i32, b1: i32, am0: f64, mmpp_a: f64, mmpp_b: f64) -> f64 {
    let shared_mid = f64::from(a0.max(b0) + a1.min(b1)) * 0.5;
    let mm = am0 + (shared_mid - f64::from(a0)) * mmpp_a;
    mm - (shared_mid - f64::from(b0)) * mmpp_b
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layout, Monitor};

    #[test]
    fn default_windows_row_becomes_bottom_aligned_row() {
        let px = [RectI::new(0, 0, 1920, 1080), RectI::new(1920, 0, 5760, 2160), RectI::new(5760, 0, 6840, 1920)];
        let mm = [(527.0, 296.0), (597.0, 336.0), (296.0, 527.0)];
        let r = auto_arrange(&px, &mm, 0);
        let bottom = r[0].bottom();
        for x in &r {
            assert!((x.bottom() - bottom).abs() < 1e-9, "{r:?}");
        }
        assert!((r[1].x - r[0].right()).abs() < 1e-9);
        assert!((r[2].x - r[1].right()).abs() < 1e-9);
    }

    #[test]
    fn stacked_screens_are_centred() {
        let px = [RectI::new(0, 0, 2560, 1440), RectI::new(320, -1080, 2240, 0)];
        let r = auto_arrange(&px, &[(597.0, 336.0), (477.0, 268.0)], 0);
        assert!((r[1].bottom() - r[0].y).abs() < 1e-9);
        let centre = |x: &RectF| x.x + x.w / 2.0;
        assert!((centre(&r[0]) - centre(&r[1])).abs() < 1e-9, "{r:?}");
    }

    /// Tiny deterministic PRNG so the property test needs no dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn range(&mut self, lo: i32, hi: i32) -> i32 {
            lo + (self.next() % (hi - lo) as u64) as i32
        }
    }

    /// Any touching Windows arrangement must produce a layout where every
    /// screen can be reached and every edge crossing lands on a real screen.
    #[test]
    fn random_arrangements_never_trap_the_cursor() {
        let sizes = [
            (1920, 1080, 527.0, 296.0),
            (3840, 2160, 597.0, 336.0),
            (2560, 1440, 597.0, 336.0),
            (1080, 1920, 296.0, 527.0),
            (3440, 1440, 800.0, 335.0),
            (1366, 768, 344.0, 194.0),
        ];
        let mut rng = Rng(0x5eed_1234_abcd_ef01);
        for _ in 0..2000 {
            let n = rng.range(2, 6) as usize;
            let mut px: Vec<RectI> = Vec::new();
            let mut mm = Vec::new();
            for k in 0..n {
                let (w, h, mw, mh) = sizes[rng.range(0, sizes.len() as i32) as usize];
                let r = if k == 0 {
                    RectI::new(0, 0, w, h)
                } else {
                    // Attach to a random existing screen on a random side, Windows-style (touching).
                    let base = px[rng.range(0, px.len() as i32) as usize];
                    let cand = match rng.range(0, 4) {
                        0 => {
                            let y = rng.range(base.top - h + 1, base.bottom);
                            RectI::new(base.right, y, base.right + w, y + h)
                        }
                        1 => {
                            let y = rng.range(base.top - h + 1, base.bottom);
                            RectI::new(base.left - w, y, base.left, y + h)
                        }
                        2 => {
                            let x = rng.range(base.left - w + 1, base.right);
                            RectI::new(x, base.bottom, x + w, base.bottom + h)
                        }
                        _ => {
                            let x = rng.range(base.left - w + 1, base.right);
                            RectI::new(x, base.top - h, x + w, base.top)
                        }
                    };
                    if px.iter().any(|o| overlaps(o, &cand)) {
                        continue;
                    }
                    cand
                };
                px.push(r);
                mm.push((mw, mh));
            }
            let rects = auto_arrange(&px, &mm, 0);
            let layout = Layout::new(px.iter().zip(&rects).map(|(p, m)| Monitor::new(*p, *m)).collect());
            assert!(layout.unreachable().is_empty(), "unreachable in {px:?} -> {rects:?}");
            for (i, m) in layout.monitors().iter().enumerate() {
                for side in crate::Side::ALL {
                    if layout.links(i, side).is_empty() {
                        continue;
                    }
                    for c in [m.px.left, m.px.top, (m.px.left + m.px.right) / 2, m.px.right - 1, m.px.bottom - 1] {
                        let p = match side {
                            crate::Side::Right => crate::Point::new(m.px.right, c),
                            crate::Side::Left => crate::Point::new(m.px.left - 1, c),
                            crate::Side::Bottom => crate::Point::new(c, m.px.bottom),
                            crate::Side::Top => crate::Point::new(c, m.px.top - 1),
                        };
                        let (t, to) = layout.cross(i, side, p).expect("linked side crosses");
                        assert!(layout.monitors()[t].px.contains(to), "landed off-screen: {to:?}");
                    }
                }
            }
        }
    }

    fn overlaps(a: &RectI, b: &RectI) -> bool {
        a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
    }
}
