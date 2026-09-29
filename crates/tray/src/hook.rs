//! The low-level mouse hook: the only code on the hot path.
//!
//! Windows calls it on our message-loop thread for every mouse move, and
//! quietly removes hooks that are slow, so it never blocks or allocates.
//! Interior moves are a bounds check and nothing else.

use std::cell::RefCell;
use std::ptr::null_mut;

use screenstitch_core::{Action, Engine, Point, RectI};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CURSOR_SHOWING, CURSORINFO, CallNextHookEx, ClipCursor, GetClipCursor, GetCursorInfo, GetCursorPos,
    GetSystemMetrics, HC_ACTION, HHOOK, MSLLHOOKSTRUCT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SetCursorPos, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_MOUSEMOVE,
};

/// Mouse messages Windows synthesizes from pen and touch carry this signature.
const PEN_TOUCH_SIGNATURE: usize = 0xFF51_5700;
const PEN_TOUCH_MASK: usize = 0xFFFF_FF00;

struct State {
    engine: Engine,
    pause_in_fullscreen: bool,
    /// (checked at tick, fullscreen app in front) — asking the shell is cached briefly.
    fullscreen: (u64, bool),
    /// The clip rectangle we set last, so we never undo one a game set.
    managed_clip: Option<RectI>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static HOOK: RefCell<HHOOK> = const { RefCell::new(null_mut()) };
}

/// Start (or restart with a new layout) handling moves.
pub fn start(engine: Engine, pause_in_fullscreen: bool) {
    release_clip();
    let mut engine = engine;
    engine.resync(cursor_pos());
    STATE.with(|s| *s.borrow_mut() = Some(State { engine, pause_in_fullscreen, fullscreen: (0, false), managed_clip: None }));
    HOOK.with(|h| {
        let mut h = h.borrow_mut();
        if h.is_null() {
            *h = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), GetModuleHandleW(std::ptr::null()), 0) };
        }
    });
}

/// Stop handling moves and give the cursor back to Windows entirely.
pub fn stop() {
    HOOK.with(|h| {
        let mut h = h.borrow_mut();
        if !h.is_null() {
            unsafe { UnhookWindowsHookEx(*h) };
            *h = null_mut();
        }
    });
    release_clip();
    STATE.with(|s| *s.borrow_mut() = None);
}

/// Drop our cursor confinement (only if it is still ours).
pub fn release_clip() {
    STATE.with(|s| {
        if let Ok(mut s) = s.try_borrow_mut()
            && let Some(st) = s.as_mut()
        {
            if st.managed_clip.is_some_and(|m| current_clip() == Some(m)) {
                unsafe { ClipCursor(std::ptr::null()) };
            }
            st.managed_clip = None;
        }
        // No state (or re-entered): only clear a clip that looks like one of ours
        // would be guesswork, so leave foreign clips alone.
    });
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wparam == WM_MOUSEMOVE as WPARAM && lparam != 0 {
        let ms = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let from_pen_or_touch = ms.dwExtraInfo & PEN_TOUCH_MASK == PEN_TOUCH_SIGNATURE;
        let handled = STATE.with(|s| {
            let Ok(mut s) = s.try_borrow_mut() else { return false };
            let Some(st) = s.as_mut() else { return false };
            st.on_move(Point::new(ms.pt.x, ms.pt.y), from_pen_or_touch)
        });
        if handled {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

impl State {
    fn on_move(&mut self, p: Point, from_pen_or_touch: bool) -> bool {
        // Fast path: moving around inside the current screen.
        if let Some(r) = self.engine.current_rect()
            && inside_away_from_edges(&r, p)
        {
            return false;
        }

        if from_pen_or_touch || self.bypassed() {
            // Pens and touch point at absolute spots; games own the cursor. Step aside.
            if from_pen_or_touch || ctrl_down() {
                self.drop_clip();
            }
            self.engine.resync(p);
            return false;
        }

        match self.engine.on_move(p) {
            Action::Pass => {
                // On an edge pixel: make sure Windows can't slide the cursor
                // across its own (pixel-based) arrangement before we see it.
                if let Some(r) = self.engine.current_rect() {
                    self.ensure_clip(r);
                }
                false
            }
            Action::Move { to, clip } => {
                self.set_clip(clip);
                unsafe { SetCursorPos(to.x, to.y) };
                true
            }
        }
    }

    /// Someone else is in charge right now (a game, or the user holding Ctrl).
    fn bypassed(&mut self) -> bool {
        if ctrl_down() || !cursor_visible() || self.fullscreen_app_in_front() {
            return true;
        }
        match current_clip() {
            None => false,
            Some(c) => Some(c) != self.managed_clip && c != virtual_screen(),
        }
    }

    fn fullscreen_app_in_front(&mut self) -> bool {
        if !self.pause_in_fullscreen {
            return false;
        }
        let now = unsafe { GetTickCount64() };
        if now.saturating_sub(self.fullscreen.0) > 500 {
            self.fullscreen = (now, crate::fullscreen::app_in_front());
        }
        self.fullscreen.1
    }

    fn ensure_clip(&mut self, r: RectI) {
        if self.managed_clip != Some(r) || current_clip() != Some(r) {
            self.set_clip(r);
        }
    }

    fn set_clip(&mut self, r: RectI) {
        let rect = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        unsafe { ClipCursor(&rect) };
        self.managed_clip = Some(r);
    }

    fn drop_clip(&mut self) {
        if self.managed_clip.is_some_and(|m| current_clip() == Some(m)) {
            unsafe { ClipCursor(std::ptr::null()) };
        }
        self.managed_clip = None;
    }
}

fn inside_away_from_edges(r: &RectI, p: Point) -> bool {
    p.x > r.left && p.x < r.right - 1 && p.y > r.top && p.y < r.bottom - 1
}

fn ctrl_down() -> bool {
    (unsafe { GetAsyncKeyState(i32::from(VK_CONTROL)) } as u16) & 0x8000 != 0
}

fn cursor_visible() -> bool {
    let mut ci: CURSORINFO = unsafe { std::mem::zeroed() };
    ci.cbSize = size_of::<CURSORINFO>() as u32;
    unsafe { GetCursorInfo(&mut ci) == 0 || ci.flags & CURSOR_SHOWING != 0 }
}

fn current_clip() -> Option<RectI> {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    (unsafe { GetClipCursor(&mut r) } != 0).then(|| RectI::new(r.left, r.top, r.right, r.bottom))
}

fn virtual_screen() -> RectI {
    unsafe {
        let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        RectI::new(x, y, x + GetSystemMetrics(SM_CXVIRTUALSCREEN), y + GetSystemMetrics(SM_CYVIRTUALSCREEN))
    }
}

pub fn cursor_pos() -> Point {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    Point::new(p.x, p.y)
}
