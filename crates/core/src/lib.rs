//! Platform-free core of ScreenStitch.
//!
//! Monitors live in two spaces: Windows' virtual-desktop **pixels** and the
//! user's desk in **millimetres**. The [`Layout`] precomputes, for every edge of
//! every monitor, which neighbour it leads to physically; the [`Engine`] turns a
//! proposed cursor position into "let it through" or "move it here instead".
//!
//! Nothing in this crate touches the OS, so the whole algorithm is unit-tested.

mod arrange;
mod engine;
mod geom;
mod layout;

pub use arrange::auto_arrange;
pub use engine::{Action, Engine};
pub use geom::{Point, RectF, RectI, Side};
pub use layout::{Layout, Link, Monitor};
