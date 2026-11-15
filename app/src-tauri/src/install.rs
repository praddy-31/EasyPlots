//! "Install Terminal Command…": links `/usr/local/bin/easyplots` to this app's program, so
//! `easyplots …` works in any Terminal window. The app runs the terminal command when it is
//! started under that name (see `main.rs`). Asks for the password once if needed.

#[cfg(target_os = "macos")]
pub fn install_terminal_command() {
    std::thread::spawn(|| {
        let message = match install() {
            Ok(()) => "The easyplots command is installed.\n\nOpen a new Terminal window and try:\n  easyplots --version".to_owned(),
            Err(e) => format!("The easyplots command was not installed.\n\n{e}"),
        };
        dialog(&message);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn install_terminal_command() {}

#[cfg(target_os = "macos")]
const LINK: &str = "/usr/local/bin/easyplots";

#[cfg(target_os = "macos")]
fn install() -> Result<(), String> {
    use std::path::Path;
    use std::process::Command;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_text = exe.to_string_lossy().into_owned();
    if exe_text.starts_with("/Volumes/") || exe_text.contains("/AppTranslocation/") {
        return Err(
            "Move EasyPlots into the Applications folder first, open it from there, then try again."
                .into(),
        );
    }
    // Without a password if /usr/local/bin is writable (e.g. set up by Homebrew)…
    let link = Path::new(LINK);
    let direct = (|| -> std::io::Result<()> {
        std::fs::create_dir_all("/usr/local/bin")?;
        if link.symlink_metadata().is_ok() {
            std::fs::remove_file(link)?;
        }
        std::os::unix::fs::symlink(&exe, link)
    })();
    if direct.is_ok() {
        return Ok(());
    }
    // …otherwise macOS asks for the administrator password.
    let shell = format!(
        "mkdir -p /usr/local/bin && ln -sf {} {LINK}",
        shell_quote(&exe_text)
    );
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        shell.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let out = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn dialog(message: &str) {
    let text = message.replace('\\', "\\\\").replace('"', "\\\"");
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "display dialog \"{text}\" buttons {{\"OK\"}} default button 1 with title \"EasyPlots\""
        ))
        .output();
}
