//! The launcher's blur strength (`[launcher] blur`, or Palette's `palette glass easyplots`).
//!
//! macOS has no public blur radius. Ghostty, kitty, WezTerm and Dispatch all use the same
//! private WindowServer call, `CGSSetWindowBackgroundBlurRadius`, and so does this. It is
//! looked up by name at run time (`dlsym`), never linked, so if a macOS update removes it
//! EasyPlots still starts: the lookup fails and the launcher falls back to Apple's frosted
//! "popover" material (the old on/off blur).

#[cfg(target_os = "macos")]
use std::ffi::{CStr, c_void};

/// Set the blur radius (0 = none) behind the window. Must run on the main thread.
#[cfg(target_os = "macos")]
pub fn set_radius(w: &tauri::WebviewWindow, radius: u32) -> Result<(), String> {
    use objc2_app_kit::NSWindow;
    type MainConnection = unsafe extern "C" fn() -> i32;
    type SetRadius = unsafe extern "C" fn(i32, isize, i32) -> i32;
    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const std::ffi::c_char) -> *mut c_void;
    }
    const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
    let find = |name: &CStr| {
        // SAFETY: dlsym with RTLD_DEFAULT only looks a name up; null means "not there".
        let p = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) };
        (!p.is_null()).then_some(p)
    };
    let missing = "this macOS has no private blur call";
    let conn = find(c"CGSMainConnectionID").ok_or(missing)?;
    let set = find(c"CGSSetWindowBackgroundBlurRadius").ok_or(missing)?;
    let ns_window = w.ns_window().map_err(|e| e.to_string())?;
    // SAFETY: the pointers come from dlsym for these exact C signatures (as Ghostty declares
    // them); `ns_window` is this window's live NSWindow; we're on the main thread.
    let code = unsafe {
        let conn: MainConnection = std::mem::transmute(conn);
        let set: SetRadius = std::mem::transmute(set);
        let number = (*(ns_window as *const NSWindow)).windowNumber();
        set(conn(), number, radius.min(100) as i32)
    };
    if code == 0 {
        Ok(())
    } else {
        Err(format!("macOS refused the blur (error {code})"))
    }
}

/// Elsewhere there is no blur behind windows: nothing to do, nothing to warn about.
#[cfg(not(target_os = "macos"))]
pub fn set_radius(_w: &tauri::WebviewWindow, _radius: u32) -> Result<(), String> {
    Ok(())
}
