//! The EasyPlots macOS app: a menu-bar icon, the launcher window and the settings window.
//!
//! The app normally runs as an *accessory* (menu bar only, no Dock icon). While Settings
//! is open it becomes a *regular* app with a Dock icon, and goes back when Settings closes.
//! All plotting work is done by `easyplots-core`; this crate only handles the macOS side.
//!
//! Started under the name `easyplots` (through the link that "Install Terminal Command…"
//! creates), the program behaves as the terminal command instead and opens no windows.

mod commands;
mod glass;
mod hotkey;
mod install;
#[cfg(target_os = "macos")]
mod panel;
mod settings;
mod state;
mod tray;
mod windows;

use std::path::Path;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::state::AppState;

fn main() {
    let mut args = std::env::args();
    let argv0 = args.next().unwrap_or_default();
    if Path::new(&argv0)
        .file_name()
        .is_some_and(|name| name == "easyplots")
    {
        let rest: Vec<String> = args.collect();
        std::process::exit(easyplots_cli::run(&rest));
    }

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    let app = builder
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        windows::toggle_launcher(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            windows::hide_launcher,
            commands::launcher_info,
            commands::search_files,
            commands::list_columns,
            commands::peek_file,
            commands::complete_command,
            commands::recall_plot,
            commands::preview_thumb,
            commands::hold_open,
            commands::set_pinned,
            commands::save_preset,
            commands::save_style,
            commands::clipboard_table,
            commands::recent_plots,
            commands::plot_thumb,
            commands::grow_launcher,
            commands::save_clipboard,
            commands::apply_preset,
            settings::settings_info,
            settings::set_setting,
            settings::pick_folder,
            settings::open_config_file,
            settings::delete_preset,
            settings::delete_style,
            settings::installed_fonts,
            settings::guide_lessons,
            settings::guide_preview,
            settings::try_lesson,
            commands::current_theme,
            commands::preview,
            commands::end_preview,
            commands::save_preview,
            commands::save_preview_as,
            commands::copy_preview,
            commands::copy_preview_data,
            commands::open_plot,
        ])
        .setup(|app| {
            windows::set_dock_icon(app.handle(), false);
            #[cfg(target_os = "macos")]
            panel::setup(app.handle());
            tray::build(app)?;
            let key = app
                .state::<AppState>()
                .inner
                .lock()
                .unwrap()
                .engine
                .config
                .general
                .hotkey
                .clone();
            hotkey::apply(app.handle(), &key);
            commands::refresh_files(app.handle());
            windows::watch_config(app.handle());
            settings::startup(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| match (window.label(), event) {
            // Clicking anywhere else dismisses the launcher, like Spotlight or fuzzel.
            // (Not while a Save dialog is open on top of it, nor while pinned with ⌘P.)
            (windows::LAUNCHER, WindowEvent::Focused(focused)) => {
                let state = window.state::<AppState>();
                let held = state.hold_open.load(std::sync::atomic::Ordering::SeqCst)
                    || state.pinned.load(std::sync::atomic::Ordering::SeqCst);
                if !focused && !held {
                    let _ = window.hide();
                }
            }
            // Files dragged onto the launcher (pinned with ⌘P so it stays open meanwhile).
            (windows::LAUNCHER, WindowEvent::DragDrop(drag)) => {
                windows::files_dragged(window.app_handle(), drag);
            }
            (windows::SETTINGS, WindowEvent::Destroyed) => {
                windows::set_dock_icon(window.app_handle(), false);
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("EasyPlots failed to start");

    app.run(|app, event| {
        #[cfg(not(target_os = "macos"))]
        let _ = app;
        match event {
            // Closing the last window must not quit a menu-bar app; only "Quit" does
            // (it calls `exit`, which sets an exit code).
            RunEvent::ExitRequested {
                api, code: None, ..
            } => api.prevent_exit(),
            // Opening EasyPlots.app again (Finder, Launchpad, Dock) shows Settings.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => windows::open_settings(app),
            _ => {}
        }
    });
}
