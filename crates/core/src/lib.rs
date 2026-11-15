//! The EasyPlots engine.
//!
//! Everything that turns a command string into a saved plot lives here. It must not depend
//! on Tauri or any GUI code, so it can be tested with `cargo test` anywhere and reused by
//! the `easyplots` terminal command and the app.
//!
//! The pipeline (see `docs/ARCHITECTURE.md`):
//!
//! ```text
//! command text ─ command::parse ─▶ PlotSpec ─ figure::build ─▶ Figure ─ render ─▶ SVG
//!                                   (names)    (+ CSV data)    (numbers)          │
//!                                                              export::encode ◀───┘
//!                                                              naming::create_unique ─▶ file
//! ```

pub mod advice;
pub mod analysis;
pub mod colormap;
pub mod command;
pub mod complete;
pub mod config;
pub mod data;
pub mod demo;
pub mod error;
pub mod export;
pub mod expr;
pub mod figure;
pub mod filter;
pub mod fit;
pub mod guide;
pub mod history;
pub mod naming;
pub mod paste;
pub mod recall;
pub mod render;
pub mod search;
pub mod settings;
pub mod sheets;
pub mod spec;
pub mod spectrum;
pub mod style;
pub mod styles;
pub mod ui_theme;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use config::Config;
pub use error::Error;
use figure::{DataCache, FigureSettings, Lookup};
use spec::LegendPos;
pub use spec::{Format, PlotSpec};
use style::Style;

/// The EasyPlots version, shared by the app and the terminal command.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Keeps the config and recently read CSV files between plots.
pub struct Engine {
    pub config: Config,
    cache: DataCache,
    /// Where `--style=NAME` files live (`styles/` next to `config.toml`).
    pub styles_dir: PathBuf,
}

/// A rendered figure, before it is encoded and saved.
pub struct Rendered {
    /// The command it was drawn from (stored in the saved file).
    pub command: String,
    pub spec: PlotSpec,
    pub svg: String,
    pub dpi: f64,
    pub title: String,
    pub timings: Timings,
    /// The fits' numbers (parameters ± standard errors, R², N), for the Terminal.
    pub fit_reports: Vec<String>,
    /// The plot advisor's hints (shown under the preview; never in the saved file).
    pub advice: Vec<advice::Advice>,
}

/// A saved plot.
#[derive(Debug)]
pub struct Saved {
    pub path: PathBuf,
    pub format: Format,
    pub timings: Timings,
    /// The drawing, for a thumbnail in the launcher.
    pub svg: String,
    pub fit_reports: Vec<String>,
    /// The plot advisor's hints, for the Terminal.
    pub advice: Vec<advice::Advice>,
}

/// How long each step took, for the speed targets.
#[derive(Debug, Default, Clone, Copy)]
pub struct Timings {
    pub parse: Duration,
    pub load: Duration,
    pub render: Duration,
    pub encode: Duration,
    pub write: Duration,
}

impl Timings {
    pub fn total(&self) -> Duration {
        self.parse + self.load + self.render + self.encode + self.write
    }
}

impl Engine {
    pub fn new(config: Config) -> Engine {
        Engine {
            config,
            cache: DataCache::default(),
            styles_dir: Config::default_path().with_file_name("styles"),
        }
    }

    /// Parse, load and draw `command` as SVG without saving anything.
    /// `cwd` is where relative file names are looked up first (the terminal's folder).
    pub fn render(&mut self, command: &str, cwd: Option<&Path>) -> Result<Rendered, Error> {
        let mut timings = Timings::default();
        let t = Instant::now();
        let mut spec = command::parse(command)?;
        timings.parse = t.elapsed();

        // A style file sits between config.toml and the command.
        let style_file = match &spec.figure.style {
            Some(name) => Some(styles::load(&self.styles_dir, name)?),
            None => None,
        };
        let mut config = match &style_file {
            Some(file) => {
                if spec.figure.format.is_none() {
                    spec.figure.format = file.format()?;
                }
                let span = spec.figure.style.as_ref().map_or(0..0, |s| s.span.clone());
                file.apply_to_spec(&mut spec, span);
                file.apply(&self.config)
            }
            None => self.config.clone(),
        };
        // A journal size (from the command, a style or config.toml) brings the journal's
        // usual font size, unless a style file sets its own.
        let size_name = match &spec.figure.size {
            Some(_) => spec.figure.size_name.clone(),
            None => Some(config.output.size.clone()),
        };
        if let Some(j) = size_name.as_deref().and_then(command::journal)
            && !style_file.as_ref().is_some_and(|f| f.sets_font_size())
        {
            config.style.font_size = j.font_size;
        }
        let mut settings = settings(&config, &spec)?;
        if let Some(file) = style_file {
            if let Some(colors) = file.colors {
                settings.style.palette = colors;
            }
            settings.style.line_styles = file.line_styles;
            settings.style.markers = file.markers;
        }
        let t = Instant::now();
        let lookup = Lookup::new(&self.config, cwd);
        let fig = figure::build(&spec, &settings, &lookup, &mut self.cache)?;
        timings.load = t.elapsed();

        let t = Instant::now();
        let (svg, drawn) = render::render_noting(&fig, &settings.style, command);
        timings.render = t.elapsed();
        let advice = advice::advise(command, &spec, &fig, &drawn, settings.style.palette.len());

        let title = fig
            .panels
            .iter()
            .find_map(|p| p.title.as_deref().map(render::mathtext::plain))
            .unwrap_or_else(|| {
                spec.panels
                    .iter()
                    .flat_map(|p| &p.layers)
                    .map(|l| l.file.value.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
        Ok(Rendered {
            command: command.to_owned(),
            dpi: spec.figure.dpi.unwrap_or(config.output.dpi),
            spec,
            svg,
            title,
            timings,
            fit_reports: fig.fit_reports,
            advice,
        })
    }

    /// The numbers `command` actually draws — after `where=`, calculations, `stat=`,
    /// baselines and fits — as tab-separated text (⌘⇧C in the preview). The figure is
    /// built exactly as for `render` (the data cache makes that cheap), just not drawn.
    /// A `--style` file is ignored: it changes the look, never the numbers.
    pub fn drawn_table(&mut self, command: &str, cwd: Option<&Path>) -> Result<String, Error> {
        let spec = command::parse(command)?;
        let settings = settings(&self.config, &spec)?;
        let lookup = Lookup::new(&self.config, cwd);
        let fig = figure::build(&spec, &settings, &lookup, &mut self.cache)?;
        Ok(export::table_text(&fig))
    }

    /// Render `command` and save it in the output folder. Returns where it went.
    pub fn plot(&mut self, command: &str, cwd: Option<&Path>) -> Result<Saved, Error> {
        let rendered = self.render(command, cwd)?;
        self.save(&rendered)
    }

    /// Save an already rendered plot in the output folder, named by the template (or
    /// `--out`), never overwriting.
    pub fn save(&self, rendered: &Rendered) -> Result<Saved, Error> {
        self.save_in(rendered, None)
    }

    /// Like `save`, in a chosen format (the launcher's ⌘1/⌘2/⌘3) whatever the command or
    /// the config say. A `--out` name keeps its name with the new extension.
    pub fn save_in(&self, rendered: &Rendered, format: Option<Format>) -> Result<Saved, Error> {
        let mut spec = rendered.spec.clone();
        if let Some(f) = format {
            spec.figure.format = Some(f);
            if let Some(out) = &mut spec.figure.out {
                out.value = Path::new(&out.value)
                    .with_extension(f.extension())
                    .to_string_lossy()
                    .into_owned();
            }
        }
        let spec = &spec;
        let layers = || spec.panels.iter().flat_map(|p| &p.layers);
        let parts = naming::NameParts {
            files: layers()
                .map(|l| {
                    Path::new(&l.file.value).file_stem().map_or_else(
                        || l.file.value.clone(),
                        |s| s.to_string_lossy().into_owned(),
                    )
                })
                .collect(),
            types: layers().map(|l| l.kind.name().to_owned()).collect(),
            ys: layers()
                .flat_map(|l| l.y.iter().map(|y| y.value.clone()))
                .collect(),
            now: chrono::Local::now().naive_local(),
        };
        let dir = self.config.output_dir();
        let (path, format) = naming::plan(
            &dir,
            spec.figure.out.as_ref(),
            spec.figure.format,
            self.config.format()?,
            &self.config.output.name,
            &parts,
        )?;
        self.write(rendered, &path, format, true)
    }

    /// Save an already rendered plot at a path the owner picked in a "Save As" dialog. The
    /// file's extension picks the format; an existing file is replaced (the dialog has
    /// already asked).
    pub fn save_as(&self, rendered: &Rendered, path: &Path) -> Result<Saved, Error> {
        let format = path
            .extension()
            .and_then(|e| Format::from_name(&e.to_string_lossy()))
            .ok_or_else(|| Error::new("the file name must end in .png, .pdf or .svg"))?;
        self.write(rendered, path, format, false)
    }

    fn write(
        &self,
        rendered: &Rendered,
        path: &Path,
        format: Format,
        unique: bool,
    ) -> Result<Saved, Error> {
        let mut timings = rendered.timings;
        let t = Instant::now();
        let bytes = export::encode(
            &rendered.svg,
            format,
            rendered.dpi,
            &rendered.command,
            &rendered.title,
        )?;
        timings.encode = t.elapsed();

        let t = Instant::now();
        let (mut file, path) = if unique {
            naming::create_unique(path)?
        } else {
            let file = std::fs::File::create(path)
                .map_err(|e| Error::new(format!("cannot write {}: {e}", path.display())))?;
            (file, path.to_path_buf())
        };
        let written = file.write_all(&bytes).and_then(|_| file.sync_all());
        if let Err(e) = written {
            let _ = std::fs::remove_file(&path);
            return Err(Error::new(format!("cannot write {}: {e}", path.display())));
        }
        timings.write = t.elapsed();
        Ok(Saved {
            path,
            format,
            timings,
            svg: rendered.svg.clone(),
            fit_reports: rendered.fit_reports.clone(),
            advice: rendered.advice.clone(),
        })
    }

    /// The command a saved plot was made from (`again PLOT`). `plot` is a path, or a name
    /// in `cwd` (the terminal's folder) or the output folder.
    pub fn recall(&self, plot: &str, cwd: Option<&Path>) -> Result<String, Error> {
        let typed = config::expand_tilde(plot.trim());
        let tries: Vec<PathBuf> = if typed.is_absolute() {
            vec![typed]
        } else {
            cwd.map(|d| d.join(&typed))
                .into_iter()
                .chain([self.config.output_dir().join(&typed)])
                .collect()
        };
        let Some(path) = tries.iter().find(|p| p.is_file()) else {
            return Err(Error::new(format!(
                "no plot {plot} (looked in {})",
                tries
                    .iter()
                    .filter_map(|p| p.parent().map(|d| d.display().to_string()))
                    .collect::<Vec<_>>()
                    .join(" and ")
            )));
        };
        recall::stored_command(path)
    }

    /// The column names and types of a CSV (for `easyplots --columns` and the picker).
    /// Keep the look of a drawn plot as `styles/NAME.toml` (⌘D): only theme, font, size,
    /// colours, line styles, markers, legend, grid, log axes and tick steps.
    pub fn save_style(&self, rendered: &Rendered, name: &str) -> Result<PathBuf, Error> {
        let base = match &rendered.spec.figure.style {
            Some(s) => Some(styles::load(&self.styles_dir, s)?),
            None => None,
        };
        let text = styles::from_command(&rendered.spec, base.as_ref())?;
        styles::save(&self.styles_dir, name, &text)
    }

    /// The saved styles' names.
    pub fn style_names(&self) -> Vec<String> {
        styles::names(&self.styles_dir)
    }

    pub fn delete_style(&self, name: &str) -> Result<(), Error> {
        styles::delete(&self.styles_dir, name)
    }

    pub fn columns(&mut self, file: &str, cwd: Option<&Path>) -> Result<data::Table, Error> {
        self.columns_of_sheet(file, None, cwd)
    }

    /// The columns of a file, or of one sheet of a workbook.
    pub fn columns_of_sheet(
        &mut self,
        file: &str,
        sheet: Option<&str>,
        cwd: Option<&Path>,
    ) -> Result<data::Table, Error> {
        let opts = data::ReadOptions {
            skip: 0,
            sheet: sheet.map(str::to_owned),
        };
        self.columns_read(file, &opts, cwd)
    }

    /// The columns of a file read with `skiplines=` and `sheet=` as a command would.
    pub fn columns_read(
        &mut self,
        file: &str,
        opts: &data::ReadOptions,
        cwd: Option<&Path>,
    ) -> Result<data::Table, Error> {
        let lookup = Lookup::new(&self.config, cwd);
        let name = spec::Spanned {
            value: file.to_owned(),
            span: 0..file.len(),
        };
        // A pattern (`run_*.csv`) or a folder (`runs/`): the columns of its first file.
        let in_folder = lookup.folder(file).and_then(|dir| {
            let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .filter(|f| f.is_file() && search::is_table_file(f))
                .collect();
            files.sort();
            files.into_iter().next()
        });
        let path = match lookup.first_match(file).or(in_folder) {
            Some(p) => p,
            None => lookup.resolve(&name)?,
        };
        Ok((*self.cache.get_with(&path, opts)?).clone())
    }
}

/// Figure size, style and legend default from the (merged) config and the command. Several
/// panels without a size anywhere make the default figure bigger: three quarters of the
/// default per extra row or column.
fn settings(c: &Config, spec: &PlotSpec) -> Result<FigureSettings, Error> {
    let (width, height) = match spec.figure.size {
        Some(s) => s,
        None => {
            let (w, h) = command::size(&c.output.size, &(0..0))
                .map_err(|e| Error::new(format!("config output.size: {}", e.message)))?;
            let (rows, cols) = spec
                .figure
                .grid
                .unwrap_or_else(|| figure::auto_grid(spec.panels.len(), spec.figure.sharex));
            let grow = |n: usize| (n as f64 * 0.75).max(1.0);
            (
                (w * grow(cols)).min(72.0 * 40.0),
                (h * grow(rows)).min(72.0 * 40.0),
            )
        }
    };
    let style = Style::from_settings(
        spec.figure.theme.as_deref().unwrap_or(&c.style.theme),
        spec.figure.font.as_deref().unwrap_or(&c.style.font),
        c.style.font_size,
        c.style.line_width,
        c.style.grid,
        &ui_theme::palette_dir(),
    )?;
    let default_legend = LegendPos::from_name(&c.style.legend).ok_or_else(|| {
        Error::new(format!(
            "config style.legend \"{}\" must be best, tr, tl, br, bl, out or off",
            c.style.legend
        ))
    })?;
    Ok(FigureSettings {
        width,
        height,
        style,
        default_legend,
    })
}
