//! The global hotkey that opens the launcher from any app (default `Ctrl+Option+P`).

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::state::AppState;
use tauri::Manager;

/// Register `text` (e.g. "Ctrl+Option+P") as the launcher hotkey, replacing the old one.
/// Problems are added to the launcher's warnings instead of stopping the app.
pub fn apply(app: &AppHandle, text: &str) {
    let state = app.state::<AppState>();
    let mut inner = state.inner.lock().unwrap();
    if inner.hotkey.as_deref() == Some(text) {
        return;
    }
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    inner.warnings.retain(|w| !w.starts_with("Hotkey"));
    match parse(text) {
        Ok(shortcut) => match shortcuts.register(shortcut) {
            Ok(()) => inner.hotkey = Some(text.to_owned()),
            Err(e) => {
                inner.hotkey = None;
                inner.warnings.push(format!(
                    "Hotkey {text} could not be registered ({e}) — another app may use it; change general.hotkey in config.toml"
                ));
            }
        },
        Err(e) => {
            inner.hotkey = None;
            inner
                .warnings
                .push(format!("Hotkey \"{text}\" is not valid: {e}"));
        }
    }
}

/// Read a hotkey like "Ctrl+Option+P" or "Cmd+Shift+Space".
pub fn parse(text: &str) -> Result<Shortcut, String> {
    text.trim().parse::<Shortcut>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_style_hotkeys() {
        for ok in [
            "Ctrl+Option+P",
            "Cmd+Shift+Space",
            "Control+Alt+F12",
            "option+p",
        ] {
            assert!(parse(ok).is_ok(), "{ok}");
        }
        assert!(parse("Ctrl+Banana").is_err());
    }
}
