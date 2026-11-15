//! macOS only: the launcher as a *panel*, the kind of window Spotlight and Raycast use.
//!
//! A normal window makes the whole app active when it takes the keyboard. Window managers
//! such as AeroSpace react to that: they jump to the workspace (or display) where the app
//! last had a window, and with Settings open they jump to Settings. A *non-activating panel*
//! takes the keyboard without making EasyPlots the active app, so the app you were in stays
//! in front and nothing else moves. The panel is also placed on the display with the mouse
//! pointer, because a hidden window otherwise reopens on whichever display it was last on.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSScreen, NSView, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use tauri::{AppHandle, Manager};
use tauri_nspanel::{CollectionBehavior, ManagerExt, WebviewWindowExt};

use crate::windows::LAUNCHER;

use self::class::LauncherPanel;

/// The panel class (in its own module: the macro brings its own `use` lines).
mod class {
    tauri_nspanel::tauri_panel! {
        panel!(LauncherPanel {
            config: {
                can_become_key_window: true,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }
}

/// Turn the launcher window into a non-activating panel (once, at start-up).
pub fn setup(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LAUNCHER) else {
        return;
    };
    let panel = match window.to_panel::<LauncherPanel>() {
        Ok(p) => p,
        Err(e) => return crate::windows::warn(app, format!("launcher panel: {e}")),
    };
    if let Err(e) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
        crate::windows::warn(app, format!("launcher panel: {e}"));
    }
    // Above normal windows; joins whichever Space is active, also next to full-screen apps.
    panel.set_level(tauri_nspanel::PanelLevel::Floating.value());
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .move_to_active_space()
            .full_screen_auxiliary()
            .ignores_cycle()
            .value(),
    );
    panel.set_hides_on_deactivate(false);
}

/// Size the launcher and place it: on the display under the mouse pointer when `follow_mouse`
/// (opening), else on the display it is on (growing for a preview and back).
pub fn place(app: &AppHandle, size: (f64, f64), anchor_height: f64, follow_mouse: bool) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(panel) = app2.get_webview_panel(LAUNCHER) else {
            return;
        };
        let window = panel.as_panel();
        let screen = if follow_mouse {
            screen_under_mouse(mtm)
        } else {
            window.screen().or_else(|| screen_under_mouse(mtm))
        };
        let Some(screen) = screen else {
            return;
        };
        let v = screen.visibleFrame();
        let f = crate::windows::place_in(
            (v.origin.x, v.origin.y, v.size.width, v.size.height),
            size,
            anchor_height,
        );
        window.setFrame_display(
            NSRect::new(NSPoint::new(f.x, f.y), NSSize::new(f.w, f.h)),
            true,
        );
    });
}

fn screen_under_mouse(mtm: MainThreadMarker) -> Option<objc2::rc::Retained<NSScreen>> {
    let mouse = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    screens
        .iter()
        .find(|s| {
            let f = s.frame();
            mouse.x >= f.origin.x
                && mouse.x < f.origin.x + f.size.width
                && mouse.y >= f.origin.y
                && mouse.y < f.origin.y + f.size.height
        })
        .or_else(|| NSScreen::mainScreen(mtm))
}

/// Show the launcher in front with the keyboard, without making EasyPlots the active app.
pub fn show(app: &AppHandle) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        // After a hide (below), our windows stay hidden until the app is un-hidden.
        let ns_app = NSApplication::sharedApplication(mtm);
        if ns_app.isHidden() {
            ns_app.unhideWithoutActivation();
        }
        let Ok(panel) = app2.get_webview_panel(LAUNCHER) else {
            return;
        };
        panel.order_front_regardless();
        panel.make_key_window();
        // The keyboard must go to the web view (the page with the text box), not to the
        // window's outer view: `show_and_make_key` picks the outer one, so the launcher was
        // in front but typing went nowhere (owner's test).
        if let Some(window) = app2.get_webview_window(LAUNCHER) {
            let _ = window.with_webview(|wv| {
                // SAFETY: on macOS these are the WKWebView and its NSWindow, alive while the
                // window is; this runs on the main thread.
                let (view, window) = unsafe {
                    (
                        &*(wv.inner() as *const NSView),
                        &*(wv.ns_window() as *const NSWindow),
                    )
                };
                let _ = window.makeFirstResponder(Some(view));
            });
        }
    });
}

/// After hiding the launcher: if something made EasyPlots the active app (the Save dialog,
/// the colour panel) and Settings is closed, hide the app so the previous app gets the
/// keyboard back. Normally the panel never activates the app and there is nothing to do.
pub fn give_back_focus(app: &AppHandle) {
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let ns_app = NSApplication::sharedApplication(mtm);
        if ns_app.isActive() {
            ns_app.hide(None);
        }
    });
}

/// Make EasyPlots the active app, for the Save dialog and the colour panel, which only
/// work (and only stay open) in the active app.
#[allow(deprecated)]
pub fn activate(app: &AppHandle) {
    let _ = app.run_on_main_thread(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        }
    });
}
