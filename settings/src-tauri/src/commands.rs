//! What the settings page can ask for. Every change is written to
//! config.json and the tray app is told to apply it immediately.

use screenstitch_core::RectF;
use screenstitch_platform::config::{self, Appearance, Config};
use screenstitch_platform::display::{self, Display};
use screenstitch_platform::tray::{self, MSG_ALIGNMENT_LINE, MSG_RELOAD};
use screenstitch_platform::{alignment_line_mm, auto_rects, autostart, desk_rects, layout, save_rects};
use serde::{Deserialize, Serialize};

use crate::system;

#[derive(Serialize)]
pub struct PxRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Desk {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    id: String,
    name: String,
    number: u32,
    primary: bool,
    scale: u32,
    px: PxRect,
    /// Physical size as detected (before any correction by the user).
    detected_mm: [f64; 2],
    size_from_edid: bool,
    desk: Desk,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    enabled: bool,
    pause_in_fullscreen: bool,
    stop_at_gaps: bool,
    check_updates: bool,
    start_with_windows: bool,
    appearance: Appearance,
    system_accent: String,
    tray_running: bool,
    version: &'static str,
    screens: Vec<Screen>,
    /// Ids of screens the cursor can't reach with this layout.
    unreachable: Vec<String>,
}

#[derive(Deserialize)]
pub struct DeskEntry {
    id: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Serialize)]
pub struct LayoutResult {
    unreachable: Vec<String>,
    line_mm: f64,
}

fn unreachable(displays: &[Display], rects: &[RectF]) -> Vec<String> {
    layout(displays, rects, false).unreachable().into_iter().map(|i| displays[i].id.clone()).collect()
}

fn to_desk(r: &RectF) -> Desk {
    Desk { x: r.x, y: r.y, w: r.w, h: r.h }
}

fn update(f: impl FnOnce(&mut Config)) -> Result<(), String> {
    let mut c = Config::load();
    f(&mut c);
    c.save().map_err(|e| format!("Couldn't save settings: {e}"))?;
    tray::post(MSG_RELOAD, 0);
    Ok(())
}

#[tauri::command]
pub fn get_state() -> State {
    let mut config = Config::load();
    let displays = display::detect();
    let (rects, guessed) = desk_rects(&mut config, &displays);
    if guessed {
        let _ = config.save();
    }
    State {
        enabled: config.enabled,
        pause_in_fullscreen: config.pause_in_fullscreen,
        stop_at_gaps: config.stop_at_gaps,
        check_updates: config.check_updates,
        start_with_windows: autostart::is_enabled(),
        appearance: config.appearance.clone(),
        system_accent: system::system_accent(),
        tray_running: tray::is_running(),
        version: env!("CARGO_PKG_VERSION"),
        unreachable: unreachable(&displays, &rects),
        screens: displays
            .iter()
            .zip(&rects)
            .map(|(d, r)| Screen {
                id: d.id.clone(),
                name: d.name.clone(),
                number: d.number,
                primary: d.primary,
                scale: d.scale,
                px: PxRect { x: d.px.left, y: d.px.top, w: d.px.width(), h: d.px.height() },
                detected_mm: [d.size_mm.0, d.size_mm.1],
                size_from_edid: d.size_from_edid,
                desk: to_desk(r),
            })
            .collect(),
    }
}

/// Save where the screens sit (and how big they are) for this monitor set.
#[tauri::command]
pub fn save_layout(desk: Vec<DeskEntry>) -> Result<LayoutResult, String> {
    let displays = display::detect();
    let mut config = Config::load();
    let (mut rects, _) = desk_rects(&mut config, &displays);
    for (d, r) in displays.iter().zip(rects.iter_mut()) {
        if let Some(e) = desk.iter().find(|e| e.id == d.id)
            && e.w > 0.0
            && e.h > 0.0
        {
            *r = RectF::new(e.x, e.y, e.w, e.h);
        }
    }
    // Only the relative placement matters; keep the numbers tidy.
    let min_x = rects.iter().map(|r| r.x).fold(f64::INFINITY, f64::min);
    let min_y = rects.iter().map(|r| r.y).fold(f64::INFINITY, f64::min);
    for r in &mut rects {
        r.x -= min_x;
        r.y -= min_y;
    }
    save_rects(&mut config, &displays, &rects);
    config.save().map_err(|e| format!("Couldn't save the layout: {e}"))?;
    tray::post(MSG_RELOAD, 0);
    Ok(LayoutResult { unreachable: unreachable(&displays, &rects), line_mm: alignment_line_mm(&rects) })
}

/// The layout ScreenStitch would guess on its own (not saved).
#[tauri::command]
pub fn auto_layout() -> Vec<Desk> {
    auto_rects(&display::detect()).iter().map(to_desk).collect()
}

#[tauri::command]
pub fn set_enabled(on: bool) -> Result<(), String> {
    update(|c| c.enabled = on)
}

#[tauri::command]
pub fn set_pause_in_fullscreen(on: bool) -> Result<(), String> {
    update(|c| c.pause_in_fullscreen = on)
}

#[tauri::command]
pub fn set_stop_at_gaps(on: bool) -> Result<(), String> {
    update(|c| c.stop_at_gaps = on)
}

#[tauri::command]
pub fn set_check_updates(on: bool) -> Result<(), String> {
    update(|c| c.check_updates = on)
}

/// Why this window was opened: "settings" or "update" (the daily check).
pub struct LaunchMode(pub &'static str);

#[tauri::command]
pub fn launch_mode(mode: tauri::State<'_, LaunchMode>) -> &'static str {
    mode.0
}

#[tauri::command]
pub fn set_start_with_windows(on: bool) -> Result<bool, String> {
    let exe = system::tray_exe().ok_or("ScreenStitch.exe wasn't found next to the settings app.")?;
    if autostart::set(on, &exe) {
        Ok(autostart::is_enabled())
    } else {
        Err("Windows didn't allow changing the startup setting.".into())
    }
}

#[tauri::command]
pub fn set_appearance(appearance: Appearance) -> Result<(), String> {
    let mut c = Config::load();
    c.appearance = appearance;
    c.save().map_err(|e| format!("Couldn't save settings: {e}"))
}

/// Frosted glass or solid, tinted for the current light/dark theme.
/// Returns the material the window actually got.
#[tauri::command]
pub fn apply_material(window: tauri::WebviewWindow, material: String, dark: bool) -> String {
    crate::material::apply(&window, &material, dark).to_string()
}

#[tauri::command]
pub fn set_alignment_line(on: bool) -> bool {
    tray::post(MSG_ALIGNMENT_LINE, usize::from(on))
}

/// Only fixed destinations: the page can't make us open arbitrary things.
#[tauri::command]
pub fn open_link(which: String) {
    match which.as_str() {
        "source" => system::open("https://github.com/BraxtonElmer/ScreenStitch"),
        "issues" => system::open("https://github.com/BraxtonElmer/ScreenStitch/issues"),
        "kofi" => system::open("https://ko-fi.com/akariyu"),
        "folder" => {
            let dir = config::dir();
            let _ = std::fs::create_dir_all(&dir);
            system::open(&dir.to_string_lossy());
        }
        _ => {}
    }
}
