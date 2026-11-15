//! What the app keeps between launcher openings: the engine (config + CSV cache), the file
//! index, the command history, and any warnings to show in the launcher.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::{Instant, SystemTime};

use easyplots_core::history::History;
use easyplots_core::search::FileEntry;
use easyplots_core::{Config, Engine, Rendered};

pub struct AppState {
    pub inner: Mutex<Inner>,
    /// True while a Save dialog is open, so losing focus doesn't hide the launcher.
    pub hold_open: AtomicBool,
    /// Pinned with ⌘P: stays open when another app is clicked (to drag files onto it).
    pub pinned: AtomicBool,
}

pub struct Inner {
    pub config_path: PathBuf,
    /// When `config.toml` was last read, to notice hand edits.
    pub config_modified: Option<SystemTime>,
    pub engine: Engine,
    pub history: History,
    pub files: Vec<FileEntry>,
    pub files_scanned: Option<Instant>,
    pub scanning: bool,
    /// Problems to show at the bottom of the launcher (bad config, hotkey taken…).
    pub warnings: Vec<String>,
    pub hotkey: Option<String>,
    /// The plot being previewed, so ⌘S / Save As… / ⌘C don't draw it again.
    pub preview: Option<Rendered>,
    /// Where the last plot was saved (may be outside the output folder, via Save As…).
    pub last_saved: Option<PathBuf>,
    /// The blur last put on the launcher window (strength, see-through), so it is only
    /// re-applied when it changes.
    pub blur_applied: Option<(u32, bool)>,
}

impl AppState {
    /// Create `config.toml` if it's missing, then load it.
    pub fn new() -> AppState {
        let config_path = Config::default_path();
        let mut warnings = Vec::new();
        if let Err(e) = Config::write_default_if_missing(&config_path) {
            warnings.push(e.message);
        }
        let (config, modified) = read_config(&config_path, &mut warnings);
        let history = History::load(
            &Config::history_path(&config_path),
            config.launcher.history_size,
        );
        AppState {
            inner: Mutex::new(Inner {
                config_path,
                config_modified: modified,
                engine: Engine::new(config),
                history,
                files: Vec::new(),
                files_scanned: None,
                scanning: false,
                warnings,
                hotkey: None,
                preview: None,
                last_saved: None,
                blur_applied: None,
            }),
            hold_open: AtomicBool::new(false),
            pinned: AtomicBool::new(false),
        }
    }
}

fn read_config(path: &PathBuf, warnings: &mut Vec<String>) -> (Config, Option<SystemTime>) {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    match Config::load(path) {
        Ok(c) => (c, modified),
        Err(e) => {
            warnings.push(format!("{} — using defaults", e.message));
            (Config::default(), modified)
        }
    }
}

impl Inner {
    /// Re-read `config.toml` if it changed on disk. Returns true if it was reloaded.
    pub fn reload_config_if_changed(&mut self) -> bool {
        let modified = std::fs::metadata(&self.config_path)
            .and_then(|m| m.modified())
            .ok();
        if modified == self.config_modified {
            return false;
        }
        self.warnings.retain(|w| !w.contains("config.toml"));
        let mut warnings = Vec::new();
        let (config, modified) = read_config(&self.config_path, &mut warnings);
        self.warnings.extend(warnings);
        let data_dir_changed = config.data_dir() != self.engine.config.data_dir();
        self.engine.config = config;
        self.config_modified = modified;
        if data_dir_changed {
            self.files.clear();
            self.files_scanned = None;
        }
        true
    }
}
