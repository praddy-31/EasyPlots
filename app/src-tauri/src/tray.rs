//! The menu-bar ("tray") icon. It reacts to clicks only: a click opens the menu.
//! There is deliberately no hover tooltip or hover action.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

use crate::windows;

const OPEN: &str = "open";
const SETTINGS: &str = "settings";
const INSTALL_CLI: &str = "install-cli";
const QUIT: &str = "quit";

pub fn build(app: &tauri::App) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, OPEN, "Open Launcher", true, None::<&str>)?,
            &MenuItem::with_id(app, SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?,
            &MenuItem::with_id(
                app,
                INSTALL_CLI,
                "Install Terminal Command…",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, "Quit EasyPlots", true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        // A "template" icon is recoloured by macOS to suit light and dark menu bars.
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN => windows::show_launcher(app),
            SETTINGS => windows::open_settings(app),
            INSTALL_CLI => crate::install::install_terminal_command(),
            QUIT => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
