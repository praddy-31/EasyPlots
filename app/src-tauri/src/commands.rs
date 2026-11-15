//! The functions the launcher window may call ("Tauri commands"). Each one locks the shared
//! state, asks `easyplots-core` to do the work and returns plain data the window can show.
//! Text positions are converted to the units JavaScript uses (UTF-16), so error underlines
//! and completions land in the right place.
//!
//! Plotting is two steps: `preview` draws the plot and shows it without saving; then
//! `save_preview` (output folder), `save_preview_as` (a folder picked in a Save dialog) or
//! `copy_preview` (clipboard) use that same drawing.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use base64::Engine as _;
use easyplots_core::complete::{self, Completion, Source, quote_if_needed};
use easyplots_core::config::{AfterSave, expand_tilde};
use easyplots_core::data::{ColumnKind, ReadOptions};
use easyplots_core::ui_theme::{self, Resolved};
use easyplots_core::{Error, Format, Rendered, Saved, export, search};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::state::{AppState, Inner};
use crate::windows;

#[derive(Serialize)]
pub struct LauncherInfo {
    /// "picker" or "command": which mode to start in.
    mode: String,
    history: Vec<String>,
    warnings: Vec<String>,
    data_dir: String,
    scanning: bool,
    ui: UiSettings,
    /// The format ↵ saves in (`output.format`).
    format: String,
    live_preview: bool,
    presets: Vec<easyplots_core::history::Preset>,
    /// Saved styles (`--style=NAME`), for the picker's style step.
    styles: Vec<String>,
    /// Bare word options (`markers`, `logy`…), coloured like options.
    flags: Vec<&'static str>,
}

/// The local appearance settings (`[launcher]` in config.toml) plus the colours.
#[derive(Serialize)]
pub struct UiSettings {
    opacity: f64,
    font: String,
    font_size: f64,
    hints: bool,
    theme: ThemeInfo,
}

#[derive(Serialize)]
pub struct ThemeInfo {
    #[serde(flatten)]
    resolved: Resolved,
    /// The window's opacity and blur: Palette's glass, or `[launcher]` opacity / blur.
    look: ui_theme::Look,
    /// Changes when Palette switches theme or glass, so the window knows to repaint.
    stamp: u64,
}

/// The launcher's look now: Palette's glass if it sets one, else config.toml's.
pub fn current_look(inner: &Inner) -> ui_theme::Look {
    let l = &inner.engine.config.launcher;
    ui_theme::look(
        l.opacity,
        l.blur,
        ui_theme::palette_glass(&ui_theme::palette_dir()).0,
    )
}

#[derive(Serialize)]
pub struct FileHit {
    display: String,
    insert: String,
    /// The file as typed, and the workbook sheet (if the entry is one sheet).
    file: String,
    sheet: Option<String>,
    /// Not a match for what was typed, but close in spelling (a likely typo).
    guess: bool,
    /// A folder of data files: picked, it gives one panel per file.
    folder: bool,
}

/// The first rows of a file, for the picker's quick look.
#[derive(Serialize)]
pub struct Peek {
    file: String,
    /// Rows in the whole file.
    rows: usize,
    names: Vec<String>,
    kinds: Vec<&'static str>,
    cells: Vec<Vec<String>>,
}

#[derive(Serialize)]
pub struct ColumnInfo {
    name: String,
    insert: String,
    kind: &'static str,
    /// Range, mean and blanks (or the text values), shown beside the column in the picker.
    summary: String,
}

#[derive(Serialize)]
pub struct Columns {
    file: String,
    rows: usize,
    columns: Vec<ColumnInfo>,
}

#[derive(Serialize)]
pub struct PlotError {
    message: String,
    /// UTF-16 positions in the command, for underlining.
    start: Option<usize>,
    end: Option<usize>,
    /// A ready fix: replace UTF-16 positions `start..end` with `text`.
    fix: Option<Fix>,
}

#[derive(Serialize)]
pub struct Fix {
    start: usize,
    end: usize,
    text: String,
}

impl PlotError {
    fn plain(e: Error) -> PlotError {
        PlotError {
            message: e.message,
            start: None,
            end: None,
            fix: None,
        }
    }

    fn in_command(e: Error, command: &str) -> PlotError {
        PlotError {
            start: e.span.as_ref().map(|s| utf16_pos(command, s.start)),
            end: e.span.as_ref().map(|s| utf16_pos(command, s.end)),
            fix: e.fix.map(|(r, text)| Fix {
                start: utf16_pos(command, r.start),
                end: utf16_pos(command, r.end),
                text,
            }),
            message: e.message,
        }
    }
}

#[derive(Serialize)]
pub struct Preview {
    image: String,
    millis: f64,
    /// The plot advisor's hints (empty when `launcher.advice` is off).
    advice: Vec<easyplots_core::advice::Advice>,
}

#[derive(Serialize)]
pub struct SaveResult {
    path: String,
    shown_path: String,
    after_save: AfterSave,
}

#[derive(Serialize)]
pub struct JsCompletion {
    start: usize,
    end: usize,
    items: Vec<complete::Suggestion>,
}

fn utf16_pos(s: &str, byte: usize) -> usize {
    let byte = byte.min(s.len());
    s[..byte].encode_utf16().count()
}

fn byte_pos(s: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}

fn tilde(path: &Path) -> String {
    let home = easyplots_core::config::home();
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// Re-scan the data folder in the background if the list is missing or older than 30 s.
pub fn refresh_files(app: &AppHandle) {
    let state = app.state::<AppState>();
    let root = {
        let mut inner = state.inner.lock().unwrap();
        let fresh = inner
            .files_scanned
            .is_some_and(|t| t.elapsed() < Duration::from_secs(30));
        if fresh || inner.scanning {
            return;
        }
        inner.scanning = true;
        (
            inner.engine.config.data_dir(),
            inner.engine.config.general.search_whole_disk,
            inner.engine.config.json_files(),
        )
    };
    let (root, whole_disk, json) = root;
    let app = app.clone();
    std::thread::spawn(move || {
        let mut files = search::scan(&root, json);
        if whole_disk {
            // Only file names from Spotlight's index; nothing is opened.
            search::merge(&mut files, search::spotlight(20_000));
        }
        let state = app.state::<AppState>();
        {
            let mut inner = state.inner.lock().unwrap();
            inner.files = files;
            inner.files_scanned = Some(Instant::now());
            inner.scanning = false;
        }
        let _ = app.emit("files-updated", ());
    });
}

fn theme_info(inner: &Inner) -> ThemeInfo {
    let resolved = ui_theme::resolve(
        &inner.engine.config.launcher.theme,
        &ui_theme::palette_dir(),
    );
    let (glass, glass_modified) = ui_theme::palette_glass(&ui_theme::palette_dir());
    let l = &inner.engine.config.launcher;
    let look = ui_theme::look(l.opacity, l.blur, glass);
    let millis = |m: Option<std::time::SystemTime>| {
        m.and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64)
    };
    // Either file changing (or the config's own opacity) must repaint.
    let stamp = millis(resolved.modified)
        .max(millis(glass_modified))
        .wrapping_add((look.opacity * 1000.0) as u64 + u64::from(look.blur));
    ThemeInfo {
        resolved,
        look,
        stamp,
    }
}

#[tauri::command]
pub fn launcher_info(state: State<AppState>) -> LauncherInfo {
    let inner = state.inner.lock().unwrap();
    let l = &inner.engine.config.launcher;
    LauncherInfo {
        mode: l.mode.clone(),
        history: inner.history.commands().to_vec(),
        warnings: inner.warnings.clone(),
        data_dir: tilde(&inner.engine.config.data_dir()),
        scanning: inner.scanning,
        format: inner.engine.config.output.format.clone(),
        live_preview: l.live_preview,
        presets: inner.history.presets().to_vec(),
        styles: inner.engine.style_names(),
        flags: easyplots_core::command::bare_flags().collect(),
        ui: UiSettings {
            opacity: current_look(&inner).opacity,
            font: l.font.clone(),
            font_size: l.font_size,
            hints: l.hints,
            theme: theme_info(&inner),
        },
    }
}

/// Polled every couple of seconds while the launcher is in front, so a Palette theme or
/// glass switch shows up live (the blur is the window's, so it is re-applied here too).
#[tauri::command]
pub fn current_theme(app: AppHandle, state: State<AppState>) -> ThemeInfo {
    let (info, blur_changed) = {
        let inner = state.inner.lock().unwrap();
        let info = theme_info(&inner);
        let changed = inner.blur_applied != Some((info.look.blur, info.look.opacity < 1.0));
        (info, changed)
    };
    if blur_changed {
        crate::windows::apply_blur(&app);
    }
    info
}

#[tauri::command]
pub fn search_files(query: String, state: State<AppState>) -> Vec<FileHit> {
    let inner = state.inner.lock().unwrap();
    let hit = |f: &search::FileEntry, guess| FileHit {
        insert: match &f.sheet {
            Some(s) => format!("{} sheet={}", quote_if_needed(f.file()), quote_if_needed(s)),
            None => quote_if_needed(&f.display),
        },
        display: f.display.clone(),
        file: f.file().to_owned(),
        sheet: f.sheet.clone(),
        guess,
        folder: f.display.ends_with('/'),
    };
    let recent = inner.history.recent_files(40);
    let hits: Vec<FileHit> = search::search(&inner.files, &query, 60, &recent)
        .into_iter()
        .map(|f| hit(f, false))
        .collect();
    if !hits.is_empty() {
        return hits;
    }
    search::closest(&inner.files, &query, 5)
        .into_iter()
        .map(|f| hit(f, true))
        .collect()
}

fn kind_name(k: ColumnKind) -> &'static str {
    match k {
        ColumnKind::Number => "numbers",
        ColumnKind::Date => "dates",
        ColumnKind::Text => "text",
    }
}

// `async` so loading a big CSV doesn't freeze the window (it runs off the main thread).
#[tauri::command]
pub async fn list_columns(
    file: String,
    sheet: Option<String>,
    skip: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Columns, PlotError> {
    let mut inner = state.inner.lock().unwrap();
    let read = ReadOptions {
        skip: skip.unwrap_or(0),
        sheet,
    };
    let table = inner
        .engine
        .columns_read(&file, &read, None)
        .map_err(PlotError::plain)?;
    Ok(Columns {
        file: table.file_name(),
        rows: table.rows,
        columns: table
            .columns
            .iter()
            .map(|c| ColumnInfo {
                name: c.name.clone(),
                insert: quote_if_needed(&c.name),
                kind: kind_name(c.data.kind()),
                summary: c.summary(),
            })
            .collect(),
    })
}

/// The first rows of the highlighted file. Only files in the data folder (and the built-in
/// samples) are opened: a file found by whole-disk search is read when it is picked, not
/// when it is merely highlighted.
#[tauri::command]
pub async fn peek_file(
    file: String,
    sheet: Option<String>,
    state: State<'_, AppState>,
) -> Result<Peek, PlotError> {
    let mut inner = state.inner.lock().unwrap();
    let data_dir = inner.engine.config.data_dir();
    let elsewhere = inner
        .files
        .iter()
        .any(|f| f.file() == file && !f.path.starts_with(&data_dir));
    if elsewhere {
        return Err(PlotError::plain(easyplots_core::Error::new(
            "outside the data folder: opened when picked",
        )));
    }
    let read = ReadOptions { skip: 0, sheet };
    let table = inner
        .engine
        .columns_read(&file, &read, None)
        .map_err(PlotError::plain)?;
    Ok(Peek {
        file: table.file_name(),
        rows: table.rows,
        names: table.columns.iter().map(|c| c.name.clone()).collect(),
        kinds: table
            .columns
            .iter()
            .map(|c| kind_name(c.data.kind()))
            .collect(),
        cells: table.peek(8),
    })
}

/// Completion uses the file index and the CSV cache.
struct AppSource<'a>(&'a mut Inner);

impl Source for AppSource<'_> {
    fn files(&mut self, query: &str, limit: usize) -> Vec<String> {
        let recent = self.0.history.recent_files(40);
        // `run_*.csv`: the pattern with each folder that has such files.
        let mut out: Vec<String> = search::pattern_folders(&self.0.files, query, limit)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        out.extend(
            search::search(&self.0.files, query, limit, &recent)
                .into_iter()
                .map(|f| f.display.clone()),
        );
        // The built-in sample files answer to `demo/…`: offer them once "demo" is being
        // typed (they'd be noise in the everyday list).
        let q = query.to_ascii_lowercase();
        if !q.is_empty() && ("demo".starts_with(&q) || q.starts_with("demo")) {
            let names: Vec<&str> = easyplots_core::demo::FILES
                .iter()
                .map(|(n, _)| *n)
                .collect();
            for name in search::filter(&names, query) {
                if out.len() >= limit {
                    break;
                }
                out.push(name.to_owned());
            }
        }
        out
    }

    fn columns(&mut self, file: &str, read: &ReadOptions) -> Option<Vec<(String, String)>> {
        let table = self.0.engine.columns_read(file, read, None).ok()?;
        Some(
            table
                .columns
                .iter()
                .map(|c| (c.name.clone(), kind_name(c.data.kind()).to_owned()))
                .collect(),
        )
    }

    fn plots(&mut self, query: &str, limit: usize) -> Vec<String> {
        let all = easyplots_core::recall::saved_plots(&self.0.engine.config.output_dir());
        let names: Vec<&str> = all.iter().map(String::as_str).collect();
        search::filter(&names, query)
            .into_iter()
            .take(limit)
            .map(str::to_owned)
            .collect()
    }

    fn styles(&mut self) -> Vec<String> {
        easyplots_core::styles::names(&self.0.engine.styles_dir)
    }

    fn presets(&mut self) -> Vec<(String, String)> {
        self.0
            .history
            .presets()
            .iter()
            .map(|p| (p.name.clone(), p.command.clone()))
            .collect()
    }
}

/// `again PLOT`: the command a saved plot was made from, to edit and draw again.
#[tauri::command]
pub fn recall_plot(plot: String, state: State<AppState>) -> Result<String, PlotError> {
    let inner = state.inner.lock().unwrap();
    inner.engine.recall(&plot, None).map_err(PlotError::plain)
}

#[tauri::command]
pub fn complete_command(input: String, cursor: usize, state: State<AppState>) -> JsCompletion {
    let mut inner = state.inner.lock().unwrap();
    let byte_cursor = byte_pos(&input, cursor);
    let Completion { start, end, items } =
        complete::complete(&input, byte_cursor, &mut AppSource(&mut inner));
    JsCompletion {
        start: utf16_pos(&input, start),
        end: utf16_pos(&input, end),
        items,
    }
}

/// Draw the plot and show it in the (enlarged) launcher, without saving anything.
// `async` so the window keeps drawing while the plot is made.
#[tauri::command]
pub async fn preview(
    command: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Preview, PlotError> {
    let started = Instant::now();
    let mut inner = state.inner.lock().unwrap();
    let rendered = inner
        .engine
        .render(&command, None)
        .map_err(|e| PlotError::in_command(e, &command))?;
    // Sharp on a Retina screen: about twice the preview's width in pixels.
    let (pw, _) = inner.engine.config.preview_size().unwrap_or((900.0, 680.0));
    let inches = svg_width_inches(&rendered).max(1.0);
    let dpi = (pw * 2.0 / inches).clamp(100.0, 400.0);
    let png =
        export::encode(&rendered.svg, Format::Png, dpi, &command, "").map_err(PlotError::plain)?;
    if let Err(e) = inner.history.push(&command) {
        inner.warnings.push(e.message);
    }
    let advice = if inner.engine.config.launcher.advice {
        rendered.advice.clone()
    } else {
        Vec::new()
    };
    inner.preview = Some(rendered);
    drop(inner);
    windows::size_launcher(&app, true);
    Ok(Preview {
        image: data_url(&png),
        millis: started.elapsed().as_secs_f64() * 1000.0,
        advice,
    })
}

/// A small drawing for the live preview while typing. Nothing is kept or remembered;
/// errors come back quietly (the box shows them only when ↵ is pressed).
#[tauri::command]
pub async fn preview_thumb(command: String, state: State<'_, AppState>) -> Result<String, ()> {
    let mut inner = state.inner.lock().unwrap();
    let rendered = inner.engine.render(&command, None).map_err(|_| ())?;
    let png = export::encode(&rendered.svg, Format::Png, 72.0, &command, "").map_err(|_| ())?;
    Ok(data_url(&png))
}

/// Keep the launcher open while something of ours (the colour panel) has the focus.
#[tauri::command]
pub fn hold_open(on: bool, app: AppHandle) {
    windows::hold_open(&app, on);
}

/// ⌘P in the launcher: keep it open when another app is clicked (pinned), or not.
#[tauri::command]
pub fn set_pinned(on: bool, state: State<'_, AppState>) {
    state.pinned.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// Save the plot being previewed as a preset called `name`.
#[tauri::command]
pub fn save_preset(name: String, app: AppHandle, state: State<AppState>) -> Result<(), PlotError> {
    let mut inner = state.inner.lock().unwrap();
    let command = inner
        .preview
        .as_ref()
        .map(|r| r.command.clone())
        .ok_or_else(|| {
            PlotError::plain(Error::new("draw a plot first, then save it as a preset"))
        })?;
    inner
        .history
        .save_preset(&name, &command)
        .map_err(PlotError::plain)?;
    drop(inner);
    // An open Settings window redraws its preset list (owner: it needed a restart).
    let _ = app.emit("config-changed", ());
    Ok(())
}

/// Keep the look of the plot being previewed as a style (⌘D): `styles/NAME.toml`.
#[tauri::command]
pub fn save_style(
    name: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<String, PlotError> {
    let inner = state.inner.lock().unwrap();
    let rendered = inner.preview.as_ref().ok_or_else(|| {
        PlotError::plain(Error::new(
            "draw a plot first, then keep its look as a style",
        ))
    })?;
    let path = inner
        .engine
        .save_style(rendered, name.trim())
        .map_err(PlotError::plain)?;
    drop(inner);
    let _ = app.emit("config-changed", ());
    Ok(tilde(&path))
}

/// A preset's command with another file in it.
#[tauri::command]
pub fn apply_preset(
    name: String,
    file: String,
    state: State<AppState>,
) -> Result<String, PlotError> {
    let inner = state.inner.lock().unwrap();
    let p = inner
        .history
        .preset(&name)
        .ok_or_else(|| PlotError::plain(Error::new(format!("no preset \"{name}\""))))?;
    easyplots_core::history::with_file(&p.command, &file).map_err(PlotError::plain)
}

/// The figure width in inches, read back from the SVG's `width="…pt"`.
fn svg_width_inches(r: &Rendered) -> f64 {
    r.svg
        .split("width=\"")
        .nth(1)
        .and_then(|rest| rest.split("pt\"").next())
        .and_then(|w| w.parse::<f64>().ok())
        .map_or(6.0, |pt| pt / 72.0)
}

/// Back to the normal launcher size (after editing the command again).
#[tauri::command]
pub fn end_preview(app: AppHandle) {
    windows::size_launcher(&app, false);
}

fn after_saving(app: &AppHandle, saved: &Saved, after_save: AfterSave) -> SaveResult {
    let shown_path = tilde(&saved.path);
    match after_save {
        AfterSave::Open => open(&saved.path, false),
        AfterSave::Reveal => open(&saved.path, true),
        AfterSave::Notify => {
            use tauri_plugin_notification::NotificationExt;
            let _ = app
                .notification()
                .builder()
                .title("Plot saved")
                .body(&shown_path)
                .show();
        }
        AfterSave::Thumbnail | AfterSave::None => {}
    }
    SaveResult {
        path: saved.path.display().to_string(),
        shown_path,
        after_save,
    }
}

/// Save the previewed plot in the output folder (named by the template, never overwriting).
#[tauri::command]
pub async fn save_preview(
    format: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SaveResult, PlotError> {
    let mut inner = state.inner.lock().unwrap();
    let rendered = inner.preview.as_ref().ok_or_else(|| {
        PlotError::plain(Error::new(
            "nothing to save yet — press ↵ to draw the plot first",
        ))
    })?;
    // ⌘1 / ⌘2 / ⌘3 save this one plot as PNG / PDF / SVG.
    let format = format.as_deref().and_then(Format::from_name);
    let saved = inner
        .engine
        .save_in(rendered, format)
        .map_err(PlotError::plain)?;
    let after_save = inner
        .engine
        .config
        .after_save()
        .unwrap_or(AfterSave::Thumbnail);
    inner.last_saved = Some(saved.path.clone());
    drop(inner);
    Ok(after_saving(&app, &saved, after_save))
}

/// Ask where to save with the macOS Save dialog, then save the previewed plot there.
/// Returns `None` if the dialog was cancelled.
#[tauri::command]
pub async fn save_preview_as(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<SaveResult>, PlotError> {
    use tauri_plugin_dialog::DialogExt;

    let (suggested, folder) = {
        let inner = state.inner.lock().unwrap();
        let rendered = inner.preview.as_ref().ok_or_else(|| {
            PlotError::plain(Error::new(
                "nothing to save yet — press ↵ to draw the plot first",
            ))
        })?;
        let format = inner.engine.config.format().unwrap_or(Format::Png);
        let stem = rendered
            .spec
            .panels
            .first()
            .and_then(|p| p.layers.first())
            .and_then(|l| {
                Path::new(&l.file.value)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "plot".into());
        (
            format!("{stem}.{}", format.extension()),
            inner
                .last_saved
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| inner.engine.config.output_dir()),
        )
    };

    // Keep the launcher up while the dialog has focus.
    windows::hold_open(&app, true);
    let picked = app
        .dialog()
        .file()
        .set_title("Save plot")
        .set_directory(&folder)
        .set_file_name(&suggested)
        .add_filter("PNG image", &["png"])
        .add_filter("PDF document", &["pdf"])
        .add_filter("SVG drawing", &["svg"])
        .blocking_save_file();
    windows::hold_open(&app, false);
    windows::refocus_launcher(&app);

    let Some(picked) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let mut inner = state.inner.lock().unwrap();
    let rendered = inner.preview.as_ref().expect("checked above");
    let saved = inner
        .engine
        .save_as(rendered, &picked)
        .map_err(PlotError::plain)?;
    let after_save = inner
        .engine
        .config
        .after_save()
        .unwrap_or(AfterSave::Thumbnail);
    inner.last_saved = Some(saved.path.clone());
    drop(inner);
    Ok(Some(after_saving(&app, &saved, after_save)))
}

/// Put the previewed plot on the clipboard as an image (at the configured DPI): a PNG,
/// as a macOS screenshot does, or with `copy_format = "tiff"` the raw picture (which
/// macOS keeps as an uncompressed TIFF).
#[tauri::command]
pub async fn copy_preview(app: AppHandle, state: State<'_, AppState>) -> Result<(), PlotError> {
    let (png, as_png) = {
        let inner = state.inner.lock().unwrap();
        let as_png = inner.engine.config.output.copy_format != "tiff";
        let rendered = inner
            .preview
            .as_ref()
            .ok_or_else(|| PlotError::plain(Error::new("nothing to copy yet")))?;
        let png = export::encode(
            &rendered.svg,
            Format::Png,
            rendered.dpi,
            &rendered.command,
            "",
        )
        .map_err(PlotError::plain)?;
        (png, as_png)
    };
    #[cfg(target_os = "macos")]
    if as_png {
        return copy_png(&png)
            .then_some(())
            .ok_or_else(|| PlotError::plain(Error::new("could not copy the plot")));
    }
    let _ = as_png;
    let image = tauri::image::Image::from_bytes(&png)
        .map_err(|e| PlotError::plain(Error::new(e.to_string())))?;
    app.clipboard()
        .write_image(&image)
        .map_err(|e| PlotError::plain(Error::new(format!("could not copy: {e}"))))
}

/// ⌘⇧C: the numbers the previewed plot actually draws (after `where=`, calculations,
/// `stat=` and fits), on the clipboard as tab-separated text ready for Excel or Numbers.
#[tauri::command]
pub async fn copy_preview_data(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), PlotError> {
    let text = {
        let mut inner = state.inner.lock().unwrap();
        let command = inner
            .preview
            .as_ref()
            .map(|r| r.command.clone())
            .ok_or_else(|| PlotError::plain(Error::new("nothing to copy yet")))?;
        inner
            .engine
            .drawn_table(&command, None)
            .map_err(|e| PlotError::in_command(e, &command))?
    };
    app.clipboard()
        .write_text(text)
        .map_err(|e| PlotError::plain(Error::new(format!("could not copy: {e}"))))
}

/// The PNG file's own bytes on the general pasteboard, as type `public.png` (what
/// ⌘⇧⌃4 puts there). The clipboard plugin can only hand over raw pixels, which macOS
/// stores as a TIFF of several MB.
#[cfg(target_os = "macos")]
fn copy_png(png: &[u8]) -> bool {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
    use objc2_foundation::NSData;
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    let data = NSData::with_bytes(png);
    // SAFETY: NSPasteboardTypePNG is a constant string AppKit sets up at load.
    board.setData_forType(Some(&data), unsafe { NSPasteboardTypePNG })
}

/// A saved plot for the browser (⌘O).
#[derive(Serialize)]
pub struct RecentPlot {
    /// As shown and as typed after `again`: `~/Plots/simple_line_….png`.
    path: String,
    name: String,
    modified: u64,
    command: String,
}

/// The newest 60 plots EasyPlots saved in the plots folder.
#[tauri::command]
pub async fn recent_plots(state: State<'_, AppState>) -> Result<Vec<RecentPlot>, ()> {
    let (out, pasted) = {
        let inner = state.inner.lock().unwrap();
        let c = &inner.engine.config;
        (c.output_dir(), c.paste_dir())
    };
    Ok(easyplots_core::recall::recent(&out, &pasted, 60)
        .into_iter()
        .map(|p| RecentPlot {
            path: tilde(&p.path),
            name: p.name,
            modified: p.modified,
            command: p.command,
        })
        .collect())
}

/// A small picture of a saved plot, kept in the cache folder so the grid opens fast. PDFs
/// are drawn again from their stored command (no picture if their data is gone).
#[tauri::command]
pub async fn plot_thumb(path: String, state: State<'_, AppState>) -> Result<String, ()> {
    use std::hash::{Hash, Hasher};
    let file = checked_plot_path(&path, &state).ok_or(())?;
    let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (&file, modified).hash(&mut h);
    let cached = cache_dir()
        .join("thumbs")
        .join(format!("{:016x}.png", h.finish()));
    if let Ok(png) = std::fs::read(&cached) {
        return Ok(data_url(&png));
    }
    const WIDTH: u32 = 360;
    let png = match easyplots_core::recall::thumbnail(&file, WIDTH) {
        Some(png) => png,
        None => {
            let command = easyplots_core::recall::stored_command(&file).map_err(|_| ())?;
            let mut inner = state.inner.lock().unwrap();
            let rendered = inner.engine.render(&command, None).map_err(|_| ())?;
            export::svg_thumbnail(&rendered.svg, WIDTH).map_err(|_| ())?
        }
    };
    if let Some(dir) = cached.parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&cached, &png);
    }
    Ok(data_url(&png))
}

/// The bigger window (the preview size), for the saved-plots grid.
#[tauri::command]
pub fn grow_launcher(app: AppHandle) {
    windows::size_launcher(&app, true);
}

/// What the clipboard holds, if it's a table (for "Clipboard" in the picker's file list).
#[derive(Serialize)]
pub struct ClipboardTable {
    rows: usize,
    columns: usize,
}

#[tauri::command]
pub fn clipboard_table(app: AppHandle) -> Option<ClipboardTable> {
    let text = app.clipboard().read_text().ok()?;
    let t = easyplots_core::paste::table(&text)?;
    Some(ClipboardTable {
        rows: t.rows,
        columns: t.columns.len(),
    })
}

/// Save the clipboard's table in the pasted-data folder; returns the file as it should be
/// typed in a command (`~/Plots/pasted/pasted-2026-10-23-140500.tsv`).
#[tauri::command]
pub fn save_clipboard(app: AppHandle, state: State<AppState>) -> Result<String, PlotError> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| PlotError::plain(Error::new(format!("cannot read the clipboard: {e}"))))?;
    let dir = state.inner.lock().unwrap().engine.config.paste_dir();
    let path = easyplots_core::paste::save_now(&text, &dir).map_err(PlotError::plain)?;
    Ok(tilde(&path))
}

/// Plots can be opened from the launcher only if they're in the output folder or were just
/// saved through the Save dialog.
fn checked_plot_path(path: &str, state: &State<AppState>) -> Option<PathBuf> {
    let inner = state.inner.lock().unwrap();
    let real = expand_tilde(path).canonicalize().ok()?;
    let out = inner.engine.config.output_dir().canonicalize().ok()?;
    let chosen = inner
        .last_saved
        .as_ref()
        .and_then(|p| p.canonicalize().ok());
    (real.starts_with(&out) || chosen.as_ref() == Some(&real)).then_some(real)
}

fn open(path: &Path, reveal: bool) {
    let mut cmd = std::process::Command::new("open");
    if reveal {
        cmd.arg("-R");
    }
    let _ = cmd.arg(path).spawn();
}

#[tauri::command]
pub fn open_plot(path: String, reveal: bool, state: State<AppState>) {
    if let Some(p) = checked_plot_path(&path, &state) {
        open(&p, reveal);
    }
}

/// `~/Library/Caches/EasyPlots` (on Linux `~/.cache/easyplots`): thumbnails; safe to delete.
fn cache_dir() -> PathBuf {
    let home = easyplots_core::config::home();
    if cfg!(target_os = "macos") {
        home.join("Library/Caches/EasyPlots")
    } else {
        home.join(".cache/easyplots")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_conversion_round_trips() {
        let s = "line \"Temp (°C)\" y=µ😀 x";
        for (b, _) in s.char_indices() {
            assert_eq!(byte_pos(s, utf16_pos(s, b)), b);
        }
        assert_eq!(utf16_pos(s, s.len()), s.encode_utf16().count());
    }
}
