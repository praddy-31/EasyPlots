//! `config.toml`: the owner's settings (see `docs/SPEC.md` §2). Every key is optional;
//! anything missing uses the default below.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::spec::Format;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub output: Output,
    pub style: StyleConfig,
    pub launcher: Launcher,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub hotkey: String,
    pub data_dir: String,
    pub search_whole_disk: bool,
    pub launch_at_login: bool,
    /// Which JSON files the file list shows: `all`, `tables` (only those holding rows) or
    /// `known` (all but well-known settings files such as `package.json`).
    pub json_files: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Output {
    pub dir: String,
    /// Where tables pasted from the clipboard are kept (so plots of them can be redrawn).
    pub paste_dir: String,
    pub format: String,
    /// What ⌘C puts on the clipboard: "png" (small, as a screenshot does) or "tiff".
    pub copy_format: String,
    pub name: String,
    pub dpi: f64,
    pub size: String,
    pub after_save: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct StyleConfig {
    pub theme: String,
    pub font: String,
    pub font_size: f64,
    pub palette: String,
    pub line_width: f64,
    pub grid: bool,
    pub legend: String,
}

/// Below this launcher opacity text gets hard to read; Settings warns (no hard limit).
pub const READABLE_OPACITY: f64 = 0.4;

/// `blur` as a number 0–100, or `true` / `false` (20 / 0) as older files have it.
fn blur_amount<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Blur {
        On(bool),
        Amount(i64),
    }
    match Blur::deserialize(d) {
        Ok(Blur::On(on)) => Ok(if on { 20 } else { 0 }),
        Ok(Blur::Amount(n)) => u32::try_from(n)
            .map_err(|_| serde::de::Error::custom("launcher.blur must be between 0 and 100")),
        Err(_) => Err(serde::de::Error::custom(
            "launcher.blur is a number from 0 (off) to 100",
        )),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Launcher {
    pub mode: String,
    pub history_size: usize,
    /// "palette", "auto" or a built-in theme name (see `ui_theme`).
    pub theme: String,
    /// Background opacity, 0–1. No lower limit: Settings warns below `READABLE_OPACITY`.
    pub opacity: f64,
    /// How strongly what's behind the window is blurred, 0 (off) to 100, like Ghostty's
    /// `background-blur` (macOS). `true` / `false` from older files mean 20 / 0.
    #[serde(deserialize_with = "blur_amount")]
    pub blur: u32,
    pub font: String,
    pub font_size: f64,
    /// Key hints at the bottom.
    pub hints: bool,
    /// Window size in points, e.g. "640x420".
    pub size: String,
    /// Window size while showing a preview.
    pub preview_size: String,
    /// Draw a small preview while a command is typed.
    pub live_preview: bool,
    /// The plot advisor: a hint under the preview when a known pitfall shows up.
    pub advice: bool,
}

impl Default for General {
    fn default() -> Self {
        General {
            hotkey: "Ctrl+Option+P".into(),
            data_dir: "~/Documents".into(),
            search_whole_disk: false,
            launch_at_login: true,
            json_files: "all".into(),
        }
    }
}

impl Default for Output {
    fn default() -> Self {
        Output {
            dir: "~/Plots".into(),
            paste_dir: "~/Plots/pasted".into(),
            format: "png".into(),
            copy_format: "png".into(),
            name: "{file}_{type}_{date}-{time}".into(),
            dpi: 300.0,
            size: "6x4in".into(),
            after_save: "thumbnail".into(),
        }
    }
}

impl Default for StyleConfig {
    fn default() -> Self {
        StyleConfig {
            // Screen look by default (owner, 2026-11-12: all their figures use it);
            // `paper` is one setting away for journals.
            theme: "light".into(),
            font: "auto".into(),
            font_size: 10.0,
            palette: "okabe-ito".into(),
            line_width: 1.25,
            grid: false,
            legend: "best".into(),
        }
    }
}

impl Default for Launcher {
    fn default() -> Self {
        Launcher {
            mode: "picker".into(),
            history_size: 200,
            theme: "palette".into(),
            opacity: 0.9,
            blur: 20,
            font: "JetBrains Mono".into(),
            font_size: 15.0,
            hints: true,
            size: "640x420".into(),
            preview_size: "900x680".into(),
            live_preview: false,
            advice: true,
        }
    }
}

/// The file written on first run: every setting with its default and a short comment.
pub const DEFAULT_TOML: &str = r#"# EasyPlots settings. Delete a line to go back to its default.
# The full description is in docs/SPEC.md of the EasyPlots repository.

[general]
hotkey       = "Ctrl+Option+P"      # opens the launcher from any app
data_dir     = "~/Documents"        # where the file picker looks
search_whole_disk = false           # also list CSVs from everywhere (Spotlight; names only)
launch_at_login   = true            # start EasyPlots when you log in
json_files   = "all"                # JSON files in the list: all | tables (only those with rows) | known (not package.json etc.)

[output]
dir      = "~/Plots"                # where plots are saved
paste_dir = "~/Plots/pasted"        # where tables pasted from the clipboard are kept
format   = "png"                    # png | pdf | svg
copy_format = "png"                 # what ⌘C copies: png | tiff
name     = "{file}_{type}_{date}-{time}"   # also {files} {y} {n}
dpi      = 300                      # PNG only
size     = "6x4in"                  # also cm, mm, pt
after_save = "thumbnail"            # thumbnail | notify | open | reveal (in Finder) | none

[style]
theme    = "light"                  # light (screen, slides) | paper (journal) | dark | palette
font     = "auto"                   # auto (the theme's own) | STIX Two | Latin Modern | Source Sans
font_size = 10                      # points
palette  = "okabe-ito"
line_width = 1.25                   # points
grid     = false
legend   = "best"                   # best | tr | tl | br | bl | out | off

[launcher]
mode     = "picker"                 # picker | command: which mode opens first
history_size = 200
theme    = "palette"                # palette | auto | catppuccin-mocha | rose-pine-dawn | gruvbox-dark | tokyo-night | everforest | kanagawa
opacity  = 0.9                      # background opacity, 0–1 (below 0.4 text gets hard to read)
blur     = 20                       # blur what's behind the window, 0 (off) to 100
font     = "JetBrains Mono"         # any installed font; JetBrains Mono is bundled
font_size = 15
hints    = true                     # key hints at the bottom
size     = "640x420"                # window size
preview_size = "900x680"            # window size while showing a plot
live_preview = false                # draw a small preview while typing a command
advice   = true                     # a hint under the preview when a plot has a known pitfall
"#;

impl Config {
    /// Write `DEFAULT_TOML` to `path` unless a file is already there. Returns whether it wrote.
    pub fn write_default_if_missing(path: &Path) -> Result<bool, Error> {
        if path.exists() {
            return Ok(false);
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Error::new(format!("cannot create {}: {e}", dir.display())))?;
        }
        std::fs::write(path, DEFAULT_TOML)
            .map_err(|e| Error::new(format!("cannot write {}: {e}", path.display())))?;
        Ok(true)
    }

    /// `history.toml` next to the config file.
    pub fn history_path(config_path: &Path) -> PathBuf {
        config_path.with_file_name("history.toml")
    }

    pub fn after_save(&self) -> Result<AfterSave, Error> {
        match self.output.after_save.as_str() {
            "thumbnail" => Ok(AfterSave::Thumbnail),
            "notify" => Ok(AfterSave::Notify),
            "open" => Ok(AfterSave::Open),
            "reveal" => Ok(AfterSave::Reveal),
            "none" => Ok(AfterSave::None),
            other => Err(Error::new(format!(
                "output.after_save \"{other}\" must be thumbnail, notify, open, reveal or none"
            ))),
        }
    }

    /// `$EASYPLOTS_CONFIG` if set, otherwise `~/.config/easyplots/config.toml`.
    pub fn default_path() -> PathBuf {
        if let Some(p) = std::env::var_os("EASYPLOTS_CONFIG") {
            return PathBuf::from(p);
        }
        home().join(".config/easyplots/config.toml")
    }

    /// Read the config file. A missing file means "all defaults"; it is not created here.
    pub fn load(path: &Path) -> Result<Config, Error> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(&text)
                .map_err(|e| Error::new(format!("{}: {}", path.display(), e.message))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(Error::new(format!("cannot read {}: {e}", path.display()))),
        }
    }

    pub fn parse(text: &str) -> Result<Config, Error> {
        let config: Config = toml::from_str(text).map_err(|e| Error::new(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), Error> {
        self.format()?;
        self.after_save()?;
        if crate::search::JsonFiles::from_name(&self.general.json_files).is_none() {
            return Err(Error::new(format!(
                "general.json_files \"{}\" must be all, tables or known",
                self.general.json_files
            )));
        }
        if !matches!(self.output.copy_format.as_str(), "png" | "tiff") {
            return Err(Error::new(format!(
                "output.copy_format \"{}\" must be png or tiff",
                self.output.copy_format
            )));
        }
        if !(30.0..=2400.0).contains(&self.output.dpi) {
            return Err(Error::new("output.dpi must be between 30 and 2400"));
        }
        if self.style.line_width <= 0.0 {
            return Err(Error::new("style.line_width must be more than 0"));
        }
        if self.output.name.trim().is_empty() {
            return Err(Error::new("output.name must not be empty"));
        }
        if !(0.0..=1.0).contains(&self.launcher.opacity) {
            return Err(Error::new("launcher.opacity must be between 0 and 1"));
        }
        if self.launcher.blur > 100 {
            return Err(Error::new("launcher.blur must be between 0 and 100"));
        }
        if !(9.0..=32.0).contains(&self.launcher.font_size) {
            return Err(Error::new("launcher.font_size must be between 9 and 32"));
        }
        let theme = self.launcher.theme.to_ascii_lowercase();
        if theme != "palette" && theme != "auto" && crate::ui_theme::builtin(&theme).is_none() {
            let names: Vec<&str> = crate::ui_theme::builtin_names().collect();
            return Err(Error::new(format!(
                "launcher.theme \"{}\" must be palette, auto or one of: {}",
                self.launcher.theme,
                names.join(", ")
            )));
        }
        self.launcher_size()?;
        self.preview_size()?;
        crate::command::size(&self.output.size, &(0..0))
            .map_err(|e| Error::new(format!("output.size: {}", e.message)))?;
        crate::spec::LegendPos::from_name(&self.style.legend).ok_or_else(|| {
            Error::new(format!(
                "style.legend \"{}\" must be best, tr, tl, br, bl, out or off",
                self.style.legend
            ))
        })?;
        crate::style::Style::from_settings(
            &self.style.theme,
            &self.style.font,
            self.style.font_size,
            self.style.line_width,
            self.style.grid,
            std::path::Path::new(""),
        )
        .map_err(|e| Error::new(format!("style: {}", e.message)))?;
        Ok(())
    }

    /// `launcher.size` as (width, height).
    pub fn launcher_size(&self) -> Result<(f64, f64), Error> {
        window_size(&self.launcher.size, "launcher.size")
    }

    pub fn preview_size(&self) -> Result<(f64, f64), Error> {
        window_size(&self.launcher.preview_size, "launcher.preview_size")
    }

    pub fn format(&self) -> Result<Format, Error> {
        Format::from_name(&self.output.format).ok_or_else(|| {
            Error::new(format!(
                "output.format \"{}\" must be png, pdf or svg",
                self.output.format
            ))
        })
    }

    pub fn output_dir(&self) -> PathBuf {
        expand_tilde(&self.output.dir)
    }

    pub fn paste_dir(&self) -> PathBuf {
        expand_tilde(&self.output.paste_dir)
    }

    /// Which JSON files the file list shows (`general.json_files`).
    pub fn json_files(&self) -> crate::search::JsonFiles {
        crate::search::JsonFiles::from_name(&self.general.json_files).unwrap_or_default()
    }

    pub fn data_dir(&self) -> PathBuf {
        expand_tilde(&self.general.data_dir)
    }
}

/// What the launcher does after saving a plot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AfterSave {
    Thumbnail,
    Notify,
    Open,
    /// Show the saved file selected in a Finder window.
    Reveal,
    None,
}

fn window_size(text: &str, key: &str) -> Result<(f64, f64), Error> {
    let bad = || {
        Error::new(format!(
            "{key} \"{text}\" must look like 640x420 (300–3000 each)"
        ))
    };
    let (w, h) = text.trim().split_once(['x', '×']).ok_or_else(bad)?;
    let w: f64 = w.trim().parse().map_err(|_| bad())?;
    let h: f64 = h.trim().parse().map_err(|_| bad())?;
    if !(300.0..=3000.0).contains(&w) || !(200.0..=3000.0).contains(&h) {
        return Err(bad());
    }
    Ok((w, h))
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Turn `~/x` into `/Users/me/x`.
pub fn expand_tilde(p: &str) -> PathBuf {
    if p == "~" {
        home()
    } else if let Some(rest) = p.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let c = Config::parse("[output]\nformat = \"pdf\"\n").unwrap();
        assert_eq!(c.format().unwrap(), Format::Pdf);
        assert_eq!(c.output.dpi, 300.0);
        assert_eq!(c.style.font, "auto");
    }

    #[test]
    fn spec_example_parses() {
        // The example from docs/SPEC.md §2, so the doc and the code can't drift apart.
        let spec = include_str!("../../../docs/SPEC.md");
        let section = spec.find("## 2. `config.toml`").unwrap();
        let start = section + spec[section..].find("```toml\n").unwrap() + 8;
        let end = start + spec[start..].find("```").unwrap();
        assert_eq!(Config::parse(&spec[start..end]).unwrap(), Config::default());
    }

    #[test]
    fn default_file_matches_defaults() {
        assert_eq!(Config::parse(DEFAULT_TOML).unwrap(), Config::default());
    }

    #[test]
    fn writes_default_once() {
        let dir = std::env::temp_dir().join(format!("easyplots-config-{}", std::process::id()));
        let path = dir.join("sub/config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(Config::write_default_if_missing(&path).unwrap());
        std::fs::write(&path, "[output]\nformat = \"pdf\"\n").unwrap();
        assert!(!Config::write_default_if_missing(&path).unwrap());
        assert_eq!(Config::load(&path).unwrap().format().unwrap(), Format::Pdf);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn launcher_settings_are_checked() {
        assert!(Config::parse("[launcher]\nopacity = 1.5\n").is_err());
        // No floor for readability (Settings warns instead), and blur as on/off or a number.
        assert_eq!(
            Config::parse("[launcher]\nopacity = 0.1\n")
                .unwrap()
                .launcher
                .opacity,
            0.1
        );
        assert_eq!(
            Config::parse("[launcher]\nblur = true\n")
                .unwrap()
                .launcher
                .blur,
            20
        );
        assert_eq!(
            Config::parse("[launcher]\nblur = false\n")
                .unwrap()
                .launcher
                .blur,
            0
        );
        assert_eq!(
            Config::parse("[launcher]\nblur = 45\n")
                .unwrap()
                .launcher
                .blur,
            45
        );
        assert!(Config::parse("[launcher]\nblur = 101\n").is_err());
        assert!(Config::parse("[launcher]\nblur = \"lots\"\n").is_err());
        assert!(Config::parse("[launcher]\ntheme = \"nope\"\n").is_err());
        assert!(Config::parse("[launcher]\nsize = \"big\"\n").is_err());
        let c =
            Config::parse("[launcher]\ntheme = \"gruvbox-dark\"\nsize = \"700x400\"\n").unwrap();
        assert_eq!(c.launcher_size().unwrap(), (700.0, 400.0));
    }

    #[test]
    fn typos_are_reported() {
        let e = Config::parse("[output]\nfromat = \"pdf\"\n").unwrap_err();
        assert!(e.message.contains("fromat"), "{}", e.message);
        assert!(Config::parse("[output]\nformat = \"gif\"\n").is_err());
    }
}
