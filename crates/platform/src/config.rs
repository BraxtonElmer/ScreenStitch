//! `%APPDATA%\ScreenStitch\config.json`: on/off and one saved desk layout per
//! set of connected monitors, so docking and undocking just switch layouts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Step aside while a fullscreen game or presentation is in front.
    #[serde(default = "yes")]
    pub pause_in_fullscreen: bool,
    /// Look for a new version about once a day.
    #[serde(default = "yes")]
    pub check_updates: bool,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

/// Settings window look. The tray app ignores it.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Appearance {
    /// "system", "light" or "dark".
    pub theme: String,
    /// Accent colour as `#rrggbb`; empty = the Windows accent colour.
    pub accent: String,
    /// Window background: "acrylic" (frosted glass), "mica" or "solid".
    pub material: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self { theme: "system".into(), accent: String::new(), material: "acrylic".into() }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Profile {
    /// Monitor id → where it sits on the desk, in mm.
    pub monitors: BTreeMap<String, DeskRect>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct DeskRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: true,
            pause_in_fullscreen: true,
            check_updates: true,
            appearance: Appearance::default(),
            profiles: BTreeMap::new(),
        }
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("ScreenStitch")
}

fn path() -> PathBuf {
    dir().join("config.json")
}

impl Config {
    /// A missing or unreadable file gives defaults; a broken file is kept as
    /// `config.json.bad` so hand edits are never silently lost.
    pub fn load() -> Self {
        let p = path();
        match std::fs::read_to_string(&p) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
                let _ = std::fs::copy(&p, p.with_extension("json.bad"));
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Write to a temp file and rename, so a crash never leaves half a file.
    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(dir())?;
        let p = path();
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &p)
    }
}

/// Same monitors in any order → same profile.
pub fn profile_key<'a>(ids: impl IntoIterator<Item = &'a str>) -> String {
    let mut ids: Vec<&str> = ids.into_iter().collect();
    ids.sort_unstable();
    ids.join("+")
}
