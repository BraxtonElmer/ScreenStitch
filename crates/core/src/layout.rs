use crate::geom::{Point, RectF, RectI, Side};

/// A neighbour further away than the closest one by more than this is ignored,
/// so the cursor always lands on the screen right next to it, never one beyond.
const LAYER_TOLERANCE_MM: f64 = 50.0;
/// Screens may overlap by this much on the desk and still count as neighbours
/// (imprecise dragging, bezels drawn inside the panel size).
const OVERLAP_TOLERANCE_MM: f64 = 15.0;
const EPS: f64 = 1e-6;

/// One screen: where Windows puts it (pixels) and where it really is (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    pub px: RectI,
    pub mm: RectF,
}

impl Monitor {
    pub fn new(px: RectI, mm: RectF) -> Self {
        Self { px, mm }
    }

    fn px_range(&self, side: Side) -> (i32, i32) {
        if side.is_vertical() { (self.px.top, self.px.bottom) } else { (self.px.left, self.px.right) }
    }

    fn mm_range(&self, side: Side) -> (f64, f64) {
        if side.is_vertical() { (self.mm.y, self.mm.bottom()) } else { (self.mm.x, self.mm.right()) }
    }

    /// Millimetres per pixel along the given edge.
    fn along_mmpp(&self, side: Side) -> f64 {
        let (p0, p1) = self.px_range(side);
        let (m0, m1) = self.mm_range(side);
        (m1 - m0) / f64::from(p1 - p0)
    }

    /// Millimetres per pixel across the given edge.
    fn across_mmpp(&self, side: Side) -> f64 {
        let other = if side.is_vertical() { Side::Top } else { Side::Left };
        self.along_mmpp(other)
    }

    /// Centre of pixel `c` along the edge, in desk millimetres.
    pub fn px_to_mm_along(&self, side: Side, c: i32) -> f64 {
        let (p0, _) = self.px_range(side);
        let (m0, _) = self.mm_range(side);
        m0 + (f64::from(c - p0) + 0.5) * self.along_mmpp(side)
    }

    /// Pixel index (fractional) along the edge for a desk position.
    pub fn mm_to_px_along(&self, side: Side, mm: f64) -> f64 {
        let (p0, _) = self.px_range(side);
        let (m0, _) = self.mm_range(side);
        f64::from(p0) + (mm - m0) / self.along_mmpp(side) - 0.5
    }

    fn edge_mm(&self, side: Side) -> f64 {
        match side {
            Side::Left => self.mm.x,
            Side::Right => self.mm.right(),
            Side::Top => self.mm.y,
            Side::Bottom => self.mm.bottom(),
        }
    }
}

const fn opposite(side: Side) -> Side {
    match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
        Side::Top => Side::Bottom,
        Side::Bottom => Side::Top,
    }
}

/// A stretch of one monitor's edge (in desk mm along that edge) that leads to
/// `target`. Landing positions are clamped to `[lo_mm, hi_mm]`, the part of
/// the target that really faces this edge, so a cursor leaving through a gap
/// lands on the nearest point that exists instead of hitting a wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub from_mm: f64,
    pub to_mm: f64,
    pub target: usize,
    pub lo_mm: f64,
    pub hi_mm: f64,
}

/// Monitors plus the precomputed edge links between them.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    monitors: Vec<Monitor>,
    links: Vec<[Vec<Link>; 4]>,
    stop_at_gaps: bool,
}

impl Layout {
    /// Parts of an edge with no screen beside them lead to the nearest point
    /// of the neighbouring screen, so the cursor never gets stuck.
    pub fn new(monitors: Vec<Monitor>) -> Self {
        Self::with_gaps(monitors, false)
    }

    /// With `stop_at_gaps`, parts of an edge with no screen physically beside
    /// them are walls instead, like the real gap on the desk.
    pub fn with_gaps(monitors: Vec<Monitor>, stop_at_gaps: bool) -> Self {
        let links =
            (0..monitors.len()).map(|i| Side::ALL.map(|side| build_side(&monitors, i, side, !stop_at_gaps))).collect();
        Self { monitors, links, stop_at_gaps }
    }

    pub fn monitors(&self) -> &[Monitor] {
        &self.monitors
    }

    pub fn links(&self, monitor: usize, side: Side) -> &[Link] {
        &self.links[monitor][side.index()]
    }

    pub fn monitor_at(&self, p: Point) -> Option<usize> {
        self.monitors.iter().position(|m| m.px.contains(p))
    }

    /// Where a cursor leaving `from` through `side` at `p` should appear.
    /// `None` when nothing is physically on that side (a wall).
    pub fn cross(&self, from: usize, side: Side, p: Point) -> Option<(usize, Point)> {
        let links = self.links(from, side);
        let a = &self.monitors[from];
        let (p0, p1) = a.px_range(side);
        let c = if side.is_vertical() { p.y } else { p.x }.clamp(p0, p1 - 1);
        let mm = a.px_to_mm_along(side, c);

        let inside = links.iter().find(|l| mm >= l.from_mm && mm < l.to_mm);
        let link = inside.or_else(|| {
            // Rounding at the very ends of a link; never across a real gap when stopping there.
            let slack = if self.stop_at_gaps { a.along_mmpp(side) } else { f64::INFINITY };
            links.iter().filter(|l| dist_to_range(mm, l.from_mm, l.to_mm) <= slack).min_by(|x, y| {
                let dx = dist_to_range(mm, x.from_mm, x.to_mm);
                let dy = dist_to_range(mm, y.from_mm, y.to_mm);
                dx.total_cmp(&dy)
            })
        })?;

        let b = &self.monitors[link.target];
        let half = b.along_mmpp(side) * 0.5;
        let mm_t = if link.hi_mm - link.lo_mm > 2.0 * half {
            mm.clamp(link.lo_mm + half, link.hi_mm - half)
        } else {
            (link.lo_mm + link.hi_mm) * 0.5
        };
        let (b0, b1) = b.px_range(side);
        let along = (b.mm_to_px_along(side, mm_t).round() as i32).clamp(b0, b1 - 1);

        // Keep the overshoot past the edge, scaled to the new screen, so fast
        // flicks don't feel like they hit a speed bump at the border.
        let over = match side {
            Side::Right => p.x - a.px.right,
            Side::Left => a.px.left - 1 - p.x,
            Side::Bottom => p.y - a.px.bottom,
            Side::Top => a.px.top - 1 - p.y,
        }
        .max(0);
        let over = (f64::from(over) * a.across_mmpp(side) / b.across_mmpp(side)).round() as i32;

        let to = match side {
            Side::Right => Point::new((b.px.left + over).min(b.px.right - 1), along),
            Side::Left => Point::new((b.px.right - 1 - over).max(b.px.left), along),
            Side::Bottom => Point::new(along, (b.px.top + over).min(b.px.bottom - 1)),
            Side::Top => Point::new(along, (b.px.bottom - 1 - over).max(b.px.top)),
        };
        Some((link.target, to))
    }

    /// Monitors that can't be reached from monitor 0 by crossing edges.
    /// A layout editor should warn about (or snap) these.
    pub fn unreachable(&self) -> Vec<usize> {
        let n = self.monitors.len();
        if n == 0 {
            return Vec::new();
        }
        let mut seen = vec![false; n];
        let mut stack = vec![0];
        seen[0] = true;
        while let Some(i) = stack.pop() {
            for side in Side::ALL {
                for l in self.links(i, side) {
                    if !seen[l.target] {
                        seen[l.target] = true;
                        stack.push(l.target);
                    }
                }
            }
        }
        (0..n).filter(|&i| !seen[i]).collect()
    }
}

fn dist_to_range(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo - v
    } else if v > hi {
        v - hi
    } else {
        0.0
    }
}

struct Candidate {
    target: usize,
    dist: f64,
    lo: f64,
    hi: f64,
}

fn build_side(ms: &[Monitor], i: usize, side: Side, fill_gaps: bool) -> Vec<Link> {
    let a = &ms[i];
    let edge = a.edge_mm(side);
    let (a0, a1) = a.mm_range(side);

    let mut cands: Vec<Candidate> = ms
        .iter()
        .enumerate()
        .filter(|&(j, _)| j != i)
        .filter_map(|(j, b)| {
            let b_edge = b.edge_mm(opposite(side));
            let dist = match side {
                Side::Right | Side::Bottom => b_edge - edge,
                Side::Left | Side::Top => edge - b_edge,
            };
            let (b0, b1) = b.mm_range(side);
            let (lo, hi) = (a0.max(b0), a1.min(b1));
            (dist >= -OVERLAP_TOLERANCE_MM && hi - lo > EPS).then_some(Candidate { target: j, dist, lo, hi })
        })
        .collect();

    let Some(nearest) = cands.iter().map(|c| c.dist).min_by(f64::total_cmp) else {
        return Vec::new();
    };
    cands.retain(|c| c.dist <= nearest + LAYER_TOLERANCE_MM);

    // Split the edge at every overlap boundary and give each piece to the
    // closest screen that faces it.
    let mut cuts: Vec<f64> = vec![a0, a1];
    cuts.extend(cands.iter().flat_map(|c| [c.lo, c.hi]));
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|x, y| (*x - *y).abs() < EPS);

    let mut pieces: Vec<(f64, f64, Option<usize>)> = Vec::new();
    for w in cuts.windows(2) {
        let (s, e) = (w[0], w[1]);
        if e - s < EPS {
            continue;
        }
        let mid = (s + e) * 0.5;
        let best = cands
            .iter()
            .enumerate()
            .filter(|(_, c)| c.lo <= mid && mid <= c.hi)
            .min_by(|(_, x), (_, y)| x.dist.total_cmp(&y.dist).then(x.target.cmp(&y.target)))
            .map(|(k, _)| k);
        pieces.push((s, e, best));
    }

    // Pieces facing nothing borrow the nearest covered piece's screen; the
    // clamp range then lands the cursor on that screen's closest edge point.
    let covered: Vec<(f64, f64, usize)> = pieces.iter().filter_map(|&(s, e, k)| k.map(|k| (s, e, k))).collect();
    let mut links: Vec<Link> = Vec::new();
    for &(s, e, k) in &pieces {
        if k.is_none() && !fill_gaps {
            continue; // a wall: nothing is physically beside this part of the edge
        }
        let k = k.unwrap_or_else(|| {
            covered
                .iter()
                .min_by(|x, y| {
                    let dx = gap(s, e, x.0, x.1);
                    let dy = gap(s, e, y.0, y.1);
                    dx.total_cmp(&dy)
                })
                .map(|c| c.2)
                .expect("at least one candidate covers part of the edge")
        });
        let c = &cands[k];
        match links.last_mut() {
            Some(last) if last.target == c.target && last.lo_mm == c.lo && (last.to_mm - s).abs() < EPS => {
                last.to_mm = e;
            }
            _ => links.push(Link { from_mm: s, to_mm: e, target: c.target, lo_mm: c.lo, hi_mm: c.hi }),
        }
    }
    links
}

fn gap(s0: f64, e0: f64, s1: f64, e1: f64) -> f64 {
    if e1 <= s0 {
        s0 - e1
    } else if s1 >= e0 {
        s1 - e0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 24" 1080p on the left, 27" 4K on the right, bottoms aligned on the desk,
    /// tops aligned in Windows (the default arrangement).
    pub(crate) fn two_screens() -> Layout {
        Layout::new(vec![
            Monitor::new(RectI::new(0, 0, 1920, 1080), RectF::new(0.0, 40.0, 527.0, 296.0)),
            Monitor::new(RectI::new(1920, 0, 5760, 2160), RectF::new(527.0, 0.0, 597.0, 336.0)),
        ])
    }

    #[test]
    fn crossing_keeps_the_physical_height() {
        let l = two_screens();
        for y in [0, 200, 540, 900, 1079] {
            let (t, to) = l.cross(0, Side::Right, Point::new(1920, y)).unwrap();
            assert_eq!(t, 1);
            let mm_from = l.monitors()[0].px_to_mm_along(Side::Right, y);
            let mm_to = l.monitors()[1].px_to_mm_along(Side::Left, to.y);
            assert!((mm_from - mm_to).abs() < 0.1, "y={y}: {mm_from} vs {mm_to}");
            assert_eq!(to.x, 1920);
        }
    }

    #[test]
    fn going_back_returns_to_the_same_row() {
        let l = two_screens();
        for y in 0..1080 {
            let (_, there) = l.cross(0, Side::Right, Point::new(1920, y)).unwrap();
            let (t, back) = l.cross(1, Side::Left, Point::new(1919, there.y)).unwrap();
            assert_eq!(t, 0);
            assert!((back.y - y).abs() <= 1, "y={y} came back as {}", back.y);
        }
    }

    #[test]
    fn leaving_through_a_gap_lands_on_the_nearest_point() {
        let l = two_screens();
        // Top 40 mm of the 4K screen have nothing to their left: land at the top of the 1080p.
        let (t, to) = l.cross(1, Side::Left, Point::new(1919, 10)).unwrap();
        assert_eq!(t, 0);
        assert_eq!(to.y, 0);
    }

    #[test]
    fn stopping_at_gaps_makes_them_walls() {
        let ms = two_screens().monitors().to_vec();
        let l = Layout::with_gaps(ms, true);
        // Nothing beside the top 40 mm of the 4K screen: the cursor stays put.
        assert!(l.cross(1, Side::Left, Point::new(1919, 10)).is_none());
        // Where the screens do face each other, crossing works as usual.
        let (t, _) = l.cross(1, Side::Left, Point::new(1919, 1500)).unwrap();
        assert_eq!(t, 0);
        // And the 1080p's whole edge still faces the 4K screen.
        assert!(l.cross(0, Side::Right, Point::new(1920, 0)).is_some());
    }

    #[test]
    fn outer_edges_are_walls() {
        let l = two_screens();
        assert!(l.cross(0, Side::Left, Point::new(-1, 500)).is_none());
        assert!(l.cross(1, Side::Top, Point::new(3000, -1)).is_none());
    }

    #[test]
    fn overshoot_is_carried_over() {
        let l = two_screens();
        let (_, to) = l.cross(0, Side::Right, Point::new(1920 + 10, 500)).unwrap();
        // 10 px at 0.274 mm/px ≈ 2.7 mm ≈ 18 px at 0.155 mm/px.
        assert!((to.x - (1920 + 18)).abs() <= 1, "{}", to.x);
    }

    #[test]
    fn nearest_layer_wins_over_screens_further_away() {
        // [A][B small][C] in a row: from A, pieces not facing B go to B's nearest point, not to C.
        let l = Layout::new(vec![
            Monitor::new(RectI::new(0, 0, 1000, 1000), RectF::new(0.0, 0.0, 300.0, 300.0)),
            Monitor::new(RectI::new(1000, 0, 2000, 500), RectF::new(300.0, 0.0, 300.0, 150.0)),
            Monitor::new(RectI::new(2000, 0, 3000, 1000), RectF::new(600.0, 0.0, 300.0, 300.0)),
        ]);
        let (t, _) = l.cross(0, Side::Right, Point::new(1000, 900)).unwrap();
        assert_eq!(t, 1);
    }
}
