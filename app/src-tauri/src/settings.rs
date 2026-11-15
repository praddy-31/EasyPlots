//! What the Settings window may call: read every option, change one (written into
//! `config.toml` with its comments kept), pick a folder, manage presets, and the parts of
//! the config the app applies itself (hotkey, blur, launch at login).

use std::path::Path;

use easyplots_core::Error;
use easyplots_core::config::expand_tilde;
use easyplots_core::history::Preset;
use easyplots_core::settings::{self, Field};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::state::AppState;
use crate::{hotkey, windows};

#[derive(Serialize)]
pub struct SettingsInfo {
    fields: Vec<Field>,
    values: serde_json::Value,
    config_path: String,
    presets: Vec<Preset>,
    styles: Vec<String>,
    warnings: Vec<String>,
    version: &'static str,
    /// The launcher's opacity and blur now, and whether Palette sets them.
    look: easyplots_core::ui_theme::Look,
}

#[tauri::command]
pub fn settings_info(state: State<AppState>) -> SettingsInfo {
    let mut inner = state.inner.lock().unwrap();
    inner.reload_config_if_changed();
    SettingsInfo {
        fields: settings::fields(),
        values: settings::values(&inner.engine.config),
        config_path: tilde(&inner.config_path),
        presets: inner.history.presets().to_vec(),
        styles: inner.engine.style_names(),
        warnings: inner.warnings.clone(),
        version: easyplots_core::VERSION,
        look: crate::commands::current_look(&inner),
    }
}

/// Change one option: write it into `config.toml`, load it, and apply it at once.
#[tauri::command]
pub fn set_setting(
    section: String,
    key: String,
    value: serde_json::Value,
    app: AppHandle,
    state: State<AppState>,
) -> Result<serde_json::Value, String> {
    let values = {
        let mut inner = state.inner.lock().unwrap();
        let path = inner.config_path.clone();
        let text = std::fs::read_to_string(&path)
            .or_else(|_| Ok::<_, std::io::Error>(easyplots_core::config::DEFAULT_TOML.to_owned()))
            .map_err(|e| e.to_string())?;
        let new = settings::set(&text, &section, &key, &value).map_err(|e| e.message)?;
        write_atomically(&path, &new).map_err(|e| e.message)?;
        inner.reload_config_if_changed();
        settings::values(&inner.engine.config)
    };
    apply(&app);
    Ok(values)
}

/// Write via a temporary file and a rename, so the file is never half-written.
fn write_atomically(path: &Path, text: &str) -> Result<(), Error> {
    let tmp = path.with_extension("toml.tmp");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| Error::new(format!("cannot create {}: {e}", dir.display())))?;
    }
    std::fs::write(&tmp, text)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| Error::new(format!("cannot write {}: {e}", path.display())))
}

/// Apply what the app itself is responsible for: hotkey, blur, launch at login, and tell
/// the launcher to repaint.
pub fn apply(app: &AppHandle) {
    let (key, login) = {
        let state = app.state::<AppState>();
        let inner = state.inner.lock().unwrap();
        let c = &inner.engine.config;
        (c.general.hotkey.clone(), c.general.launch_at_login)
    };
    hotkey::apply(app, &key);
    windows::apply_blur(app);
    apply_login_item(app, login);
    let _ = app.emit("config-changed", ());
}

/// Add or remove the login item to match `launch_at_login`.
pub fn apply_login_item(app: &AppHandle, on: bool) {
    let manager = app.autolaunch();
    let enabled = manager.is_enabled().unwrap_or(false);
    let result = match (on, enabled) {
        (true, false) => manager.enable(),
        (false, true) => manager.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        let state = app.state::<AppState>();
        let mut inner = state.inner.lock().unwrap();
        inner.warnings.retain(|w| !w.starts_with("Launch at login"));
        inner
            .warnings
            .push(format!("Launch at login could not be changed: {e}"));
    }
}

/// Choose a folder for `data_dir` or `output.dir` with the macOS folder dialog.
#[tauri::command]
pub async fn pick_folder(
    section: String,
    key: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    use tauri_plugin_dialog::DialogExt;
    let start = {
        let inner = state.inner.lock().unwrap();
        let c = &inner.engine.config;
        expand_tilde(match (section.as_str(), key.as_str()) {
            ("general", _) => &c.general.data_dir,
            (_, "paste_dir") => &c.output.paste_dir,
            _ => &c.output.dir,
        })
    };
    let picked = app
        .dialog()
        .file()
        .set_directory(start)
        .blocking_pick_folder();
    let Some(folder) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    set_setting(
        section,
        key,
        serde_json::Value::String(tilde(&folder)),
        app.clone(),
        state,
    )
    .map(Some)
}

/// Open `config.toml` in the default text editor.
#[tauri::command]
pub fn open_config_file(state: State<AppState>) {
    let path = state.inner.lock().unwrap().config_path.clone();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
}

/// The fonts installed on this Mac, for the launcher's font box (monospaced first).
// `async` so reading the font files doesn't hold up the window.
#[tauri::command]
pub async fn installed_fonts() -> Vec<settings::FontFamily> {
    settings::installed_fonts()
}

#[tauri::command]
pub fn delete_preset(name: String, state: State<AppState>) -> Result<Vec<Preset>, String> {
    let mut inner = state.inner.lock().unwrap();
    inner.history.delete_preset(&name).map_err(|e| e.message)?;
    Ok(inner.history.presets().to_vec())
}

#[tauri::command]
pub fn delete_style(
    name: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<String>, String> {
    let inner = state.inner.lock().unwrap();
    inner.engine.delete_style(&name).map_err(|e| e.message)?;
    let names = inner.engine.style_names();
    drop(inner);
    let _ = app.emit("config-changed", ());
    Ok(names)
}

/// One Try-it lesson, as the Settings tab shows it.
#[derive(Serialize)]
pub struct Lesson {
    section: &'static str,
    title: &'static str,
    text: &'static str,
    command: &'static str,
}

/// The Try-it guide's lessons (round 8c). They live in core (`guide.rs`), where
/// `cargo test` draws every one, so the tab can't go stale.
#[tauri::command]
pub fn guide_lessons() -> Vec<Lesson> {
    easyplots_core::guide::lessons()
        .into_iter()
        .map(|l| Lesson {
            section: l.section,
            title: l.title,
            text: l.text,
            command: l.command,
        })
        .collect()
}

/// Draw one lesson's picture (SVG text; the sample data is built into the app).
#[tauri::command]
pub fn guide_preview(command: String, state: State<AppState>) -> Result<String, String> {
    let mut inner = state.inner.lock().unwrap();
    inner
        .engine
        .render(&command, None)
        .map(|r| r.svg)
        .map_err(|e| e.message)
}

/// A lesson's "Open in launcher": show the launcher with the command filled in.
#[tauri::command]
pub fn try_lesson(command: String, app: AppHandle) {
    windows::show_launcher(&app);
    let _ = app.emit_to(windows::LAUNCHER, "launcher-command", command);
}

/// `~/…` for paths in the home folder.
pub fn tilde(path: &Path) -> String {
    let home = easyplots_core::config::home();
    match path.strip_prefix(&home) {
        Ok(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        Ok(_) => "~".to_owned(),
        Err(_) => path.display().to_string(),
    }
}

/// At start-up: make the login item match the config.
pub fn startup(app: &AppHandle) {
    let login = app
        .state::<AppState>()
        .inner
        .lock()
        .unwrap()
        .engine
        .config
        .general
        .launch_at_login;
    apply_login_item(app, login);
}
