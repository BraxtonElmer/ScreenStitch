//! How the settings window (or a second launch) talks to the running tray app:
//! a window message to its hidden window. Nothing to connect, nothing to leak.

use std::ptr::null;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, RegisterWindowMessageW, WM_APP};

use crate::wide::to_wide;

pub const WINDOW_CLASS: &str = "ScreenStitchTray";

/// Re-read config.json and apply it (layout, on/off).
pub const MSG_RELOAD: u32 = WM_APP + 2;
/// Open (or focus) the settings window.
pub const MSG_OPEN_SETTINGS: u32 = WM_APP + 3;
/// Draw (wparam 1) or hide (wparam 0) the alignment line on every screen.
pub const MSG_ALIGNMENT_LINE: u32 = WM_APP + 4;

pub fn window() -> Option<HWND> {
    let class = to_wide(WINDOW_CLASS);
    let hwnd = unsafe { FindWindowW(class.as_ptr(), null()) };
    (!hwnd.is_null()).then_some(hwnd)
}

pub fn is_running() -> bool {
    window().is_some()
}

/// Returns false when the tray app isn't running.
pub fn post(msg: u32, wparam: usize) -> bool {
    match window() {
        Some(hwnd) => unsafe { PostMessageW(hwnd, msg, wparam, 0) != 0 },
        None => false,
    }
}

/// Windows' "TaskbarCreated" broadcast, sent when Explorer restarts.
pub fn taskbar_created_message() -> u32 {
    unsafe { RegisterWindowMessageW(to_wide("TaskbarCreated").as_ptr()) }
}
