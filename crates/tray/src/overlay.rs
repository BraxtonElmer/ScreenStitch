//! "Check alignment": one thin line across every real screen at the same
//! height on the desk. If it looks broken where two screens meet, the layout
//! is off there. The windows are click-through and never take focus.

use std::cell::RefCell;
use std::ptr::{null, null_mut};

use screenstitch_core::RectF;
use screenstitch_platform::alignment_line_mm;
use screenstitch_platform::display::Display;
use windows_sys::Win32::Foundation::{COLORREF, HWND};
use windows_sys::Win32::Graphics::Gdi::CreateSolidBrush;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, LWA_ALPHA, RegisterClassW, SW_SHOWNOACTIVATE,
    SetLayeredWindowAttributes, ShowWindow, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use screenstitch_platform::wide::to_wide;

const THICKNESS: i32 = 3;
/// Pink-red that stands out on any wallpaper (BGR for COLORREF).
const COLOR: COLORREF = 0x0058_45F0;

thread_local! {
    static LINES: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
}

pub fn is_visible() -> bool {
    LINES.with(|l| !l.borrow().is_empty())
}

pub fn show(displays: &[Display], rects: &[RectF]) {
    hide();
    let class = to_wide("ScreenStitchLine");
    let hinst = unsafe { GetModuleHandleW(null()) };
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(DefWindowProcW),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: null_mut(),
        hCursor: null_mut(),
        hbrBackground: unsafe { CreateSolidBrush(COLOR) },
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    unsafe { RegisterClassW(&wc) }; // fails harmlessly if already registered

    let y_mm = alignment_line_mm(rects);
    let mut lines = Vec::new();
    for (d, r) in displays.iter().zip(rects) {
        if y_mm < r.y || y_mm > r.bottom() {
            continue; // this screen doesn't reach that height
        }
        let y = d.px.top + ((y_mm - r.y) / r.h * f64::from(d.px.height())).round() as i32 - THICKNESS / 2;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                null(),
                WS_POPUP,
                d.px.left,
                y,
                d.px.width(),
                THICKNESS,
                null_mut(),
                null_mut(),
                hinst,
                null(),
            )
        };
        if hwnd.is_null() {
            continue;
        }
        unsafe {
            SetLayeredWindowAttributes(hwnd, 0, 235, LWA_ALPHA);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        lines.push(hwnd);
    }
    LINES.with(|l| *l.borrow_mut() = lines);
}

pub fn hide() {
    LINES.with(|l| {
        for hwnd in l.borrow_mut().drain(..) {
            unsafe { DestroyWindow(hwnd) };
        }
    });
}
