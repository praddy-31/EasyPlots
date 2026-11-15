//! Colours for the launcher window (not for plots).
//!
//! The colour names are Palette's (github.com/praddy-31/Palette), so a Palette theme drops
//! straight in. `[launcher] theme` in config.toml picks where they come from:
//!
//! - `"palette"`: follow Palette. Palette writes the current theme's colours to
//!   `~/.config/palette/current/easyplots.json`. Until Palette has an EasyPlots target, the
//!   Habit Tracker's file next to it (same keys) is used. No file → `"auto"`.
//! - `"auto"`: Catppuccin Mocha in dark mode, Rosé Pine Dawn in light mode.
//! - a built-in name (`"gruvbox-dark"`, …): always that theme.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

/// Every colour the launcher uses, by Palette's names.
pub const KEYS: [&str; 14] = [
    "background",
    "foreground",
    "accent",
    "selection",
    "muted",
    "dark_foreground",
    "light_foreground",
    "red",
    "orange",
    "yellow",
    "green",
    "cyan",
    "blue",
    "magenta",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UiTheme {
    pub name: String,
    /// "dark" or "light".
    pub mode: String,
    pub colors: BTreeMap<String, String>,
}

/// The themes to use in dark and light mode (the same one twice unless `auto`), where
/// they came from, and the Palette file's date so the window can notice a switch.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resolved {
    pub dark: UiTheme,
    pub light: UiTheme,
    pub source: String,
    #[serde(skip)]
    pub file: Option<PathBuf>,
    #[serde(skip)]
    pub modified: Option<SystemTime>,
}

/// Built-in themes: colours copied from Palette's bundled themes (Omarchy's, MIT).
const BUILTIN: &[(&str, &str, [&str; 14])] = &[
    (
        "catppuccin-mocha",
        "dark",
        [
            "#1e1e2e", "#cdd6f4", "#89b4fa", "#45475a", "#585b70", "#6c7086", "#bac2de", "#f38ba8",
            "#f6b6ab", "#f9e2af", "#a6e3a1", "#94e2d5", "#89b4fa", "#f5c2e7",
        ],
    ),
    (
        "rose-pine-dawn",
        "light",
        [
            "#faf4ed", "#575279", "#56949f", "#dfdad9", "#cecacd", "#9893a5", "#6e6a86", "#b4637a",
            "#cf8057", "#ea9d34", "#286983", "#d7827e", "#56949f", "#907aa9",
        ],
    ),
    (
        "gruvbox-dark",
        "dark",
        [
            "#282828", "#d4be98", "#7daea3", "#504945", "#665c54", "#7c6f64", "#bdae93", "#ea6962",
            "#e1875c", "#d8a657", "#a9b665", "#89b482", "#7daea3", "#d3869b",
        ],
    ),
    (
        "tokyo-night",
        "dark",
        [
            "#1a1b26", "#a9b1d6", "#7aa2f7", "#292e42", "#414868", "#565f89", "#b4bee6", "#f7768e",
            "#eb927b", "#e0af68", "#9ece6a", "#449dab", "#7aa2f7", "#ad8ee6",
        ],
    ),
    (
        "everforest",
        "dark",
        [
            "#2d353b", "#d3c6aa", "#7fbbb3", "#3d484d", "#475258", "#4f585e", "#9da9a0", "#e67e80",
            "#e09d7f", "#dbbc7f", "#a7c080", "#83c092", "#7fbbb3", "#d699b6",
        ],
    ),
    (
        "kanagawa",
        "dark",
        [
            "#1f1f28", "#dcd7ba", "#dcd7ba", "#363646", "#54546d", "#727169", "#c8c093", "#c34043",
            "#c17158", "#c0a36e", "#76946a", "#6a9589", "#7e9cd8", "#957fb8",
        ],
    ),
];

pub fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTIN.iter().map(|(n, _, _)| *n)
}

pub fn builtin(name: &str) -> Option<UiTheme> {
    BUILTIN
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
        .map(|(n, mode, values)| UiTheme {
            name: (*n).to_owned(),
            mode: (*mode).to_owned(),
            colors: KEYS
                .iter()
                .zip(values)
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        })
}

fn is_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Read a Palette colours file (JSON). Only plain `#rrggbb` values are taken; anything
/// missing or odd falls back to the built-in theme of the same mode.
pub fn from_palette_json(text: &str) -> Option<UiTheme> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let obj = value.as_object()?;
    let mode = match obj.get("mode").and_then(|m| m.as_str()) {
        Some("light") => "light",
        _ => "dark",
    };
    let fallback = builtin(if mode == "light" {
        "rose-pine-dawn"
    } else {
        "catppuccin-mocha"
    })?;
    let mut found = 0;
    let colors = KEYS
        .iter()
        .map(|k| {
            let v = obj
                .get(*k)
                .and_then(|v| v.as_str())
                .filter(|v| is_hex(v))
                .map(|v| {
                    found += 1;
                    v.to_ascii_lowercase()
                })
                .unwrap_or_else(|| fallback.colors[*k].clone());
            ((*k).to_owned(), v)
        })
        .collect();
    // A file with none of the colours isn't a Palette file.
    (found > 0).then(|| UiTheme {
        name: obj
            .get("theme")
            .and_then(|t| t.as_str())
            .unwrap_or("palette")
            .to_owned(),
        mode: mode.to_owned(),
        colors,
    })
}

/// Where Palette keeps the current theme's files.
pub fn palette_dir() -> PathBuf {
    crate::config::home().join(".config/palette/current")
}

/// Work out the launcher's themes from the `[launcher] theme` setting.
pub fn resolve(setting: &str, palette_dir: &Path) -> Resolved {
    let auto = || Resolved {
        dark: builtin("catppuccin-mocha").expect("built in"),
        light: builtin("rose-pine-dawn").expect("built in"),
        source: "built-in (follows macOS light/dark)".to_owned(),
        file: None,
        modified: None,
    };
    let setting = setting.trim().to_ascii_lowercase();
    if setting == "palette" {
        for name in ["easyplots.json", "habit-tracker.json"] {
            let file = palette_dir.join(name);
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if let Some(theme) = from_palette_json(&text) {
                let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
                return Resolved {
                    source: format!("Palette: {}", theme.name),
                    dark: theme.clone(),
                    light: theme,
                    file: Some(file),
                    modified,
                };
            }
        }
        let mut r = auto();
        r.source = "built-in (Palette's colour file not found)".to_owned();
        return r;
    }
    match builtin(&setting) {
        Some(theme) => Resolved {
            source: format!("built-in: {}", theme.name),
            dark: theme.clone(),
            light: theme,
            file: None,
            modified: None,
        },
        None => auto(),
    }
}

/// The launcher window's glass as Palette sets it (`palette glass easyplots`): the
/// `glass` block in `easyplots.json`. Kind `own` (or no block) leaves it to config.toml.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Glass {
    /// "solid", "blurred" or "clear".
    pub kind: &'static str,
    /// 0 (fully see-through) to 1 (solid). No limit for readability: Settings warns.
    pub opacity: f64,
    /// 0 (off) to 100.
    pub blur: u32,
}

/// What the launcher window looks like: from Palette when it sets the glass, else from
/// `[launcher] opacity` / `blur`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Look {
    pub opacity: f64,
    pub blur: u32,
    /// Palette is in charge (Settings shows a banner and dims its own controls).
    pub palette: Option<Glass>,
    /// Text may be hard to read (opacity below `READABLE_OPACITY`).
    pub faint: bool,
}

/// Read the `glass` block of a Palette colours file.
pub fn glass_from_json(text: &str) -> Option<Glass> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let g = value.get("glass")?.as_object()?;
    let kind = match g.get("kind")?.as_str()? {
        "solid" => "solid",
        "blurred" => "blurred",
        "clear" => "clear",
        _ => return None,
    };
    let opacity = g
        .get("opacity")
        .and_then(|o| o.as_f64().or_else(|| o.as_str()?.parse().ok()))
        .filter(|o| o.is_finite())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let blur = g.get("blur").and_then(|b| b.as_u64()).unwrap_or(0).min(100) as u32;
    Some(match kind {
        "solid" => Glass {
            kind,
            opacity: 1.0,
            blur: 0,
        },
        "clear" => Glass {
            kind,
            opacity,
            blur: 0,
        },
        _ => Glass {
            kind,
            opacity,
            blur,
        },
    })
}

/// Palette's glass for EasyPlots and the file's date, read whatever the launcher's colour
/// setting is (the glass is its own Palette control).
pub fn palette_glass(palette_dir: &Path) -> (Option<Glass>, Option<SystemTime>) {
    let file = palette_dir.join("easyplots.json");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return (None, None);
    };
    let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    (glass_from_json(&text), modified)
}

/// The launcher's look: Palette's glass if it sets one, else the config's values.
pub fn look(opacity: f64, blur: u32, glass: Option<Glass>) -> Look {
    let (opacity, blur) = glass.map_or((opacity, blur), |g| (g.opacity, g.blur));
    Look {
        opacity,
        blur,
        palette: glass,
        faint: opacity < crate::config::READABLE_OPACITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_glass_wins_and_is_never_capped() {
        let base = r##"{"palette":1,"theme":"nord","background":"#2e3440""##;
        let with = |g: &str| glass_from_json(&format!("{base},\"glass\":{g}}}"));
        assert_eq!(glass_from_json(&format!("{base}}}")), None);
        assert_eq!(with(r#"{"kind":"own","opacity":0.5,"blur":50}"#), None);
        assert_eq!(
            with(r#"{"kind":"solid","opacity":0.5,"blur":50}"#),
            Some(Glass {
                kind: "solid",
                opacity: 1.0,
                blur: 0
            })
        );
        // 95 % see-through goes through as it is (only a warning in Settings).
        let clear = with(r#"{"kind":"clear","opacity":0.05,"blur":50}"#).unwrap();
        assert_eq!((clear.opacity, clear.blur), (0.05, 0));
        let blurred = with(r#"{"kind":"blurred","opacity":0.7,"blur":80}"#).unwrap();
        assert_eq!((blurred.opacity, blurred.blur), (0.7, 80));
        let l = look(0.9, 20, Some(clear));
        assert_eq!((l.opacity, l.blur, l.faint), (0.05, 0, true));
        let own = look(0.9, 20, None);
        assert_eq!((own.opacity, own.blur, own.faint), (0.9, 20, false));
        assert!(look(0.1, 0, None).faint);
    }

    #[test]
    fn builtins_are_complete() {
        for name in builtin_names() {
            let t = builtin(name).unwrap();
            assert_eq!(t.colors.len(), KEYS.len());
            assert!(t.colors.values().all(|v| is_hex(v)), "{name}");
        }
    }

    #[test]
    fn reads_palette_file_and_ignores_bad_values() {
        let json = r##"{"palette":1,"theme":"nord","mode":"dark","background":"#2E3440",
            "accent":"url(evil)","foreground":"#d8dee9"}"##;
        let t = from_palette_json(json).unwrap();
        assert_eq!(t.name, "nord");
        assert_eq!(t.colors["background"], "#2e3440");
        // Not a plain colour: replaced by the built-in value.
        assert_eq!(t.colors["accent"], "#89b4fa");
        assert!(from_palette_json("{}").is_none());
        assert!(from_palette_json("not json").is_none());
    }

    #[test]
    fn resolve_prefers_easyplots_file_then_habit_tracker() {
        let dir = std::env::temp_dir().join(format!("easyplots-palette-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(resolve("palette", &dir).source.contains("not found"));
        std::fs::write(
            dir.join("habit-tracker.json"),
            r##"{"theme":"nord","mode":"light","background":"#eceff4"}"##,
        )
        .unwrap();
        let r = resolve("palette", &dir);
        assert_eq!(r.source, "Palette: nord");
        assert_eq!(r.light.mode, "light");
        std::fs::write(
            dir.join("easyplots.json"),
            r##"{"theme":"kanagawa","background":"#1f1f28"}"##,
        )
        .unwrap();
        assert_eq!(resolve("palette", &dir).source, "Palette: kanagawa");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_builtin_and_auto() {
        let nowhere = Path::new("/nonexistent");
        assert_eq!(resolve("gruvbox-dark", nowhere).dark.name, "gruvbox-dark");
        let auto = resolve("auto", nowhere);
        assert_eq!(
            (auto.dark.name.as_str(), auto.light.name.as_str()),
            ("catppuccin-mocha", "rose-pine-dawn")
        );
    }
}
