//! The tray icon, drawn in code: two screens of different sizes joined by a
//! stitch. Matches the taskbar's light/dark theme; faded while turned off.

use std::ptr::null_mut;

use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIcon, HICON};

use screenstitch_platform::wide::to_wide;

const SIZE: usize = 32;

pub fn tray_icon(on: bool) -> HICON {
    let (r, g, b) = if taskbar_is_light() { (0x1B, 0x1B, 0x1B) } else { (0xFF, 0xFF, 0xFF) };
    let alpha: u8 = if on { 0xFF } else { 0x70 };
    let mut px = [[0u8; 4]; SIZE * SIZE];
    let mut set = |x: usize, y: usize| px[y * SIZE + x] = [b, g, r, alpha];

    // Left screen (smaller) and right screen (bigger), 2 px outlines.
    for (x0, y0, x1, y1) in [(1, 9, 13, 24), (16, 5, 31, 27)] {
        for x in x0..=x1 {
            for y in y0..=y1 {
                let edge = x <= x0 + 1 || x >= x1 - 1 || y <= y0 + 1 || y >= y1 - 1;
                if edge {
                    set(x, y);
                }
            }
        }
    }
    // The stitch across both.
    for x in 4..=28 {
        if (x - 4) % 5 < 3 {
            set(x, 15);
            set(x, 16);
        }
    }

    let xor: Vec<u8> = px.iter().flatten().copied().collect();
    let and = [0u8; SIZE * SIZE / 8];
    unsafe { CreateIcon(null_mut(), SIZE as i32, SIZE as i32, 1, 32, and.as_ptr(), xor.as_ptr()) }
}

fn taskbar_is_light() -> bool {
    let key = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let name = to_wide("SystemUsesLightTheme");
    let mut v: u32 = 0;
    let mut len = size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&raw mut v).cast(),
            &mut len,
        )
    };
    rc == 0 && v != 0
}
