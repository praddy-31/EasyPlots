//! The Settings window's side of `config.toml`: every option with a label, a hint and the
//! kind of control it needs (`FIELDS`), and `set`, which changes one value in the file's
//! text while keeping the owner's comments and layout (`toml_edit`).
//!
//! A change is only written if the whole file still reads as a valid config, so the form
//! can't leave a broken file behind.

use serde::Serialize;
use toml_edit::{DocumentMut, Item, Table, Value};

use crate::{Config, Error};

/// What kind of control an option needs.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Text,
    /// A free text box with suggestions (any installed font, a size…).
    Suggest {
        options: Vec<String>,
    },
    Number {
        min: f64,
        max: f64,
        step: f64,
    },
    Whole {
        min: i64,
        max: i64,
    },
    Bool,
    Choice {
        options: Vec<String>,
    },
    Hotkey,
    Folder,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Field {
    pub section: &'static str,
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

/// Every option in `config.toml`, grouped by section in the order the form shows them.
pub fn fields() -> Vec<Field> {
    let f = |section, key, label, help, kind| Field {
        section,
        key,
        label,
        help,
        kind,
    };
    let mut launcher_themes = vec!["palette".to_owned(), "auto".to_owned()];
    launcher_themes.extend(crate::ui_theme::builtin_names().map(str::to_owned));
    vec![
        f(
            "general",
            "hotkey",
            "Hotkey",
            "Opens the launcher from any app. Click and press the keys.",
            Kind::Hotkey,
        ),
        f(
            "general",
            "data_dir",
            "Data folder",
            "Where the file picker looks for CSV files.",
            Kind::Folder,
        ),
        f(
            "general",
            "search_whole_disk",
            "Search the whole disk",
            "Also list CSV files from everywhere, using the Spotlight index (file names only).",
            Kind::Bool,
        ),
        f(
            "general",
            "launch_at_login",
            "Launch at login",
            "Start EasyPlots when you log in.",
            Kind::Bool,
        ),
        f(
            "general",
            "json_files",
            "JSON files in the list",
            "all lists every .json file (after CSV files and workbooks); tables only those \
             that hold rows; known leaves out settings files such as package.json.",
            Kind::Choice {
                options: strings(&crate::search::JsonFiles::NAMES),
            },
        ),
        f(
            "output",
            "dir",
            "Plots folder",
            "Where plots are saved.",
            Kind::Folder,
        ),
        f(
            "output",
            "paste_dir",
            "Pasted data folder",
            "Tables pasted from the clipboard are kept here, so their plots can be redrawn.",
            Kind::Folder,
        ),
        f(
            "output",
            "format",
            "Format",
            "PNG for slides and screens, PDF or SVG for papers.",
            Kind::Choice {
                options: strings(&["png", "pdf", "svg"]),
            },
        ),
        f(
            "output",
            "copy_format",
            "Copy format",
            "What ⌘C puts on the clipboard. PNG is small and pastes wherever a screenshot does; TIFF is uncompressed (about 50 times larger).",
            Kind::Choice {
                options: strings(&["png", "tiff"]),
            },
        ),
        f(
            "output",
            "name",
            "File name",
            "Tokens: {file} {files} {type} {y} {date} {time} {n}.",
            Kind::Text,
        ),
        f(
            "output",
            "dpi",
            "PNG resolution (dpi)",
            "300 is normal for print.",
            Kind::Whole { min: 30, max: 2400 },
        ),
        f(
            "output",
            "size",
            "Figure size",
            "Width x height: 6x4in, 15x10cm, 160x100mm.",
            Kind::Suggest {
                options: strings(&["6x4in", "3.5x2.6in", "7x3in", "15x10cm"]),
            },
        ),
        f(
            "output",
            "after_save",
            "After saving",
            "Thumbnail keeps the launcher open with the path; reveal shows the file in Finder.",
            Kind::Choice {
                options: strings(&["thumbnail", "notify", "open", "reveal", "none"]),
            },
        ),
        f(
            "style",
            "theme",
            "Plot theme",
            "paper for journals; light, dark and palette for screens.",
            Kind::Choice {
                options: strings(&crate::style::THEMES),
            },
        ),
        f(
            "style",
            "font",
            "Plot font",
            "auto uses the theme's own font.",
            Kind::Choice {
                options: strings(&["auto", "STIX Two", "Latin Modern", "Source Sans"]),
            },
        ),
        f(
            "style",
            "font_size",
            "Font size (pt)",
            "Axis labels and legend; ticks are 90 %, titles 110 %.",
            Kind::Number {
                min: 4.0,
                max: 36.0,
                step: 0.5,
            },
        ),
        f(
            "style",
            "line_width",
            "Line width (pt)",
            "",
            Kind::Number {
                min: 0.25,
                max: 6.0,
                step: 0.25,
            },
        ),
        f(
            "style",
            "grid",
            "Grid on paper plots",
            "The screen themes always start with a grid.",
            Kind::Bool,
        ),
        f(
            "style",
            "legend",
            "Legend position",
            "best avoids the data; out puts it right of the plot.",
            Kind::Choice {
                options: strings(&["best", "tr", "tl", "br", "bl", "out", "off"]),
            },
        ),
        f(
            "launcher",
            "mode",
            "Opens in",
            "Picker builds the command step by step.",
            Kind::Choice {
                options: strings(&["picker", "command"]),
            },
        ),
        f(
            "launcher",
            "theme",
            "Launcher colours",
            "palette follows Palette's current theme; auto follows light and dark mode.",
            Kind::Choice {
                options: launcher_themes,
            },
        ),
        f(
            "launcher",
            "opacity",
            "Opacity",
            "How solid the launcher's background is: 1 is solid, 0 fully see-through.",
            Kind::Number {
                min: 0.0,
                max: 1.0,
                step: 0.05,
            },
        ),
        f(
            "launcher",
            "blur",
            "Blur",
            "How strongly what's behind the launcher is blurred: 0 off, 20 light (like Ghostty's background-blur).",
            Kind::Whole { min: 0, max: 100 },
        ),
        f(
            "launcher",
            "font",
            "Launcher font",
            "Any installed font; JetBrains Mono is built in.",
            Kind::Suggest {
                options: strings(&[
                    "JetBrains Mono",
                    "SF Mono",
                    "Menlo",
                    "Monaco",
                    "Courier New",
                ]),
            },
        ),
        f(
            "launcher",
            "font_size",
            "Launcher font size",
            "",
            Kind::Number {
                min: 9.0,
                max: 32.0,
                step: 1.0,
            },
        ),
        f(
            "launcher",
            "hints",
            "Key hints",
            "Show the keys at the bottom of the launcher.",
            Kind::Bool,
        ),
        f(
            "launcher",
            "live_preview",
            "Live preview",
            "Draw a small preview while you type a command.",
            Kind::Bool,
        ),
        f(
            "launcher",
            "advice",
            "Plot advisor",
            "A hint under the preview when a plot has a known pitfall (legend over data, \
             ranges far apart, many decades…); ⇥ adds the suggested option.",
            Kind::Bool,
        ),
        f(
            "launcher",
            "history_size",
            "Commands remembered",
            "",
            Kind::Whole { min: 1, max: 5000 },
        ),
        f(
            "launcher",
            "size",
            "Window size",
            "Width x height in points.",
            Kind::Suggest {
                options: strings(&["640x420", "720x480", "560x380"]),
            },
        ),
        f(
            "launcher",
            "preview_size",
            "Preview size",
            "The window size while a plot is shown.",
            Kind::Suggest {
                options: strings(&["900x680", "1100x800", "760x560"]),
            },
        ),
    ]
}

/// The current values as JSON, `{ "general": { "hotkey": "…" }, … }`.
/// An installed font family, for the launcher's font list.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FontFamily {
    pub name: String,
    /// Every letter equally wide, which suits the launcher's columns.
    pub mono: bool,
}

/// The font families installed on this computer, monospaced ones first, then by name.
/// Only the font files' name tables are read (about 0.1–0.3 s for a few hundred fonts).
pub fn installed_fonts() -> Vec<FontFamily> {
    use resvg::usvg::fontdb;
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut families: std::collections::BTreeMap<String, bool> = Default::default();
    for face in db.faces() {
        if let Some((name, _)) = face.families.first()
            && !name.starts_with('.')
        {
            *families.entry(name.clone()).or_default() |= face.monospaced;
        }
    }
    let mut out: Vec<FontFamily> = families
        .into_iter()
        .map(|(name, mono)| FontFamily { name, mono })
        .collect();
    out.sort_by(|a, b| b.mono.cmp(&a.mono).then(a.name.cmp(&b.name)));
    out
}

pub fn values(config: &Config) -> serde_json::Value {
    let text = toml::to_string(config).unwrap_or_default();
    let parsed: toml::Table = toml::from_str(&text).unwrap_or_default();
    serde_json::to_value(parsed).unwrap_or_default()
}

/// Set `[section] key = value` in the config file's text, keeping comments. Returns the new
/// text, or an error (in plain words) if the result wouldn't be a valid config.
pub fn set(
    text: &str,
    section: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<String, Error> {
    let field = fields()
        .into_iter()
        .find(|f| f.section == section && f.key == key)
        .ok_or_else(|| Error::new(format!("unknown setting {section}.{key}")))?;
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| Error::new(format!("config.toml can't be read: {e}")))?;
    let new: Value = match (&field.kind, value) {
        (Kind::Bool, serde_json::Value::Bool(b)) => (*b).into(),
        (Kind::Whole { min, max }, v) => {
            let n = v
                .as_f64()
                .filter(|n| n.fract() == 0.0)
                .ok_or_else(|| Error::new(format!("{} must be a whole number", field.label)))?
                as i64;
            if !(*min..=*max).contains(&n) {
                return Err(Error::new(format!(
                    "{} must be between {min} and {max}",
                    field.label
                )));
            }
            n.into()
        }
        (Kind::Number { min, max, .. }, v) => {
            let n = v
                .as_f64()
                .ok_or_else(|| Error::new(format!("{} must be a number", field.label)))?;
            if !(*min..=*max).contains(&n) {
                return Err(Error::new(format!(
                    "{} must be between {min} and {max}",
                    field.label
                )));
            }
            // Whole numbers stay whole in the file (10, not 10.0).
            if n.fract() == 0.0 && n.abs() < 1e15 {
                (n as i64).into()
            } else {
                n.into()
            }
        }
        (Kind::Choice { options }, serde_json::Value::String(s)) => {
            if !options.iter().any(|o| o == s) {
                return Err(Error::new(format!(
                    "{} must be one of: {}",
                    field.label,
                    options.join(", ")
                )));
            }
            s.as_str().into()
        }
        (_, serde_json::Value::String(s)) => s.trim().into(),
        _ => {
            return Err(Error::new(format!(
                "wrong kind of value for {}",
                field.label
            )));
        }
    };
    if !doc.contains_key(section) {
        doc.insert(section, Item::Table(Table::new()));
    }
    let table = doc[section]
        .as_table_mut()
        .ok_or_else(|| Error::new(format!("[{section}] in config.toml is not a section")))?;
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            // Keep the spacing and the comment after the old value, with the comment
            // staying in its column.
            let mut decor = old.decor().clone();
            let bare = |v: &Value| {
                let mut v = v.clone();
                v.decor_mut().clear();
                v.to_string().trim().chars().count() as i64
            };
            let (old_len, new_len) = (bare(old), bare(&new));
            if let Some(suffix) = decor.suffix().and_then(|s| s.as_str()).map(str::to_owned) {
                let rest = suffix.trim_start_matches(' ');
                let spaces = (suffix.len() - rest.len()) as i64;
                if spaces > 0 && !rest.is_empty() {
                    let width = (spaces - (new_len - old_len)).max(1) as usize;
                    decor.set_suffix(format!("{}{rest}", " ".repeat(width)));
                }
            }
            *old = new;
            *old.decor_mut() = decor;
        }
        None => {
            table.insert(key, Item::Value(new));
        }
    }
    let out = doc.to_string();
    Config::parse(&out).map_err(|e| Error::new(format!("not saved: {}", e.message)))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_TOML;

    #[test]
    fn installed_fonts_are_unique_and_monospaced_first() {
        let fonts = installed_fonts();
        let first_proportional = fonts.iter().position(|f| !f.mono).unwrap_or(fonts.len());
        assert!(fonts[first_proportional..].iter().all(|f| !f.mono));
        let mut names: Vec<_> = fonts.iter().map(|f| &f.name).collect();
        names.dedup();
        assert_eq!(names.len(), fonts.len());
    }

    /// The other way round: a value added to `config.toml` must get a control in Settings
    /// (or be listed here with the reason it has none).
    #[test]
    fn every_config_value_is_in_the_form() {
        // One possible value and nothing reads it yet: a control would do nothing.
        const NOT_IN_FORM: &[(&str, &str)] = &[("style", "palette")];
        let v = values(&Config::default());
        let form = fields();
        for (section, table) in v.as_object().unwrap() {
            for key in table.as_object().unwrap().keys() {
                let listed = form.iter().any(|f| f.section == section && f.key == key);
                let skipped = NOT_IN_FORM.contains(&(section.as_str(), key.as_str()));
                assert!(
                    listed != skipped,
                    "{section}.{key}: in config.toml but not in Settings (or no longer skipped)"
                );
            }
        }
    }

    /// The form's choices must be values the config accepts.
    #[test]
    fn every_choice_is_accepted() {
        for f in fields() {
            if let Kind::Choice { options } = &f.kind {
                for o in options {
                    let out = set(DEFAULT_TOML, f.section, f.key, &serde_json::json!(o));
                    assert!(
                        out.is_ok(),
                        "{}.{} = {o}: {:?}",
                        f.section,
                        f.key,
                        out.err()
                    );
                }
            }
        }
    }

    #[test]
    fn every_field_exists_in_the_config() {
        let v = values(&Config::default());
        for f in fields() {
            assert!(v[f.section].get(f.key).is_some(), "{}.{}", f.section, f.key);
        }
    }

    #[test]
    fn keeps_comments_and_checks_values() {
        let out = set(
            DEFAULT_TOML,
            "launcher",
            "opacity",
            &serde_json::json!(0.75),
        )
        .unwrap();
        assert!(
            out.contains("opacity  = 0.75                     # background opacity"),
            "{out}"
        );
        assert!(out.starts_with("# EasyPlots settings."));
        let out = set(&out, "general", "hotkey", &serde_json::json!("Cmd+Shift+P")).unwrap();
        assert!(out.contains("hotkey       = \"Cmd+Shift+P\"        # opens the launcher"));
        let out = set(&out, "style", "font_size", &serde_json::json!(9)).unwrap();
        assert_eq!(Config::parse(&out).unwrap().style.font_size, 9.0);
        assert!(set(&out, "launcher", "opacity", &serde_json::json!(2)).is_err());
        assert!(set(&out, "output", "format", &serde_json::json!("gif")).is_err());
        assert!(set(&out, "output", "copy_format", &serde_json::json!("jpeg")).is_err());
        let out = set(&out, "output", "copy_format", &serde_json::json!("tiff")).unwrap();
        assert_eq!(Config::parse(&out).unwrap().output.copy_format, "tiff");
        assert!(
            set(&out, "output", "size", &serde_json::json!("big")).is_err()
                || Config::parse(&out).is_ok()
        );
        // A key missing from the file is added to its section.
        let out = set("[general]\n", "launcher", "blur", &serde_json::json!(0)).unwrap();
        assert_eq!(Config::parse(&out).unwrap().launcher.blur, 0);
    }
}
