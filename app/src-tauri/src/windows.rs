//! Showing and hiding the launcher and settings windows, and the Dock icon switch.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;
use crate::{commands, hotkey};

pub const LAUNCHER: &str = "launcher";
pub const SETTINGS: &str = "settings";

/// Show the launcher on the display with the mouse pointer, with keyboard focus. Picks up
/// config changes (and a changed hotkey) and refreshes the file list first.
pub fn show_launcher(app: &AppHandle) {
    show_launcher_with(app, Vec::new());
}

/// Show the launcher; `dropped` are files dropped on it (the launcher starts afresh with
/// them picked).
pub fn show_launcher_with(app: &AppHandle, dropped: Vec<String>) {
    let key = {
        let state = app.state::<AppState>();
        let mut inner = state.inner.lock().unwrap();
        inner.reload_config_if_changed();
        inner.engine.config.general.hotkey.clone()
    };
    hotkey::apply(app, &key);
    commands::refresh_files(app);
    apply_blur(app);
    let _ = app.emit_to(LAUNCHER, "launcher-shown", dropped);
    #[cfg(target_os = "macos")]
    {
        place_launcher(app, false, true);
        crate::panel::show(app);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window(LAUNCHER) {
        // Show first, then size: resizing a hidden window can leave it blank (seen on Linux).
        let _ = window.show();
        place_launcher(app, false, true);
        let _ = window.set_focus();
    }
}

/// Normal size, or the larger preview size while a plot is shown (`[launcher] size` and
/// `preview_size`).
pub fn size_launcher(app: &AppHandle, preview: bool) {
    place_launcher(app, preview, false);
}

/// Size the launcher and place it (see `place_in`), on the display with the mouse pointer
/// when opening, else on the display it is on.
fn place_launcher(app: &AppHandle, preview: bool, follow_mouse: bool) {
    let (size, anchor_height) = {
        let state = app.state::<AppState>();
        let inner = state.inner.lock().unwrap();
        let c = &inner.engine.config;
        let normal = c.launcher_size().unwrap_or((640.0, 420.0));
        let big = c.preview_size().unwrap_or((900.0, 680.0));
        let size = if preview { big } else { normal };
        // The top edge is where a centred preview's would be, so the preview is centred
        // and the command line doesn't move when it opens (owner, 2026-11-15).
        (size, big.1.max(normal.1))
    };
    #[cfg(target_os = "macos")]
    crate::panel::place(app, size, anchor_height, follow_mouse);
    #[cfg(not(target_os = "macos"))]
    {
        let Some(window) = app.get_webview_window(LAUNCHER) else {
            return;
        };
        let monitor = if follow_mouse {
            app.cursor_position()
                .ok()
                .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        } else {
            None
        };
        let Some(monitor) = monitor.or_else(|| window.current_monitor().ok().flatten()) else {
            let _ = window.set_size(tauri::LogicalSize::new(size.0, size.1));
            let _ = window.center();
            return;
        };
        // Work in points like macOS does; the monitor's pixels are points × scale.
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let (ax, ay) = (
            area.position.x as f64 / scale,
            area.position.y as f64 / scale,
        );
        let (aw, ah) = (
            area.size.width as f64 / scale,
            area.size.height as f64 / scale,
        );
        // place_in counts y from the bottom (AppKit); flip it for a top-down screen.
        let f = place_in((0.0, 0.0, aw, ah), size, anchor_height);
        let top = ah - (f.y + f.h);
        let _ = window.set_size(tauri::LogicalSize::new(f.w, f.h));
        let _ = window.set_position(tauri::PhysicalPosition::new(
            ((ax + f.x) * scale).round() as i32,
            ((ay + top) * scale).round() as i32,
        ));
    }
}

/// Where the launcher goes in a display's usable area `(x, y, width, height)` (y counted
/// from the bottom, as on macOS): centred across; the top edge where a centred window of
/// `anchor_height` (the preview's) would have it. The small launcher then sits above the
/// middle, like Spotlight, the preview is centred, and the command line stays put when
/// the window grows; moved up only as far as a window taller than the anchor needs.
pub fn place_in(area: (f64, f64, f64, f64), size: (f64, f64), anchor_height: f64) -> Frame {
    let (ax, ay, aw, ah) = area;
    let w = size.0.min(aw);
    let h = size.1.min(ah);
    let top_gap = ((ah - anchor_height) / 2.0).min(ah - h).max(0.0);
    Frame {
        x: ax + ((aw - w) / 2.0).max(0.0),
        y: ay + ah - top_gap - h,
        w,
        h,
    }
}

/// A window's place: x, y (from the bottom), width and height, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Blur what's behind the launcher by the strength in `[launcher] blur` or Palette's glass
/// (0 = none; nothing to blur when the window is solid). The colours stay the theme's.
/// Uses the blur radius macOS gives Ghostty (`glass.rs`); if that call is missing, Apple's
/// frosted popover material instead, with a note in the launcher's warnings.
pub fn apply_blur(app: &AppHandle) {
    let look = {
        let state = app.state::<AppState>();
        let mut inner = state.inner.lock().unwrap();
        let look = commands::current_look(&inner);
        inner.blur_applied = Some((look.blur, look.opacity < 1.0));
        look
    };
    let blur = if look.opacity < 1.0 { look.blur } else { 0 };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        use tauri::window::{Effect, EffectState, EffectsBuilder};
        let Some(window) = handle.get_webview_window(LAUNCHER) else {
            return;
        };
        let radius = crate::glass::set_radius(&window, blur);
        let material = (blur > 0 && radius.is_err()).then(|| {
            EffectsBuilder::new()
                .effect(Effect::Popover)
                .state(EffectState::Active)
                .radius(12.0)
                .build()
        });
        let _ = window.set_effects(material);
        if let (Err(why), true) = (radius, blur > 0) {
            let note = format!("Blur: {why}; using the frosted material instead");
            let state = handle.state::<AppState>();
            let mut inner = state.inner.lock().unwrap();
            if !inner.warnings.contains(&note) {
                inner.warnings.push(note);
            }
        }
    });
}

/// Apply hand edits to `config.toml` at once: the hotkey, the blur, the login item, and
/// (through the "config-changed" event) the launcher's look and the Settings window's form.
/// macOS tells us when something in the config folder changes (FSEvents, through `notify`),
/// so nothing runs while the file is left alone. The folder is watched rather than the
/// file because editors save by writing a new file and renaming it over the old one.
pub fn watch_config(app: &AppHandle) {
    use notify::{RecursiveMode, Watcher};
    let path = app
        .state::<AppState>()
        .inner
        .lock()
        .unwrap()
        .config_path
        .clone();
    let Some(dir) = path.parent().map(|d| d.to_path_buf()) else {
        return;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && event
                .paths
                .iter()
                .any(|p| p.file_name() == path.file_name())
        {
            let _ = tx.send(());
        }
    });
    let mut watcher = match watcher {
        Ok(w) => w,
        Err(e) => return warn(app, format!("config.toml changes need a reopen ({e})")),
    };
    if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        return warn(app, format!("config.toml changes need a reopen ({e})"));
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let _keep = watcher; // stops watching when dropped
        while rx.recv().is_ok() {
            // One save can arrive as several events; let them settle, then read once.
            std::thread::sleep(std::time::Duration::from_millis(150));
            while rx.try_recv().is_ok() {}
            let changed = {
                let state = app.state::<AppState>();
                let mut inner = state.inner.lock().unwrap();
                inner.reload_config_if_changed()
            };
            if changed {
                crate::settings::apply(&app);
            }
        }
    });
}

pub fn warn(app: &AppHandle, message: String) {
    let state = app.state::<AppState>();
    state.inner.lock().unwrap().warnings.push(message);
}

/// Keep the launcher open while something of ours (the Save dialog, the colour panel) has
/// the focus. On macOS those only work in the active app, so EasyPlots is made active first.
pub fn hold_open(app: &AppHandle, on: bool) {
    app.state::<AppState>()
        .hold_open
        .store(on, std::sync::atomic::Ordering::SeqCst);
    #[cfg(target_os = "macos")]
    if on {
        crate::panel::activate(app);
    }
}

/// Bring the launcher back to the front after a Save dialog closes.
pub fn refocus_launcher(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    crate::panel::show(app);
    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window(LAUNCHER) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The hotkey opens the launcher, or closes it if it's already in front.
pub fn toggle_launcher(app: &AppHandle) {
    let open = app
        .get_webview_window(LAUNCHER)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
    if open {
        hide_launcher(app.clone());
    } else {
        show_launcher(app);
    }
}

/// File types the launcher can plot (dropping anything else does nothing).
const DATA_FILES: &[&str] = &["csv", "tsv", "txt", "xlsx", "xlsm", "xls", "ods", "json"];

/// Files dragged onto the launcher: while over it, the launcher shows where to drop
/// (`drag-over`); dropped data files start it afresh with them picked (one: the picker's
/// next step; several: overlaid). Replaces dropping on the menu-bar icon, which macOS
/// didn't pass on reliably.
pub fn files_dragged(app: &AppHandle, drag: &tauri::DragDropEvent) {
    use tauri::DragDropEvent;
    match drag {
        DragDropEvent::Enter { .. } | DragDropEvent::Over { .. } => {
            let _ = app.emit_to(LAUNCHER, "drag-over", true);
        }
        DragDropEvent::Leave => {
            let _ = app.emit_to(LAUNCHER, "drag-over", false);
        }
        DragDropEvent::Drop { paths, .. } => {
            let _ = app.emit_to(LAUNCHER, "drag-over", false);
            let files: Vec<String> = paths
                .iter()
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| DATA_FILES.iter().any(|d| e.eq_ignore_ascii_case(d)))
                })
                .map(|p| crate::settings::tilde(p))
                .collect();
            if files.is_empty() {
                return;
            }
            let _ = app.emit_to(LAUNCHER, "launcher-shown", files);
            refocus_launcher(app);
        }
        _ => {}
    }
}

/// Hide the launcher. Called by the launcher itself when Esc is pressed.
#[tauri::command]
pub fn hide_launcher(app: AppHandle) {
    // Closing always unpins (⌘P).
    app.state::<AppState>()
        .pinned
        .store(false, std::sync::atomic::Ordering::SeqCst);
    if let Some(window) = app.get_webview_window(LAUNCHER) {
        let _ = window.hide();
    }
    // With Settings closed, give the keyboard back to the app that was in front, in case
    // something (the Save dialog) made EasyPlots the active app.
    #[cfg(target_os = "macos")]
    if app.get_webview_window(SETTINGS).is_none() {
        crate::panel::give_back_focus(&app);
    }
}

/// Open the settings window (or bring it to the front) and give the app a Dock icon.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    set_dock_icon(app, true);
    let built =
        WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings/index.html".into()))
            .title("EasyPlots Settings")
            .inner_size(720.0, 560.0)
            .min_inner_size(560.0, 400.0)
            .center()
            .focused(true)
            .build();
    if built.is_err() {
        set_dock_icon(app, false);
    }
}

/// Switch between menu-bar-only (no Dock icon) and a normal app with a Dock icon.
/// Only macOS has this distinction; elsewhere it does nothing.
pub fn set_dock_icon(app: &AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let policy = if visible {
            ActivationPolicy::Regular
        } else {
            ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
        if visible {
            // Bring the app (and so the new window) to the front.
            let _ = app.show();
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_is_centred_and_keeps_the_launchers_top_edge() {
        // A 1440×875 usable area (a laptop), y from the bottom; the launcher is 640×420,
        // the preview 900×680 (owner, 2026-11-15: the preview hung down to the bottom).
        let area = (0.0, 0.0, 1440.0, 875.0);
        let normal = place_in(area, (640.0, 420.0), 680.0);
        assert_eq!((normal.x, normal.w, normal.h), (400.0, 640.0, 420.0));
        let normal_top = normal.y + normal.h;
        assert_eq!(normal_top, 875.0 - 97.5);
        // The preview hangs from the same top edge, centred both ways.
        let preview = place_in(area, (900.0, 680.0), 680.0);
        assert_eq!(preview.y + preview.h, normal_top);
        assert_eq!((preview.x, preview.y), (270.0, 97.5));
        // A window taller than the anchor moves up just enough.
        let tall = place_in((0.0, 0.0, 1440.0, 700.0), (900.0, 680.0), 600.0);
        assert_eq!((tall.y, tall.y + tall.h), (0.0, 680.0));
        // On a second display to the right (its own origin), and never bigger than it.
        let other = place_in((1440.0, 100.0, 800.0, 500.0), (900.0, 680.0), 420.0);
        assert_eq!(
            (other.x, other.y, other.w, other.h),
            (1440.0, 100.0, 800.0, 500.0)
        );
    }
}
