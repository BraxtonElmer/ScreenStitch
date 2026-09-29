// No console window behind the settings window.
#![windows_subsystem = "windows"]

mod commands;
mod material;
mod system;

use tauri::{Manager, Theme};

use screenstitch_platform::config::Config;
use screenstitch_platform::tray::{self, MSG_ALIGNMENT_LINE};

fn main() {
    // Only one settings window: a second launch brings the first one forward.
    if !system::claim_single_instance() {
        system::focus_existing_window();
        return;
    }
    system::ensure_tray_running();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_layout,
            commands::auto_layout,
            commands::set_enabled,
            commands::set_pause_in_fullscreen,
            commands::set_start_with_windows,
            commands::set_appearance,
            commands::apply_material,
            commands::set_alignment_line,
            commands::open_link,
        ])
        .setup(|app| {
            let window = app.get_webview_window("main").expect("main window");
            // Set the saved material before the page shows, so it doesn't flash.
            let a = Config::load().appearance;
            let dark = match a.theme.as_str() {
                "dark" => true,
                "light" => false,
                _ => window.theme().is_ok_and(|t| t == Theme::Dark),
            };
            material::apply(&window, &a.material, dark);
            Ok(())
        })
        .on_window_event(|_, event| {
            if let tauri::WindowEvent::Destroyed = event {
                // Never leave the alignment line on the screens after closing.
                tray::post(MSG_ALIGNMENT_LINE, 0);
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to start the settings window");
}
