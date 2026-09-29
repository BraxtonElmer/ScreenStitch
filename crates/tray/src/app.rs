//! The tray app's hidden window: tray icon and menu, hotkey, messages from the
//! settings window, and reacting to display, theme and power changes.
//! Everything runs on one thread.

use std::cell::RefCell;
use std::ptr::{null, null_mut};

use screenstitch_core::{Engine, RectF};
use screenstitch_platform::config::Config;
use screenstitch_platform::display::{self, Display};
use screenstitch_platform::tray::{
    MSG_ALIGNMENT_LINE, MSG_OPEN_SETTINGS, MSG_RELOAD, WINDOW_CLASS, taskbar_created_message,
};
use screenstitch_platform::wide::{fill_wide, to_wide};
use screenstitch_platform::{desk_rects, layout};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, HICON, KillTimer, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PBT_APMRESUMEAUTOMATIC,
    PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetMenuDefaultItem, SetTimer, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_DISPLAYCHANGE,
    WM_HOTKEY, WM_LBUTTONUP, WM_NULL, WM_POWERBROADCAST, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW,
};

use crate::{fullscreen, hook, icon, overlay};

const WM_TRAY: u32 = WM_APP + 1;
const TIMER_REDETECT: usize = 1;
/// Once a second: look for a fullscreen game (the hook is removed while one is in
/// front) and check the hook is still alive.
const TIMER_TICK: usize = 2;
const HOTKEY_TOGGLE: i32 = 1;

const CMD_OPEN: usize = 10;
const CMD_TOGGLE: usize = 11;
const CMD_REDETECT: usize = 12;
const CMD_QUIT: usize = 13;

struct App {
    hwnd: HWND,
    config: Config,
    displays: Vec<Display>,
    rects: Vec<RectF>,
    /// A fullscreen app is in front: the hook is fully removed until it's gone.
    paused_for_fullscreen: bool,
    icon: HICON,
    taskbar_created: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn run(open_settings_now: bool) {
    let hinst = unsafe { GetModuleHandleW(null()) };
    let class = to_wide(WINDOW_CLASS);
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: null_mut(),
        hCursor: null_mut(),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    unsafe { RegisterClassW(&wc) };
    // A real (hidden) top-level window: message-only windows miss WM_DISPLAYCHANGE.
    let title = to_wide("ScreenStitch Tray");
    let hwnd = unsafe {
        CreateWindowExW(0, class.as_ptr(), title.as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null())
    };
    if hwnd.is_null() {
        return;
    }

    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            hwnd,
            config: Config::load(),
            displays: Vec::new(),
            rects: Vec::new(),
            paused_for_fullscreen: false,
            icon: null_mut(),
            taskbar_created: taskbar_created_message(),
        });
    });
    unsafe {
        RegisterHotKey(hwnd, HOTKEY_TOGGLE, MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, u32::from(b'S'));
        SetTimer(hwnd, TIMER_TICK, 1000, None);
    }
    with_app(|app| {
        app.redetect();
        app.tray(NIM_ADD);
    });
    if open_settings_now {
        open_settings();
    }

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    overlay::hide();
    hook::stop();
}

/// Start the settings window (it focuses an already open one by itself).
pub fn open_settings() {
    let Ok(exe) = std::env::current_exe() else { return };
    let settings = exe.with_file_name("screenstitch-settings.exe");
    let _ = std::process::Command::new(settings).spawn();
}

impl App {
    /// Read the monitors again, then apply the config.
    fn redetect(&mut self) {
        hook::release_clip();
        self.displays = display::detect();
        self.apply();
    }

    /// (Re)start or stop the cursor handling from the current config.
    fn apply(&mut self) {
        let (rects, guessed) = desk_rects(&mut self.config, &self.displays);
        if guessed {
            let _ = self.config.save();
        }
        self.rects = rects;
        if self.config.enabled && !self.paused_for_fullscreen {
            hook::start(Engine::new(layout(&self.displays, &self.rects)), self.config.pause_in_fullscreen);
        } else {
            hook::stop();
        }
        if overlay::is_visible() {
            overlay::show(&self.displays, &self.rects);
        }
    }

    /// Games get the mouse exactly as if ScreenStitch weren't running: while a
    /// fullscreen app is in front there is no hook in the input path at all.
    fn check_fullscreen(&mut self) {
        let pause = self.config.enabled && self.config.pause_in_fullscreen && fullscreen::app_in_front();
        if pause != self.paused_for_fullscreen {
            self.paused_for_fullscreen = pause;
            self.apply();
            self.tray(NIM_MODIFY);
        }
    }

    fn set_enabled(&mut self, on: bool) {
        self.config.enabled = on;
        let _ = self.config.save();
        self.apply();
        self.tray(NIM_MODIFY);
    }

    fn tray(&mut self, action: u32) {
        let old = self.icon;
        self.icon = icon::tray_icon(self.config.enabled && !self.paused_for_fullscreen);
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = self.icon;
        fill_wide(&mut nid.szTip, &self.status());
        unsafe { Shell_NotifyIconW(action, &nid) };
        if !old.is_null() {
            unsafe { DestroyIcon(old) };
        }
    }

    fn status(&self) -> String {
        match (self.config.enabled, self.displays.len()) {
            (false, _) => "ScreenStitch is off".into(),
            _ if self.paused_for_fullscreen => "ScreenStitch is paused for a fullscreen app".into(),
            (true, 1) => "ScreenStitch is on · 1 screen".into(),
            (true, n) => format!("ScreenStitch is on · {n} screens"),
        }
    }
}

fn show_menu(hwnd: HWND) {
    let Some((enabled, status)) = with_app(|a| (a.config.enabled, a.status())) else { return };
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, to_wide(&status).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_OPEN, to_wide("Open ScreenStitch").as_ptr());
        let toggle = if enabled { "Turn off\tCtrl+Alt+Shift+S" } else { "Turn on\tCtrl+Alt+Shift+S" };
        AppendMenuW(menu, MF_STRING, CMD_TOGGLE, to_wide(toggle).as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_REDETECT, to_wide("Detect screens again").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_QUIT, to_wide("Quit").as_ptr());
        SetMenuDefaultItem(menu, CMD_OPEN as u32, 0);

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // The cursor is probably confined to one screen; the menu must be free to open anywhere.
        hook::release_clip();
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        run_command(hwnd, cmd as usize);
    }
}

fn run_command(hwnd: HWND, cmd: usize) {
    match cmd {
        CMD_OPEN => open_settings(),
        CMD_TOGGLE => {
            with_app(|a| a.set_enabled(!a.config.enabled));
        }
        CMD_REDETECT => {
            with_app(|a| {
                a.redetect();
                a.tray(NIM_MODIFY);
            });
        }
        CMD_QUIT => unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        },
        _ => {}
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match (lparam & 0xFFFF) as u32 {
                WM_LBUTTONUP => open_settings(),
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            0
        }
        MSG_RELOAD => {
            // The settings window saved a change: pick it up right away.
            with_app(|a| {
                a.config = Config::load();
                a.apply();
                a.tray(NIM_MODIFY);
            });
            0
        }
        MSG_OPEN_SETTINGS => {
            open_settings();
            0
        }
        MSG_ALIGNMENT_LINE => {
            if wparam != 0 {
                with_app(|a| overlay::show(&a.displays, &a.rects));
            } else {
                overlay::hide();
            }
            0
        }
        WM_HOTKEY if wparam == HOTKEY_TOGGLE as WPARAM => {
            with_app(|a| a.set_enabled(!a.config.enabled));
            0
        }
        WM_DISPLAYCHANGE => {
            // Free the cursor at once, then wait for the displays to settle before re-reading them.
            hook::release_clip();
            unsafe { SetTimer(hwnd, TIMER_REDETECT, 600, None) };
            0
        }
        WM_POWERBROADCAST => {
            if wparam == PBT_APMRESUMEAUTOMATIC as WPARAM {
                hook::release_clip();
                unsafe { SetTimer(hwnd, TIMER_REDETECT, 1500, None) };
            }
            1
        }
        WM_TIMER if wparam == TIMER_TICK => {
            with_app(App::check_fullscreen);
            hook::watchdog();
            0
        }
        WM_TIMER if wparam == TIMER_REDETECT => {
            unsafe { KillTimer(hwnd, TIMER_REDETECT) };
            with_app(|a| {
                a.redetect();
                a.tray(NIM_MODIFY);
            });
            0
        }
        WM_SETTINGCHANGE => {
            // Taskbar switched between light and dark: redraw the icon to match.
            with_app(|a| a.tray(NIM_MODIFY));
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_CLOSE => {
            overlay::hide();
            hook::stop();
            with_app(|a| {
                a.tray(NIM_DELETE);
                unsafe { UnregisterHotKey(a.hwnd, HOTKEY_TOGGLE) };
            });
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => {
            if with_app(|a| a.taskbar_created == msg).unwrap_or(false) {
                // Explorer restarted: our icon is gone, add it back.
                with_app(|a| a.tray(NIM_ADD));
                return 0;
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}
