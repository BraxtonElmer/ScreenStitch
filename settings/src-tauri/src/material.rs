//! The window's background material: frosted glass (Acrylic), Mica, or solid.

use tauri::WebviewWindow;
use tauri::window::{Color, Effect, EffectsBuilder};

use crate::system;

/// Apply `wanted` and return what the window actually got ("acrylic",
/// "mica" or "solid") so the page can pick matching surface colours.
pub fn apply(window: &WebviewWindow, wanted: &str, dark: bool) -> &'static str {
    let effects = match wanted {
        // A neutral tint keeps text readable over busy windows behind.
        "acrylic" => Some((
            "acrylic",
            EffectsBuilder::new()
                .effect(Effect::Acrylic)
                .color(if dark { Color(28, 28, 30, 150) } else { Color(245, 245, 247, 150) })
                .build(),
        )),
        "mica" if system::supports_mica() => {
            Some(("mica", EffectsBuilder::new().effect(Effect::Mica).build()))
        }
        _ => None,
    };
    if let Some((name, config)) = effects
        && window.set_effects(config).is_ok()
    {
        return name;
    }
    let _ = window.set_effects(None);
    "solid"
}
