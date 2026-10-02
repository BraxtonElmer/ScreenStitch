//! The window's background material: frosted glass (Acrylic) or solid.

use tauri::WebviewWindow;
use tauri::window::{Color, Effect, EffectsBuilder};

/// Apply `wanted` and return what the window actually got ("acrylic" or
/// "solid") so the page can pick matching surface colours. Anything other
/// than "solid", including settings saved by older versions, means glass.
pub fn apply(window: &WebviewWindow, wanted: &str, dark: bool) -> &'static str {
    if wanted != "solid" {
        let glass = EffectsBuilder::new()
            .effect(Effect::Acrylic)
            .color(if dark { Color(20, 20, 23, 210) } else { Color(214, 214, 218, 200) })
            .build();
        if window.set_effects(glass).is_ok() {
            return "acrylic";
        }
    }
    let _ = window.set_effects(None);
    "solid"
}
