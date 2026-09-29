//! Is a game (or anything else fullscreen) in front right now?
//!
//! Exclusive fullscreen and presentations are reported by the shell;
//! borderless-windowed games aren't, so a foreground window that covers its
//! whole monitor counts too.

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows_sys::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect};

use screenstitch_platform::wide::from_wide;

pub fn app_in_front() -> bool {
    let mut state = 0;
    let shell_says = unsafe { SHQueryUserNotificationState(&mut state) } == 0
        && matches!(state, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE);
    shell_says || foreground_covers_monitor()
}

fn foreground_covers_monitor() -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return false;
    }
    // The desktop and taskbar cover the screen too, but they aren't apps.
    let mut class = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    let class = from_wide(&class[..n.max(0) as usize]);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }

    let mut w: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut w) } == 0 {
        return false;
    }
    let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if unsafe { GetMonitorInfoW(mon, &mut mi) } == 0 {
        return false;
    }
    let m = mi.rcMonitor;
    w.left <= m.left && w.top <= m.top && w.right >= m.right && w.bottom >= m.bottom
}
