use crate::geom::{Point, RectI, Side};
use crate::layout::Layout;

/// What the OS layer should do with one proposed cursor position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Let Windows handle it.
    Pass,
    /// Swallow the move, confine the cursor to `clip` and put it at `to`.
    Move { to: Point, clip: RectI },
}

/// Tracks which screen the cursor is on and decides every crossing.
#[derive(Clone, Debug, Default)]
pub struct Engine {
    layout: Layout,
    current: Option<usize>,
}

impl Engine {
    pub fn new(layout: Layout) -> Self {
        Self { layout, current: None }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Rectangle of the screen the cursor is on, if known.
    pub fn current_rect(&self) -> Option<RectI> {
        self.current.map(|c| self.layout.monitors()[c].px)
    }

    /// Forget the tracked screen and look it up again from `p` (after a
    /// pause, a display change or anything else that moved the cursor).
    pub fn resync(&mut self, p: Point) {
        self.current = self.layout.monitor_at(p);
    }

    pub fn on_move(&mut self, p: Point) -> Action {
        let Some(cur) = self.current else {
            self.resync(p);
            return Action::Pass;
        };
        let r = self.layout.monitors()[cur].px;
        if r.contains(p) {
            return Action::Pass;
        }

        // Try the side the cursor went furthest past first (matters in corners).
        let mut sides = [
            (Side::Left, r.left - p.x),
            (Side::Right, p.x - (r.right - 1)),
            (Side::Top, r.top - p.y),
            (Side::Bottom, p.y - (r.bottom - 1)),
        ];
        sides.sort_by_key(|s| std::cmp::Reverse(s.1));
        for (side, dist) in sides {
            if dist <= 0 {
                break;
            }
            if let Some((target, to)) = self.layout.cross(cur, side, p) {
                self.current = Some(target);
                return Action::Move { to, clip: self.layout.monitors()[target].px };
            }
        }
        Action::Move { to: r.clamp(p), clip: r }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Monitor, RectF};

    fn engine() -> Engine {
        Engine::new(Layout::new(vec![
            Monitor::new(RectI::new(0, 0, 1920, 1080), RectF::new(0.0, 40.0, 527.0, 296.0)),
            Monitor::new(RectI::new(1920, 0, 5760, 2160), RectF::new(527.0, 0.0, 597.0, 336.0)),
        ]))
    }

    #[test]
    fn first_event_only_starts_tracking() {
        let mut e = engine();
        assert_eq!(e.on_move(Point::new(100, 100)), Action::Pass);
        assert_eq!(e.current_rect(), Some(RectI::new(0, 0, 1920, 1080)));
    }

    #[test]
    fn interior_moves_pass_through() {
        let mut e = engine();
        e.on_move(Point::new(100, 100));
        assert_eq!(e.on_move(Point::new(1919, 1079)), Action::Pass);
    }

    #[test]
    fn crossing_moves_and_confines_to_the_new_screen() {
        let mut e = engine();
        e.on_move(Point::new(1900, 540));
        let Action::Move { to, clip } = e.on_move(Point::new(1920, 540)) else { panic!() };
        assert_eq!(clip, RectI::new(1920, 0, 5760, 2160));
        assert!(clip.contains(to));
        assert_eq!(e.on_move(Point::new(to.x + 5, to.y)), Action::Pass);
    }

    #[test]
    fn walls_keep_the_cursor_on_screen_and_let_it_slide() {
        let mut e = engine();
        e.on_move(Point::new(10, 500));
        assert_eq!(
            e.on_move(Point::new(-3, 520)),
            Action::Move { to: Point::new(0, 520), clip: RectI::new(0, 0, 1920, 1080) }
        );
    }

    #[test]
    fn corner_prefers_the_side_with_a_neighbour() {
        let mut e = engine();
        e.on_move(Point::new(1919, 1079));
        // Past both the right and the bottom edge; only the right has a screen.
        let Action::Move { clip, .. } = e.on_move(Point::new(1921, 1085)) else { panic!() };
        assert_eq!(clip, RectI::new(1920, 0, 5760, 2160));
    }
}
