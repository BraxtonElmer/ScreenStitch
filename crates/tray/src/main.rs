// Debug builds keep a console for logs; release builds are a pure tray app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fullscreen;
mod hook;
mod icon;
mod overlay;

use std::ptr::null;

use screenstitch_platform::display;
use screenstitch_platform::tray::{self, MSG_OPEN_SETTINGS};
use screenstitch_platform::wide::to_wide;
use screenstitch_platform::{alignment_line_mm, auto_rects, layout};
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

/// `ScreenStitch.exe`            start in the tray and open the settings window
/// `ScreenStitch.exe --background` start quietly (used by "Start with Windows")
/// `ScreenStitch.exe --list`       print the detected screens and guessed layout
fn main() {
    // Without this every coordinate Windows hands us is scaled and wrong.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
        list();
        return;
    }
    let background = args.iter().any(|a| a == "--background");

    let name = to_wide(r"Local\ScreenStitch.Tray");
    let _mutex = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // Already running: opening the app again means "show me the window".
        if !background {
            tray::post(MSG_OPEN_SETTINGS, 0);
        }
        return;
    }
    app::run(!background);
}

fn list() {
    let displays = display::detect();
    let rects = auto_rects(&displays);
    for (d, r) in displays.iter().zip(&rects) {
        let diag = (d.size_mm.0.powi(2) + d.size_mm.1.powi(2)).sqrt() / 25.4;
        println!(
            "{}. {}{}  [{}]\n   pixels {}x{} at ({}, {}), scaling {}%\n   size   {:.0} x {:.0} mm ({:.1}\"){}\n   desk   x {:.0} mm, y {:.0} mm",
            d.number,
            d.name,
            if d.primary { " (main)" } else { "" },
            d.id,
            d.px.width(),
            d.px.height(),
            d.px.left,
            d.px.top,
            d.scale,
            d.size_mm.0,
            d.size_mm.1,
            diag,
            if d.size_from_edid { "" } else { "  (guessed, no EDID)" },
            r.x,
            r.y,
        );
    }
    println!("alignment line at {:.0} mm", alignment_line_mm(&rects));
    let lost = layout(&displays, &rects).unreachable();
    if !lost.is_empty() {
        println!("warning: screens {lost:?} can't be reached from the first one");
    }
}
