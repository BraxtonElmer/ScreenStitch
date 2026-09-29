// Debug builds keep a console for logs; release builds are a pure tray app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod display;
mod edid;
mod hook;
mod icon;
mod wide;

use std::ptr::null;

use screenstitch_core::{Layout, Monitor, auto_arrange};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

fn main() {
    // Without this every coordinate Windows hands us is scaled and wrong.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
        list();
        return;
    }

    let name = wide::to_wide(r"Local\ScreenStitch.Agent");
    let _mutex = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return; // already running
    }
    app::run();
}

/// `screenstitch --list`: what was detected and the layout it would use.
fn list() {
    let displays = display::detect();
    let px: Vec<_> = displays.iter().map(|d| d.px).collect();
    let mm: Vec<_> = displays.iter().map(|d| d.size_mm).collect();
    let primary = displays.iter().position(|d| d.primary).unwrap_or(0);
    let rects = auto_arrange(&px, &mm, primary);

    for (i, (d, r)) in displays.iter().zip(&rects).enumerate() {
        let diag = (d.size_mm.0.powi(2) + d.size_mm.1.powi(2)).sqrt() / 25.4;
        println!(
            "{}. {}{}  [{}]\n   pixels {}x{} at ({}, {})\n   size   {:.0} x {:.0} mm ({:.1}\"){}\n   desk   x {:.0} mm, y {:.0} mm",
            i + 1,
            d.name,
            if d.primary { " (main)" } else { "" },
            d.id,
            d.px.width(),
            d.px.height(),
            d.px.left,
            d.px.top,
            d.size_mm.0,
            d.size_mm.1,
            diag,
            if d.size_from_edid { "" } else { "  (guessed, no EDID)" },
            r.x,
            r.y,
        );
    }
    let layout = Layout::new(displays.iter().zip(rects).map(|(d, r)| Monitor::new(d.px, r)).collect());
    let lost = layout.unreachable();
    if !lost.is_empty() {
        println!("warning: screens {lost:?} can't be reached from screen 1");
    }
}
