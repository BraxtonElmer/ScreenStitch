//! The agent's hidden window: tray icon, menu, hotkey and reacting to
//! display, theme and power changes. Everything runs on one thread.

use std::cell::RefCell;
use std::ptr::{null, null_mut};

use screenstitch_core::{Engine, Layout, Monitor, RectF, auto_arrange};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, ShellExecuteW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, HICON, KillTimer, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PBT_APMRESUMEAUTOMATIC,
    PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SW_SHOWNORMAL, SetForegroundWindow,
    SetTimer, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE,
    WM_DESTROY, WM_DISPLAYCHANGE, WM_HOTKEY, WM_LBUTTONUP, WM_NULL, WM_POWERBROADCAST, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW,
};

use crate::config::{self, Config, DeskRect, Profile};
use crate::display::{self, Display};
use crate::wide::{fill_wide, to_wide};
use crate::{hook, icon};

const WM_TRAY: u32 = WM_APP + 1;
const TIMER_RELOAD: usize = 1;
const HOTKEY_TOGGLE: i32 = 1;

const CMD_TOGGLE: usize = 10;
const CMD_RELOAD: usize = 11;
const CMD_OPEN_FOLDER: usize = 12;
const CMD_QUIT: usize = 13;

struct App {
    hwnd: HWND,
    config: Config,
    displays: Vec<Display>,
    icon: HICON,
    taskbar_created: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn run() {
    let hinst = unsafe { GetModuleHandleW(null()) };
    let class = to_wide("ScreenStitchAgent");
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
    let title = to_wide("ScreenStitch");
    let hwnd = unsafe {
        CreateWindowExW(0, class.as_ptr(), title.as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null())
    };
    if hwnd.is_null() {
        return;
    }

    let taskbar_created = unsafe { RegisterWindowMessageW(to_wide("TaskbarCreated").as_ptr()) };
    let config = Config::load();
    APP.with(|a| {
        *a.borrow_mut() = Some(App { hwnd, config, displays: Vec::new(), icon: null_mut(), taskbar_created });
    });
    unsafe {
        RegisterHotKey(hwnd, HOTKEY_TOGGLE, MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, u32::from(b'S'));
    }
    with_app(|app| {
        app.reload();
        app.tray(NIM_ADD);
    });

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    hook::stop();
}

impl App {
    fn reload(&mut self) {
        hook::release_clip();
        self.displays = display::detect();
        let layout = self.layout();
        if self.config.enabled {
            hook::start(Engine::new(layout));
        } else {
            hook::stop();
        }
    }

    /// The saved desk layout for this set of monitors, or a fresh guess (saved
    /// right away so it can be edited).
    fn layout(&mut self) -> Layout {
        let key = config::profile_key(self.displays.iter().map(|d| d.id.as_str()));
        let saved = self.config.profiles.get(&key).filter(|p| self.displays.iter().all(|d| p.monitors.contains_key(&d.id)));
        let rects: Vec<RectF> = match saved {
            Some(p) => self
                .displays
                .iter()
                .map(|d| {
                    let r = p.monitors[&d.id];
                    RectF::new(r.x, r.y, r.w, r.h)
                })
                .collect(),
            None => {
                let px: Vec<_> = self.displays.iter().map(|d| d.px).collect();
                let mm: Vec<_> = self.displays.iter().map(|d| d.size_mm).collect();
                let primary = self.displays.iter().position(|d| d.primary).unwrap_or(0);
                let rects = auto_arrange(&px, &mm, primary);
                let monitors = self
                    .displays
                    .iter()
                    .zip(&rects)
                    .map(|(d, r)| (d.id.clone(), DeskRect { x: r.x, y: r.y, w: r.w, h: r.h }))
                    .collect();
                self.config.profiles.insert(key, Profile { monitors });
                let _ = self.config.save();
                rects
            }
        };
        Layout::new(self.displays.iter().zip(rects).map(|(d, mm)| Monitor::new(d.px, mm)).collect())
    }

    fn set_enabled(&mut self, on: bool) {
        self.config.enabled = on;
        let _ = self.config.save();
        self.reload();
        self.tray(NIM_MODIFY);
    }

    fn tray(&mut self, action: u32) {
        let old = self.icon;
        self.icon = icon::tray_icon(self.config.enabled);
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
        let n = self.displays.len();
        match (self.config.enabled, n) {
            (false, _) => "ScreenStitch is off".into(),
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
        let toggle = if enabled { "Turn off\tCtrl+Alt+Shift+S" } else { "Turn on\tCtrl+Alt+Shift+S" };
        AppendMenuW(menu, MF_STRING, CMD_TOGGLE, to_wide(toggle).as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_RELOAD, to_wide("Detect screens again").as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_OPEN_FOLDER, to_wide("Open settings folder").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_QUIT, to_wide("Quit").as_ptr());

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // The cursor is probably clipped to one screen; the menu must be free to open anywhere.
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
        CMD_TOGGLE => {
            with_app(|a| a.set_enabled(!a.config.enabled));
        }
        CMD_RELOAD => {
            with_app(|a| {
                a.reload();
                a.tray(NIM_MODIFY);
            });
        }
        CMD_OPEN_FOLDER => {
            let dir = config::dir();
            let _ = std::fs::create_dir_all(&dir);
            let dir = to_wide(&dir.to_string_lossy());
            unsafe {
                ShellExecuteW(null_mut(), to_wide("open").as_ptr(), dir.as_ptr(), null(), null(), SW_SHOWNORMAL)
            };
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
            let event = (lparam & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_menu(hwnd);
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
            unsafe { SetTimer(hwnd, TIMER_RELOAD, 600, None) };
            0
        }
        WM_POWERBROADCAST => {
            if wparam == PBT_APMRESUMEAUTOMATIC as WPARAM {
                hook::release_clip();
                unsafe { SetTimer(hwnd, TIMER_RELOAD, 1500, None) };
            }
            1
        }
        WM_TIMER if wparam == TIMER_RELOAD => {
            unsafe { KillTimer(hwnd, TIMER_RELOAD) };
            with_app(|a| {
                a.reload();
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
            hook::stop();
            with_app(|a| {
                a.tray(NIM_DELETE);
                unsafe { UnregisterHotKey(a.hwnd, HOTKEY_TOGGLE) };
            });
            unsafe { PostQuitMessage(0) };
            0
        }
        WM_DESTROY => 0,
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
