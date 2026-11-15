//! Turning a `PlotSpec` (names as typed) into a `Figure` (numbers ready to draw): find the
//! files, read them, pick the columns (or calculate them), fill in defaults (labels,
//! colours, markers), and do the counting that some plot types need: histogram bins, box
//! plot quartiles, stacking and fitted curves.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::Error;
use crate::colormap::Colormap;
use crate::config::{Config, expand_tilde};
use crate::data::{self, ColumnData, ColumnKind, ReadOptions, Table};
use crate::error::suggestion;
use crate::spec::*;
use crate::style::Style;
use crate::{expr, filter, fit};

mod columns;
mod fitting;
mod multivariate;
mod univariate;
pub use columns::*;
pub use fitting::*;
use multivariate::*;
use univariate::*;

#[derive(Debug, Clone)]
pub struct Figure {
    /// The fits' numbers (parameters ± standard errors, R², N), for the Terminal.
    pub fit_reports: Vec<String>,
    /// Relative row heights (empty: all the same); a residuals strip is a short row.
    pub row_weights: Vec<f64>,
    /// Relative column widths (empty: all the same), from `--widths=`.
    pub col_weights: Vec<f64>,
    /// One legend for all panels, on this side of the figure (`--legend=`, or by default
    /// for several panels).
    pub shared_legend: Option<SharedLegend>,
    /// Which panels' entries it holds (empty: every panel's). The automatic one takes
    /// only the panels that would have shown a legend themselves.
    pub shared_from: Vec<bool>,
    /// Size in points.
    pub width: f64,
    pub height: f64,
    pub panels: Vec<Panel>,
    /// Panel rows and columns.
    pub grid: (usize, usize),
    pub sharex: bool,
    pub sharey: bool,
    pub suptitle: Option<String>,
    pub letters: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisKind {
    Number,
    Date,
    Category,
}

#[derive(Debug, Clone)]
pub struct Panel {
    pub title: Option<String>,
    pub xlabel: Option<String>,
    pub ylabel: Option<String>,
    pub xaxis: AxisKind,
    /// Category names when `xaxis` is `Category` (x values are their positions).
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    pub xlim: (Option<f64>, Option<f64>),
    pub ylim: (Option<f64>, Option<f64>),
    /// Tick steps chosen with `xlim=0:100:20` (None: automatic).
    pub xstep: Option<f64>,
    pub ystep: Option<f64>,
    pub y2step: Option<f64>,
    /// Reference lines and shaded regions (`hline=`, `vline=`, `span=`, `yspan=`).
    pub hlines: Vec<f64>,
    pub vlines: Vec<f64>,
    pub spans: Vec<(f64, f64)>,
    pub yspans: Vec<(f64, f64)>,
    /// `hline2=` / `yspan2=` with `axis=right` columns: in right-axis units (with `y2=`
    /// they are turned into left-axis units instead; `vline2=` always into bottom units).
    pub hlines2: Vec<f64>,
    pub yspans2: Vec<(f64, f64)>,
    /// `rect=`, `circle=`, `segment=`.
    pub shapes: Vec<Shape>,
    /// What `peaks`, `dips`, `onset` and `area=` found, drawn in each series' colour.
    pub marks: Vec<Mark>,
    pub ref_labels: Vec<RefLabel>,
    pub notes: Vec<Note>,
    /// A second x axis along the top (`x2=`), in other units of the same x.
    pub x2: Option<TopAxis>,
    /// A right-hand y axis in other units of the left (`y2=`); its label is `y2label`.
    pub y2conv: Option<TopAxis>,
    /// `inset=5:8`: a zoomed copy of that x range inside the plot.
    pub inset: Option<Inset>,
    pub legend: LegendPos,
    /// The fits' numbers, for the Terminal (gathered into `Figure::fit_reports`).
    pub fit_reports: Vec<String>,
    /// `residuals`: each data point's distance from its fit, for the strip below.
    pub residuals: Vec<Series>,
    /// This panel is the residuals strip of the panel before it (same x axis).
    pub residual_of_prev: bool,
    /// Hints for the plot advisor that only building the panel can see (error bars from
    /// two values after stat=); shown under the preview, never drawn.
    pub hints: Vec<String>,
    pub grid: bool,
    /// `grid=x` / `grid=y`: only one axis's lines.
    pub grid_axis: Option<GridLines>,
    pub logx: bool,
    pub logy: bool,
    /// The right-hand y axis is logarithmic (`logy2`).
    pub logy2: bool,
    /// Colour each y axis like its curve when it has just one (`axiscolor=off` stops it).
    pub axis_color: bool,
    /// The right-hand y axis (for `axis=right` series).
    pub y2label: Option<String>,
    pub y2lim: (Option<f64>, Option<f64>),
    /// Bars and areas are stacked (their `base` is filled in).
    pub stacked: bool,
    /// Box plots of several columns side by side in each group (`box f x=group y=a,b`).
    pub box_grouped: bool,
    /// Category names on the y axis (heatmap rows), at positions 0, 1, 2… from the bottom.
    pub ycategories: Vec<String>,
    /// Heatmaps and contours: the colour map, the value range it spans, the colour bar
    /// label, and the contour levels.
    pub cmap: Colormap,
    pub zrange: (f64, f64),
    pub zlabel: Option<String>,
    pub levels: Vec<f64>,
    pub filled: bool,
    pub values: bool,
    /// How `values` writes its numbers (None: up to three significant figures).
    pub values_format: Option<NumberFormat>,
    /// A round plot (angle and radius); angles in degrees unless `radians`.
    pub polar: bool,
    pub radians: bool,
    /// `horizontal`: the data is kept upright here (categories on x) and turned on its
    /// side when drawn; labels, limits and reference lines are as seen on the page.
    pub horizontal: bool,
    /// Categories written in bold (`highlight=`).
    pub bold_categories: Vec<usize>,
    /// `stats` on box, violin and strip plots: lines written under each category's name
    /// (N, then mean ± sd); empty when not asked for.
    pub cat_notes: Vec<String>,
    /// Strips spread at random (`jitter`) instead of as a beeswarm.
    pub jitter: bool,
    /// `ybreak=20:80`: this range of the (left, linear) y axis is squeezed into a small
    /// gap with break marks, so data far apart keeps its detail.
    pub ybreak: Option<(f64, f64)>,
}

impl Panel {
    pub fn has_right_axis(&self) -> bool {
        self.series.iter().any(|s| s.right)
    }

    /// Whether a colour bar is drawn (heatmaps, contours, points coloured with `c=`).
    pub fn has_colorbar(&self) -> bool {
        self.series
            .iter()
            .any(|s| s.grid.is_some() || s.cvalues.is_some())
    }
}

#[derive(Debug, Clone)]
pub struct Series {
    pub kind: PlotKind,
    pub label: String,
    /// A second, smaller line under the label in the legend (`stats`).
    pub note: Option<String>,
    /// For histograms: the bin edges (one more than `y`).
    pub x: Vec<f64>,
    /// For histograms: the bar heights (counts or density).
    pub y: Vec<f64>,
    pub color: Color,
    pub style: LineStyle,
    pub width: f64,
    /// Markers to draw (`Marker::None` for a plain line).
    pub marker: Marker,
    pub alpha: f64,
    /// `alpha=` was written. Filled shapes that are see-through by default (boxes,
    /// violins, areas, bands) then take `alpha` as it stands, so `alpha=1` is solid.
    pub alpha_set: bool,
    /// Error bar half-lengths, one per point.
    pub yerr: Option<Vec<f64>>,
    pub xerr: Option<Vec<f64>>,
    /// The downward (leftward) lengths when a bar isn't the same both ways (quartiles
    /// around a median); `yerr` / `xerr` are then the upward ones.
    pub yerr_lo: Option<Vec<f64>>,
    pub xerr_lo: Option<Vec<f64>>,
    /// Draw `yerr` as a shaded band around the line (`band`) instead of bars.
    pub band: bool,
    /// The lower edge per point: areas between two columns, and anything stacked.
    pub base: Option<Vec<f64>>,
    /// Measured on the right-hand y axis.
    pub right: bool,
    pub step: StepWhere,
    /// Box plots: one box per group.
    pub boxes: Vec<BoxStats>,
    /// A fitted curve rather than data.
    pub is_fit: bool,
    /// The fit itself (on a fitted curve), so ⌘⇧C can give its value at each data x.
    pub fitted: Option<Box<crate::fit::Fitted>>,
    /// Heatmaps and contours: the values on a grid.
    pub grid: Option<Grid>,
    /// A colour per bar or box (`highlight=`); empty: all in `color`.
    pub colors: Vec<Color>,
    /// Strips: how far (in category widths) dots may spread either side of their x.
    pub spread: f64,
    /// The `by=` layer this series is a group of (where `by=` was written), so `corr`
    /// can take the groups together.
    pub by_layer: Option<usize>,
    /// `c=`: a value per point, drawn as its colour on the panel's colour map.
    pub cvalues: Option<Vec<f64>>,
}

impl Series {
    /// Point `i`'s vertical error bar as (down, up) lengths.
    pub fn yerr_at(&self, i: usize) -> Option<(f64, f64)> {
        let up = *self.yerr.as_ref()?.get(i)?;
        let down = self.yerr_lo.as_ref().and_then(|l| l.get(i)).copied();
        Some((down.unwrap_or(up), up))
    }

    /// Point `i`'s horizontal error bar as (left, right) lengths.
    pub fn xerr_at(&self, i: usize) -> Option<(f64, f64)> {
        let up = *self.xerr.as_ref()?.get(i)?;
        let down = self.xerr_lo.as_ref().and_then(|l| l.get(i)).copied();
        Some((down.unwrap_or(up), up))
    }
}

/// Something read off a curve, drawn in the curve's colour on its own y axis.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A ring at a point with a label beside it (a peak, a dip, an onset).
    Point {
        x: f64,
        y: f64,
        label: String,
        color: Color,
        right: bool,
    },
    /// A thin dashed line (an onset's baseline and tangent).
    Line {
        from: (f64, f64),
        to: (f64, f64),
        color: Color,
        right: bool,
    },
    /// A thin dashed curve through these points (a fitted baseline, `showbaseline`).
    Path {
        points: Vec<(f64, f64)>,
        color: Color,
        right: bool,
    },
    /// A short line across a group at its mean (`mean`, solid) or median (`median`,
    /// dashed), with the value beside it (`mean=label`).
    Level {
        from: (f64, f64),
        to: (f64, f64),
        color: Color,
        label: Option<String>,
        dashed: bool,
    },
    /// The area under part of a curve, shaded down to y = 0 (`area=`).
    Fill {
        outline: Vec<(f64, f64)>,
        color: Color,
        right: bool,
    },
}

/// Values on a grid: `z[j * xs.len() + i]` is the value at (`xs[i]`, `ys[j]`). For heatmaps
/// the positions are cell numbers (0, 1, 2…); for contours real coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub z: Vec<f64>,
}

impl Grid {
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.z[j * self.xs.len() + i]
    }
}

impl Series {
    fn new(kind: PlotKind, label: String, x: Vec<f64>, y: Vec<f64>, color: Color) -> Series {
        Series {
            kind,
            label,
            note: None,
            x,
            y,
            color,
            style: LineStyle::Solid,
            width: 1.0,
            marker: Marker::None,
            alpha: 1.0,
            alpha_set: false,
            yerr: None,
            xerr: None,
            yerr_lo: None,
            xerr_lo: None,
            band: false,
            base: None,
            right: false,
            step: StepWhere::Post,
            boxes: Vec::new(),
            is_fit: false,
            grid: None,
            colors: Vec::new(),
            spread: 0.35,
            by_layer: None,
            cvalues: None,
            fitted: None,
        }
    }

    /// The colour of bar or box `i`.
    pub fn color_at(&self, i: usize) -> Color {
        self.colors.get(i).copied().unwrap_or(self.color)
    }
}

/// One box of a box plot: the middle half of the values (`q1`…`q3`) with the median,
/// whiskers to the furthest values within 1.5 box heights, and the values beyond.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxStats {
    /// Position on the x axis (the group's category number).
    pub pos: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub low: f64,
    pub high: f64,
    pub outliers: Vec<f64>,
    /// Violins: (value, density) pairs along the values, from low to high.
    pub shape: Vec<(f64, f64)>,
}

impl BoxStats {
    pub fn of(pos: f64, values: &[f64]) -> Option<BoxStats> {
        let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
        if v.is_empty() {
            return None;
        }
        v.sort_by(f64::total_cmp);
        let q = |p: f64| {
            let at = p * (v.len() - 1) as f64;
            let (i, f) = (at.floor() as usize, at.fract());
            if i + 1 < v.len() {
                v[i] + (v[i + 1] - v[i]) * f
            } else {
                v[i]
            }
        };
        let (q1, median, q3) = (q(0.25), q(0.5), q(0.75));
        let reach = 1.5 * (q3 - q1);
        let inside = |x: &&f64| **x >= q1 - reach && **x <= q3 + reach;
        let low = v.iter().find(inside).copied().unwrap_or(q1);
        let high = v.iter().rev().find(inside).copied().unwrap_or(q3);
        let outliers = v.iter().copied().filter(|x| !inside(&x)).collect();
        Some(BoxStats {
            pos,
            q1,
            median,
            q3,
            low,
            high,
            outliers,
            shape: Vec::new(),
        })
    }
}

/// Loaded CSV files, kept in memory and reloaded only when the file changes.
/// A table as read, with the file's date and length when it was read.
type CachedTable = (Option<SystemTime>, u64, Arc<Table>);

#[derive(Default)]
pub struct DataCache {
    tables: HashMap<(PathBuf, ReadOptions), CachedTable>,
}

impl DataCache {
    pub fn get(&mut self, path: &Path) -> Result<Arc<Table>, Error> {
        self.get_with(path, &ReadOptions::default())
    }

    /// The table read with `opts` (`skiplines=`, `sheet=`); each way of reading is kept apart.
    pub fn get_with(&mut self, path: &Path, opts: &ReadOptions) -> Result<Arc<Table>, Error> {
        let meta = std::fs::metadata(path).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok());
        let len = meta.as_ref().map_or(0, |m| m.len());
        let key = (path.to_path_buf(), opts.clone());
        if let Some((m, l, table)) = self.tables.get(&key)
            && *m == modified
            && *l == len
            && modified.is_some()
        {
            return Ok(table.clone());
        }
        let table = Arc::new(data::load(path, opts)?);
        self.tables.insert(key, (modified, len, table.clone()));
        Ok(table)
    }
}

/// Where relative file names are looked up.
pub struct Lookup<'a> {
    /// The terminal's current folder (None in the app).
    pub cwd: Option<&'a Path>,
    pub data_dir: PathBuf,
}

impl<'a> Lookup<'a> {
    pub fn new(config: &Config, cwd: Option<&'a Path>) -> Self {
        Lookup {
            cwd,
            data_dir: config.data_dir(),
        }
    }

    fn folders(&self) -> Vec<&Path> {
        self.cwd
            .into_iter()
            .chain(std::iter::once(self.data_dir.as_path()))
            .collect()
    }

    /// `run_*.csv`: one layer per matching file (sorted by name), each like the original.
    /// Names without `*` or `?` stay as they are.
    pub fn expand_patterns(&self, layers: &[LayerSpec]) -> Result<Vec<LayerSpec>, Error> {
        let mut out = Vec::new();
        for layer in layers {
            if !layer.file.value.contains(['*', '?']) {
                out.push(layer.clone());
                continue;
            }
            let files = self.matching(&layer.file.value);
            if files.is_empty() {
                return Err(Error::at(
                    format!("no CSV files match {}", layer.file.value),
                    layer.file.span.clone(),
                ));
            }
            out.extend(one_layer_per_file(layer, files));
        }
        Ok(out)
    }

    /// The folder a typed name means (`runs`, `runs/`), if it is one.
    pub fn folder(&self, name: &str) -> Option<PathBuf> {
        if name.contains(['*', '?']) || crate::demo::resolve(name).is_some() {
            return None;
        }
        let typed = expand_tilde(name);
        let tries: Vec<PathBuf> = if typed.is_absolute() {
            vec![typed]
        } else {
            self.folders().iter().map(|f| f.join(&typed)).collect()
        };
        // A file of that name wins: `runs` may also be `runs.csv`'s short form.
        if tries
            .iter()
            .any(|p| p.is_file() || p.with_extension("csv").is_file())
        {
            return None;
        }
        tries.into_iter().find(|p| p.is_dir())
    }

    /// A folder in place of a file gives one panel per data file in it (sorted by name),
    /// each titled with its file's name: `line runs/ x=t y=temp` is a grid of every run.
    /// Other panels stay as they are.
    pub fn expand_folders(&self, spec: &PlotSpec) -> Result<PlotSpec, Error> {
        /// More small panels than this stop being readable.
        const MAX_PANELS: usize = 36;
        let mut out = spec.clone();
        out.panels.clear();
        for p in &spec.panels {
            let folder = p
                .layers
                .iter()
                .find_map(|l| self.folder(&l.file.value).map(|dir| (dir, l.file.clone())));
            let Some((dir, typed)) = folder else {
                out.panels.push(p.clone());
                continue;
            };
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|f| f.is_file() && crate::search::is_table_file(f))
                        .collect()
                })
                .unwrap_or_default();
            files.sort();
            if files.is_empty() {
                return Err(Error::at(
                    format!(
                        "{} is a folder with no data files in it (CSV, TSV or workbooks)",
                        typed.value
                    ),
                    typed.span,
                ));
            }
            // stat= averages files: the folder's files become layers of this one panel
            // (owner, round 13: `runs/ … stat=mean` said "needs several files").
            if p.layers
                .iter()
                .any(|l| l.file.value == typed.value && l.stat.is_some())
            {
                let mut panel = p.clone();
                panel.layers = p
                    .layers
                    .iter()
                    .flat_map(|l| {
                        if l.file.value == typed.value {
                            one_layer_per_file(l, files.clone())
                        } else {
                            vec![l.clone()]
                        }
                    })
                    .collect();
                out.panels.push(panel);
                continue;
            }
            if files.len() > MAX_PANELS {
                return Err(Error::at(
                    format!(
                        "{} holds {} data files; a grid shows up to {MAX_PANELS}. Narrow it \
                         with a pattern, e.g. {}/run_1*.csv",
                        typed.value,
                        files.len(),
                        typed.value.trim_end_matches('/')
                    ),
                    typed.span,
                ));
            }
            for file in files {
                let mut panel = p.clone();
                for l in &mut panel.layers {
                    if l.file.value == typed.value {
                        l.file.value = file.display().to_string();
                    }
                }
                let stem = file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                panel.title = Some(match &p.title {
                    Some(t) if !t.is_empty() => format!("{t}: {stem}"),
                    _ => stem,
                });
                out.panels.push(panel);
            }
        }
        Ok(out)
    }

    /// The first file a pattern like `run_*.csv` matches (None for plain names).
    pub fn first_match(&self, pattern: &str) -> Option<PathBuf> {
        if !pattern.contains(['*', '?']) {
            return None;
        }
        self.matching(pattern).into_iter().next()
    }

    /// Files matching a pattern like `lab/run_*.csv` (`*` any text, `?` one character;
    /// case doesn't matter), in the first folder (terminal folder, then data folder) that
    /// has any. A name without wildcards matches just that file. Also behind
    /// `easyplots --each`.
    pub fn matching(&self, pattern: &str) -> Vec<PathBuf> {
        let typed = expand_tilde(pattern);
        let (dir, name) = match (typed.parent(), typed.file_name()) {
            (Some(d), Some(n)) => (d.to_path_buf(), n.to_string_lossy().into_owned()),
            _ => return Vec::new(),
        };
        let bases: Vec<PathBuf> = if typed.is_absolute() {
            vec![dir]
        } else {
            self.folders().iter().map(|f| f.join(&dir)).collect()
        };
        for base in bases {
            let Ok(entries) = std::fs::read_dir(&base) else {
                continue;
            };
            let mut found: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.file_name().is_some_and(|n| {
                            wildcard(&name.to_lowercase(), &n.to_string_lossy().to_lowercase())
                        })
                })
                .collect();
            if !found.is_empty() {
                found.sort();
                return found;
            }
        }
        Vec::new()
    }

    /// Find a CSV by the name typed in the command.
    pub fn resolve(&self, name: &Spanned<String>) -> Result<PathBuf, Error> {
        // The built-in sample files answer to their reserved names (`demo/peak_onset.csv`,
        // `demo/peak_onset`) before any folder is searched; they aren't on disk.
        if let Some(demo) = crate::demo::resolve(&name.value) {
            return Ok(PathBuf::from(demo));
        }
        let typed = expand_tilde(&name.value);
        let mut tries: Vec<PathBuf> = Vec::new();
        if typed.is_absolute() {
            tries.push(typed.clone());
        } else {
            for folder in self.folders() {
                tries.push(folder.join(&typed));
            }
        }
        if typed.extension().is_none() {
            let with_ext: Vec<PathBuf> = tries.iter().map(|p| p.with_extension("csv")).collect();
            tries.extend(with_ext);
        }
        if let Some(found) = tries.iter().find(|p| p.is_file()) {
            return Ok(found.clone());
        }

        let mut known: Vec<String> = Vec::new();
        for folder in self.folders() {
            if let Ok(entries) = std::fs::read_dir(folder) {
                known.extend(entries.flatten().filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    n.to_ascii_lowercase().ends_with(".csv").then_some(n)
                }));
            }
        }
        let places = if typed.is_absolute() {
            String::new()
        } else {
            let names: Vec<String> = self
                .folders()
                .iter()
                .map(|f| f.display().to_string())
                .collect();
            format!(" (looked in {})", names.join(" and "))
        };
        Err(Error::at(
            format!(
                "file not found: {}{places}{}",
                name.value,
                suggestion(&name.value, known.iter().map(String::as_str))
            ),
            name.span.clone(),
        ))
    }
}

/// `layer` once per file (sorted by name), each like the original. With one y column,
/// lists (`color=red,blue`) go one entry per file, in name order; files past the end of a
/// list get the usual defaults; the file's name says which line is which.
fn one_layer_per_file(layer: &LayerSpec, files: Vec<PathBuf>) -> Vec<LayerSpec> {
    let mut out = Vec::new();
    for (i, path) in files.into_iter().enumerate() {
        let mut l = layer.clone();
        if l.y.len() == 1 {
            fn nth<T: Clone>(v: &mut Vec<T>, i: usize) {
                if v.len() > 1 {
                    *v = v.get(i).cloned().into_iter().collect();
                }
            }
            nth(&mut l.label, i);
            nth(&mut l.color, i);
            nth(&mut l.style, i);
            nth(&mut l.marker, i);
            nth(&mut l.right, i);
        }
        if l.label.is_empty() && l.y.len() == 1 {
            l.label = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .into_iter()
                .collect();
        }
        l.file.value = path.display().to_string();
        out.push(l);
    }
    out
}

/// Does `name` fit `pattern`, where `*` stands for any text and `?` for one character?
pub fn wildcard(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    // matched[j]: the pattern so far matches the first j characters of the name.
    let mut matched = vec![false; n.len() + 1];
    matched[0] = true;
    for c in p {
        if c == '*' {
            for j in 1..=n.len() {
                matched[j] = matched[j] || matched[j - 1];
            }
        } else {
            for j in (1..=n.len()).rev() {
                matched[j] = matched[j - 1] && (c == '?' || c == n[j - 1]);
            }
            matched[0] = false;
        }
    }
    matched[n.len()]
}

/// A top x axis: the bottom axis's values through a formula of `x` (or a right y axis:
/// the left one's through a formula of `y`).
#[derive(Debug, Clone)]
pub struct TopAxis {
    pub expr: expr::Expr,
    /// The formula's variable: `x` or `y`.
    pub var: &'static str,
    pub label: Option<String>,
    /// The x column's name, usable in the formula as well as `x`.
    pub alias: Option<String>,
}

impl TopAxis {
    /// The top axis's values at these bottom-axis values.
    pub fn values(&self, xs: &[f64]) -> Result<Vec<f64>, String> {
        let column = |name: &str| data::Column {
            name: name.to_owned(),
            data: ColumnData::Number(xs.to_vec()),
        };
        let mut columns = vec![column(self.var)];
        if let Some(a) = self.alias.as_deref().filter(|a| *a != self.var) {
            columns.push(column(a));
        }
        let table = Table {
            path: PathBuf::from("x2"),
            columns,
            rows: xs.len(),
            delimiter: b',',
            decimal_comma: false,
        };
        self.expr.eval(&table)
    }

    /// The bottom-axis value where the top axis reads `v`, between `lo` and `hi` (found by
    /// halving the interval; the formula keeps rising or falling there).
    pub fn inverse(&self, v: f64, lo: f64, hi: f64) -> Option<f64> {
        let f = |x: f64| self.values(&[x]).ok().and_then(|r| r.first().copied());
        let (mut a, mut b) = (lo, hi);
        let (fa, fb) = (f(a)?, f(b)?);
        if !((fa <= v && v <= fb) || (fb <= v && v <= fa)) {
            return None;
        }
        let rising = fb > fa;
        for _ in 0..60 {
            let m = (a + b) / 2.0;
            let fm = f(m)?;
            if (fm < v) == rising {
                a = m;
            } else {
                b = m;
            }
        }
        Some((a + b) / 2.0)
    }
}

/// Settings that apply to the whole figure, after combining the command and config.
pub struct FigureSettings {
    pub width: f64,
    pub height: f64,
    pub style: Style,
    pub default_legend: LegendPos,
}

/// Rows and columns for `n` panels without `--grid`: side by side up to three, then a
/// near-square grid; stacked in one column with `--sharex` (time series one above another).
pub fn auto_grid(n: usize, sharex: bool) -> (usize, usize) {
    match n {
        0 | 1 => (1, 1),
        _ if sharex => (n, 1),
        2 | 3 => (1, n),
        4 => (2, 2),
        _ => {
            let cols = (n as f64).sqrt().ceil() as usize;
            (n.div_ceil(cols), cols)
        }
    }
}

pub fn build(
    spec: &PlotSpec,
    settings: &FigureSettings,
    lookup: &Lookup,
    cache: &mut DataCache,
) -> Result<Figure, Error> {
    let spec = &lookup.expand_folders(spec)?;
    let panels = spec
        .panels
        .iter()
        .map(|p| {
            let mut p = p.clone();
            p.layers = lookup.expand_patterns(&p.layers)?;
            let p = split_groups(&p, lookup, cache)?;
            match summarise(&p, lookup, cache)? {
                Some(s) => {
                    let mut panel = build_panel(&s.spec, settings, lookup, cache, Some(s.table))?;
                    panel.hints = s.hints;
                    if !s.notes.is_empty() && panel.categories.len() == s.notes.len() {
                        panel.cat_notes = s.notes;
                    }
                    Ok(panel)
                }
                None => build_panel(&p, settings, lookup, cache, None),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let f = &spec.figure;
    for (on, axis) in [(f.sharex, "x"), (f.sharey, "y")] {
        if on && panels.len() > 1 && panels.iter().any(|p| p.horizontal) {
            return Err(Error::new(format!(
                "--share{axis} doesn't work with horizontal panels yet"
            )));
        }
        if on && panels.len() > 1 {
            let kinds: Vec<AxisKind> = panels
                .iter()
                .map(|p| {
                    if axis == "x" {
                        p.xaxis
                    } else {
                        AxisKind::Number
                    }
                })
                .collect();
            if kinds.iter().any(|k| *k != kinds[0]) || kinds[0] == AxisKind::Category {
                return Err(Error::new(format!(
                    "--share{axis} needs the same kind of {axis} axis (numbers or dates) in every panel"
                )));
            }
        }
    }
    // Residual strips: each under its panel, panels stacked in one column.
    let (panels, row_weights, grid) = if panels.iter().any(|p| !p.residuals.is_empty()) {
        let mut out = Vec::new();
        let mut weights = Vec::new();
        for p in panels {
            let strip = (!p.residuals.is_empty()).then(|| residual_panel(&p));
            out.push(p);
            weights.push(3.0);
            if let Some(r) = strip {
                out.push(r);
                weights.push(1.2);
            }
        }
        let n = out.len();
        (out, weights, Some((n, 1)))
    } else {
        (panels, Vec::new(), None)
    };
    let strips = grid.is_some();
    let grid = grid.unwrap_or_else(|| f.grid.unwrap_or_else(|| auto_grid(panels.len(), f.sharex)));
    // --widths= / --heights=: one relative size per column / row of the layout.
    for (name, sizes, n, what) in [
        ("widths", &f.widths, grid.1, "column"),
        ("heights", &f.heights, grid.0, "row"),
    ] {
        if !sizes.is_empty() && sizes.len() != n {
            return Err(Error::new(format!(
                "--{name} has {} value{} but the layout has {n} {what}{}: one size per {what} \
                 (--layout=ROWSxCOLUMNS sets the layout)",
                sizes.len(),
                if sizes.len() == 1 { "" } else { "s" },
                if n == 1 { "" } else { "s" },
            )));
        }
    }
    if strips && !f.heights.is_empty() {
        return Err(Error::new(
            "--heights can't be combined with residuals strips (they set the row heights)",
        ));
    }
    let row_weights = if f.heights.is_empty() {
        row_weights
    } else {
        f.heights.clone()
    };
    // One legend for the figure: the panels' own are left out. Asked for with --legend=,
    // and the default for several panels when those that would show a legend all list
    // the same entries (a legend inside a small panel covers data, and one copy is
    // enough). Panels with different entries keep their own: a merged legend couldn't
    // say which entry is in which panel. So do all panels when one says legend= itself,
    // or with --legend=off.
    let mut panels = panels;
    let had_legend: Vec<bool> = panels.iter().map(|p| p.legend != LegendPos::Off).collect();
    let by_hand = spec.panels.iter().any(|p| p.legend.is_some());
    let (shared_legend, shared_from) = match f.legend {
        Some(SharedLegend::Panels) => (None, Vec::new()),
        Some(side) => (Some(side), Vec::new()),
        None if spec.panels.len() > 1
            && !by_hand
            && !strips
            && same_entries(&panels, &had_legend)
            // A lone legend goes above its own panel, so that panel must be in the
            // top row; lower down it stays inside its panel.
            && (had_legend.iter().filter(|h| **h).count() > 1
                || had_legend.iter().position(|h| *h).is_some_and(|i| i < grid.1)) =>
        {
            (Some(SharedLegend::Top), had_legend)
        }
        None => (None, Vec::new()),
    };
    if shared_legend.is_some() {
        for p in &mut panels {
            p.legend = LegendPos::Off;
        }
    }
    Ok(Figure {
        fit_reports: panels.iter().flat_map(|p| p.fit_reports.clone()).collect(),
        row_weights,
        col_weights: f.widths.clone(),
        shared_legend,
        shared_from,
        width: settings.width,
        height: settings.height,
        grid,
        panels,
        sharex: f.sharex,
        sharey: f.sharey,
        suptitle: f.suptitle.clone(),
        letters: f.letters,
    })
}

/// Whether the panels that show a legend (`has`) all list the same labels, and at least
/// one does.
fn same_entries(panels: &[Panel], has: &[bool]) -> bool {
    let labels = |p: &Panel| -> Vec<String> {
        let mut l: Vec<String> = p
            .series
            .iter()
            .filter(|s| s.grid.is_none() && !s.label.is_empty())
            .map(|s| s.label.clone())
            .collect();
        l.sort();
        l.dedup();
        l
    };
    let mut sets = panels
        .iter()
        .zip(has)
        .filter(|(_, h)| **h)
        .map(|(p, _)| labels(p));
    match sets.next() {
        Some(first) => sets.all(|s| s == first),
        None => false,
    }
}

enum XValues {
    Index,
    Column(usize),
    Calc(Vec<f64>),
}

/// A y (or error) column as named in the command: a real column or a calculation.
enum Col {
    Index(usize),
    Calc { name: String, values: Vec<f64> },
}

/// What one data series is drawn from, while the layer is read.
struct Pending {
    layer: usize,
    /// On the right-hand y axis.
    right: bool,
    auto_label: bool,
    stem: String,
    filter: Option<String>,
    yname: String,
    /// The colour was given (`color=`), so `highlight=` keeps it.
    color_given: bool,
}

fn build_panel(
    spec: &PanelSpec,
    settings: &FigureSettings,
    lookup: &Lookup,
    cache: &mut DataCache,
    summary: Option<Arc<Table>>,
) -> Result<Panel, Error> {
    let style = &settings.style;
    let mut series: Vec<Series> = Vec::new();
    let mut info: Vec<Pending> = Vec::new();
    // showbaseline: (series, baseline coefficients) to draw; the fitted ranges to shade;
    // the baselines' numbers for the Terminal.
    let mut baseline_curves: Vec<(usize, Vec<f64>)> = Vec::new();
    let mut baseline_spans: Vec<(f64, f64)> = Vec::new();
    let mut baseline_reports: Vec<String> = Vec::new();
    let mut xaxis: Option<(AxisKind, std::ops::Range<usize>)> = None;
    let mut categories: Vec<String> = Vec::new();
    let mut xnames: Vec<String> = Vec::new();
    let mut box_grouped = false;

    // Some types need the panel to themselves.
    let alone = [
        PlotKind::Box,
        PlotKind::Violin,
        PlotKind::Heatmap,
        PlotKind::Polar,
        PlotKind::Qq,
    ];
    // Strips may go over boxes and violins of the same groups.
    let groupish = |k: PlotKind| matches!(k, PlotKind::Box | PlotKind::Violin | PlotKind::Strip);
    if let Some(first) = spec.layers.iter().find(|l| alone.contains(&l.kind))
        && let Some(other) = spec
            .layers
            .iter()
            .find(|l| l.kind != first.kind && !(groupish(first.kind) && groupish(l.kind)))
    {
        return Err(Error::at(
            format!(
                "{} plots can't share a panel with other plot types — put them in their own panel with ;",
                first.kind.name()
            ),
            other.file.span.clone(),
        ));
    }
    if spec
        .layers
        .iter()
        .filter(|l| l.kind == PlotKind::Heatmap)
        .count()
        > 1
    {
        return Err(Error::at(
            "one heatmap per panel — start another panel with ;",
            spec.layers[1].file.span.clone(),
        ));
    }
    let boxlike = |k: PlotKind| matches!(k, PlotKind::Box | PlotKind::Violin);
    let has_box = spec.layers.iter().any(|l| boxlike(l.kind));
    let has_strip = spec.layers.iter().any(|l| l.kind == PlotKind::Strip);
    if has_strip
        && has_box
        && let Some(l) = spec
            .layers
            .iter()
            .find(|l| l.x.is_none() && l.group.is_none())
    {
        return Err(Error::at(
            "a strip over boxes or violins needs the groups in a column: x=GROUP on each",
            l.file.span.clone(),
        ));
    }
    if spec.horizontal {
        if let Some(l) = spec.layers.iter().find(|l| !l.kind.is_grouped()) {
            return Err(Error::at(
                format!(
                    "horizontal turns bar, box, violin and strip plots on their side, not {}",
                    l.kind.name()
                ),
                l.file.span.clone(),
            ));
        }
        if spec
            .layers
            .iter()
            .any(|l| !l.right.is_empty() && l.right.iter().any(|r| *r))
        {
            return Err(Error::new(
                "horizontal plots have no right-hand axis (axis=right)",
            ));
        }
        for (on, what) in [
            (spec.x2.is_some(), "x2="),
            (spec.y2.is_some(), "y2="),
            (spec.inset.is_some(), "inset="),
            (spec.residuals, "residuals"),
            (!spec.shapes.is_empty(), "rect=, circle= and segment="),
            (
                spec.logy,
                "logy (the y axis holds the groups; logx for the values)",
            ),
        ] {
            if on {
                return Err(Error::new(format!(
                    "{what} doesn't work with horizontal yet"
                )));
            }
        }
    }
    let polar = spec.layers.iter().any(|l| l.kind == PlotKind::Polar);
    let mut ycategories: Vec<String> = Vec::new();
    let mut znames: Vec<(String, String, String)> = Vec::new();
    // Columns that points are coloured by (`c=`), for the colour bar's label.
    let mut cnames: Vec<String> = Vec::new();

    // fft over a date column: frequencies come out in Hz.
    let mut fft_hz = false;
    for (li, layer) in spec.layers.iter().enumerate() {
        let table = match &summary {
            Some(t) => t.clone(),
            None => {
                let path = lookup.resolve(&layer.file)?;
                cache
                    .get_with(&path, &layer.read)
                    .map_err(|e| e.or_at(layer.file.span.clone()))?
            }
        };

        // Which rows: all, or those matching `where=`.
        let mut keep: Vec<usize> = match &layer.filter {
            None => (0..table.rows).collect(),
            Some(f) => {
                let mask = filter::parse(&f.value)
                    .and_then(|flt| flt.mask(&table))
                    .map_err(|m| Error::at(format!("where: {m}"), f.span.clone()))?;
                let rows: Vec<usize> = (0..table.rows).filter(|&i| mask[i]).collect();
                if rows.is_empty() {
                    return Err(Error::at(
                        format!(
                            "where= keeps none of the {} rows of {}",
                            table.rows,
                            table.file_name()
                        ),
                        f.span.clone(),
                    ));
                }
                rows
            }
        };
        // A by= group keeps only its rows.
        if let Some(g) = &layer.group {
            let col = &table.columns[column(&table, &g.column)?].data;
            keep.retain(|&r| group_value(col, r).as_deref() == Some(g.value.as_str()));
        }
        // skiprows=: drop the first data rows; every=: keep every n-th of the rest.
        keep.retain(|&r| r >= layer.skiprows);
        if layer.every > 1 {
            keep = keep.into_iter().step_by(layer.every).collect();
        }
        if keep.is_empty() {
            return Err(Error::at(
                format!(
                    "skiprows= / every= leave none of the {} rows of {}",
                    table.rows,
                    table.file_name()
                ),
                layer.file.span.clone(),
            ));
        }
        // Heatmaps and contours read a grid of values instead of x and y columns.
        if layer.kind.is_grid() {
            let g = grid_layer(layer, &table, &keep)?;
            let kind = if layer.kind == PlotKind::Heatmap {
                categories = g.xlabels.clone();
                ycategories = g.ylabels.clone();
                AxisKind::Category
            } else {
                AxisKind::Number
            };
            match &xaxis {
                None => xaxis = Some((kind, layer.file.span.clone())),
                Some((k, span)) if *k != kind => {
                    return Err(Error::at(
                        "a contour needs number axes; this layer's x axis holds something else",
                        span.clone(),
                    ));
                }
                _ => {}
            }
            znames.push((g.xname.clone(), g.yname.clone(), g.zname.clone()));
            let mut s = Series::new(
                layer.kind,
                per_column(&layer.label, 0).unwrap_or_else(|| g.zname.clone()),
                Vec::new(),
                Vec::new(),
                per_column(&layer.color, 0).unwrap_or(style.foreground),
            );
            s.width = layer.width.unwrap_or(style.line_width * 0.7);
            s.alpha = layer.alpha.unwrap_or(1.0);
            s.alpha_set = layer.alpha.is_some();
            s.grid = Some(g.grid);
            series.push(s);
            info.push(Pending {
                layer: li,
                right: false,
                auto_label: layer.label.is_empty(),
                stem: table.file_stem(),
                filter: layer.filter.as_ref().map(|f| f.value.clone()),
                yname: g.zname,
                color_given: !layer.color.is_empty(),
            });
            continue;
        }

        // Which columns: explicit, calculated, or sensible defaults.
        let (x, ys) = choose_columns(layer, &table)?;
        // groups= without by=: the groups of a text x column, kept and put in order.
        if let Some(wanted) = layer.groups.as_ref().filter(|_| layer.group.is_none()) {
            let text_x = match &x {
                XValues::Column(i) => match &table.columns[*i].data {
                    ColumnData::Text(v) => Some((v, table.columns[*i].name.clone())),
                    _ => None,
                },
                _ => None,
            };
            let Some((v, name)) = text_x else {
                return Err(Error::at(
                    "groups= picks groups of by=COLUMN, or of a text x column",
                    wanted.span.clone(),
                ));
            };
            let mut found: Vec<String> = Vec::new();
            for &r in &keep {
                let g = v[r].trim();
                if !g.is_empty() && !found.iter().any(|f| f == g) {
                    found.push(g.to_owned());
                }
            }
            let order = choose_groups(found, Some(wanted), &format!("\"{name}\""))?;
            keep.retain(|&r| order.iter().any(|g| g == v[r].trim()));
            for g in order {
                if !categories.contains(&g) {
                    categories.push(g);
                }
            }
        }
        let pick = |v: &[f64]| -> Vec<f64> { keep.iter().map(|&i| v[i]).collect() };
        let x_span = layer
            .x
            .as_ref()
            .map_or(layer.file.span.clone(), |s| s.span.clone());

        let (mut kind, mut xs): (AxisKind, Vec<f64>) = match &x {
            XValues::Index => (
                AxisKind::Number,
                keep.iter().map(|&i| (i + 1) as f64).collect(),
            ),
            XValues::Calc(v) => (AxisKind::Number, pick(v)),
            XValues::Column(i) => match &table.columns[*i].data {
                ColumnData::Number(v) => (AxisKind::Number, pick(v)),
                ColumnData::Date(v) => (AxisKind::Date, pick(v)),
                ColumnData::Text(v) => {
                    let pos =
                        keep.iter()
                            .map(|&r| v[r].trim())
                            .map(|name| {
                                let idx = categories.iter().position(|c| c == name).unwrap_or_else(
                                    || {
                                        categories.push(name.to_owned());
                                        categories.len() - 1
                                    },
                                );
                                idx as f64
                            })
                            .collect();
                    (AxisKind::Category, pos)
                }
            },
        };
        match layer.kind {
            // A histogram's x axis holds the values themselves.
            PlotKind::Hist | PlotKind::Kde => kind = AxisKind::Number,
            // A spectrum's x axis holds frequencies.
            PlotKind::Fft => {
                if kind == AxisKind::Category {
                    return Err(Error::at(
                        "fft needs numbers or dates on x (evenly spaced, like times)",
                        x_span,
                    ));
                }
                fft_hz |= kind == AxisKind::Date;
                kind = AxisKind::Number;
            }
            PlotKind::Box | PlotKind::Violin | PlotKind::Strip => {
                if layer.x.is_some() || matches!(x, XValues::Column(_)) {
                    if kind != AxisKind::Category {
                        return Err(Error::at(
                            format!(
                                "{} plots group by a text column (e.g. x=site); leave out x to compare columns",
                                layer.kind.name()
                            ),
                            x_span,
                        ));
                    }
                    box_grouped = true;
                }
                kind = AxisKind::Category;
            }
            _ => {}
        }
        match &xaxis {
            None => xaxis = Some((kind, x_span.clone())),
            Some((k, _)) if *k != kind => {
                return Err(Error::at(
                    format!(
                        "this x column holds {} but an earlier one holds {} — one plot can't mix them",
                        axis_kind_name(kind),
                        axis_kind_name(*k)
                    ),
                    x_span,
                ));
            }
            _ => {}
        }
        // Bars of a text column: one row per name, or they'd hide each other.
        if layer.kind == PlotKind::Bar && kind == AxisKind::Category {
            let mut count = vec![0usize; categories.len()];
            for x in &xs {
                if let Some(c) = count.get_mut(*x as usize) {
                    *c += 1;
                }
            }
            if let Some((c, n)) = count.iter().enumerate().find(|(_, n)| **n > 1) {
                let col = layer.x.as_ref().map_or("x", |x| x.value.as_str());
                return Err(Error::at(
                    format!(
                        "\"{}\" comes {n} times in {col}, so its bars would hide each other — \
                         add stat=mean (or stat=median) for one bar per {col} with error bars, \
                         or use strip or box to show every value",
                        categories[c]
                    ),
                    x_span,
                ));
            }
        }
        xnames.push(match &x {
            XValues::Index => "Row".to_owned(),
            XValues::Calc(_) => layer
                .x
                .as_ref()
                .map_or_else(String::new, |s| s.value.clone()),
            XValues::Column(i) => table.columns[*i].name.clone(),
        });

        // Error bars: one column for all y columns, or one each.
        let mut errs: Vec<Vec<f64>> = layer
            .err
            .iter()
            .map(|e| numbers(&table, &column_or_calc(&table, e)?, &e.span).map(|(_, v)| pick(&v)))
            .collect::<Result<_, _>>()?;
        let mut errs_low: Vec<Vec<f64>> = layer
            .err_low
            .iter()
            .map(|e| numbers(&table, &column_or_calc(&table, e)?, &e.span).map(|(_, v)| pick(&v)))
            .collect::<Result<_, _>>()?;
        if errs.len() > 1 && errs.len() != ys.len() {
            return Err(Error::at(
                format!(
                    "give one err column, or one for each of the {} y columns",
                    ys.len()
                ),
                layer.err[0].span.start..layer.err.last().unwrap().span.end,
            ));
        }
        let mut xerr = match &layer.xerr {
            Some(e) => Some(pick(
                &numbers(&table, &column_or_calc(&table, e)?, &e.span)?.1,
            )),
            None => None,
        };
        // c=: a number per point, shown as its colour.
        let mut cvals = match &layer.c {
            Some(c) => {
                if layer.kind != PlotKind::Scatter {
                    return Err(Error::at(
                        "c= colours scatter points by a number column: use it with scatter \
                         (color= sets one fixed colour)",
                        c.span.clone(),
                    ));
                }
                if layer.group.is_some() {
                    return Err(Error::at(
                        "c= colours by a number and by= by group: keep one of them",
                        c.span.clone(),
                    ));
                }
                if let Some(given) = layer.color.first() {
                    let _ = given;
                    return Err(Error::at(
                        "c= already colours the points: leave out color= (cmap= picks the \
                         colour map)",
                        c.span.clone(),
                    ));
                }
                let (name, v) = numbers(&table, &column_or_calc(&table, c)?, &c.span)?;
                let v = pick(&v);
                if !v.iter().any(|x| x.is_finite()) {
                    return Err(Error::at(
                        format!("column \"{name}\" has no numbers to colour by"),
                        c.span.clone(),
                    ));
                }
                cnames.push(name);
                Some(v)
            }
            None => None,
        };
        if layer.band && errs.is_empty() {
            return Err(Error::at(
                "band shades ±error around the line: add err=COLUMN",
                layer.file.span.clone(),
            ));
        }

        let mut values: Vec<(String, Vec<f64>, std::ops::Range<usize>)> = Vec::new();
        for (col, y_span) in &ys {
            let (name, v) = numbers(&table, col, y_span)?;
            let mut v = pick(&v);
            // clip=0:100: values outside are treated as missing.
            if let Some((lo, hi)) = layer.clip {
                for x in &mut v {
                    if *x < lo || *x > hi {
                        *x = f64::NAN;
                    }
                }
            }
            // smooth=5: a moving average over the rows that are drawn.
            if let Some(n) = layer.smooth {
                v = crate::expr::moving_average(&v, n);
            }
            if !v.iter().any(|v| v.is_finite()) {
                let why = match layer.clip {
                    Some((lo, hi))
                        if pick(&numbers(&table, col, y_span)?.1)
                            .iter()
                            .any(|x| x.is_finite()) =>
                    {
                        format!(
                            "column \"{name}\" has no values inside clip={lo}:{hi} (clip applies \
                             to every y column; leave this column out or widen the range)"
                        )
                    }
                    _ => format!("column \"{name}\" has no numbers to plot"),
                };
                return Err(Error::at(why, y_span.clone()));
            }
            values.push((name, v, y_span.clone()));
        }
        // dropna: rows with a blank in any drawn column go (lines don't break at them).
        if layer.dropna {
            let ok: Vec<bool> = (0..xs.len())
                .map(|i| {
                    xs[i].is_finite()
                        && values.iter().all(|(_, v, _)| v[i].is_finite())
                        && errs.iter().all(|e| e[i].is_finite())
                        && xerr.as_ref().is_none_or(|e| e[i].is_finite())
                })
                .collect();
            let only = |v: &[f64]| -> Vec<f64> {
                v.iter()
                    .zip(&ok)
                    .filter(|(_, k)| **k)
                    .map(|(x, _)| *x)
                    .collect()
            };
            xs = only(&xs);
            for (_, v, _) in &mut values {
                *v = only(v);
            }
            errs = errs.iter().map(|e| only(e)).collect();
            errs_low = errs_low.iter().map(|e| only(e)).collect();
            xerr = xerr.map(|e| only(&e));
            cvals = cvals.map(|c| only(&c));
            if xs.is_empty() {
                return Err(Error::at(
                    "dropna leaves no rows: every row has a blank in a drawn column",
                    layer.file.span.clone(),
                ));
            }
        }
        // baseline=: a polynomial fitted over chosen x ranges and taken off each y column
        // (with showbaseline: drawn instead, with the ranges shaded).
        if layer.show_baseline && layer.baseline.is_none() {
            return Err(Error::at(
                "showbaseline draws the fitted baseline: add baseline= with the x ranges to fit \
                 it on, e.g. baseline=0:5,90:100",
                layer.file.span.clone(),
            ));
        }
        if let Some(b) = &layer.baseline {
            if !matches!(
                layer.kind,
                PlotKind::Line | PlotKind::Scatter | PlotKind::Step | PlotKind::Area
            ) || layer.between
            {
                return Err(Error::at(
                    "baseline= works with line, scatter, step and area plots",
                    b.span.clone(),
                ));
            }
            if kind != AxisKind::Number {
                return Err(Error::at(
                    "baseline= needs numbers on x (not dates or names)",
                    b.span.clone(),
                ));
            }
            let shape = ["constant", "linear", "poly2", "poly3"][b.degree.min(3)];
            for (k, (name, v, _)) in values.iter_mut().enumerate() {
                let (coef, used) =
                    crate::analysis::baseline(&xs, v, b.degree, &b.ranges).map_err(|m| {
                        Error::at(
                            format!(
                                "baseline of \"{}\": {m}",
                                crate::render::mathtext::plain(name)
                            ),
                            b.span.clone(),
                        )
                    })?;
                let ranges: Vec<String> = used
                    .iter()
                    .map(|(a, c)| format!("{}:{}", data::short(*a), data::short(*c)))
                    .collect();
                baseline_reports.push(format!(
                    "baseline of {} ({shape}, fitted over x = {}): {}{}",
                    crate::render::mathtext::plain(name),
                    ranges.join(", "),
                    poly_text(&coef),
                    if layer.show_baseline {
                        " — drawn, not taken off (showbaseline)"
                    } else {
                        ""
                    }
                ));
                if layer.show_baseline {
                    baseline_curves.push((series.len() + k, coef));
                    for r in used {
                        if !baseline_spans.contains(&r) {
                            baseline_spans.push(r);
                        }
                    }
                } else {
                    for (x, y) in xs.iter().zip(v.iter_mut()) {
                        *y -= crate::analysis::poly(&coef, *x);
                    }
                }
            }
        }
        // slope / integral: each y column becomes its slope against x, or its running area.
        if layer.derive != Derive::None {
            let word = if layer.derive == Derive::Slope {
                "slope"
            } else {
                "integral"
            };
            if !matches!(
                layer.kind,
                PlotKind::Line | PlotKind::Scatter | PlotKind::Step | PlotKind::Area
            ) || layer.between
            {
                return Err(Error::at(
                    format!("{word} works with line, scatter, step and area plots"),
                    layer.file.span.clone(),
                ));
            }
            if kind != AxisKind::Number {
                return Err(Error::at(
                    format!("{word} needs numbers on x (not dates or names)"),
                    x_span,
                ));
            }
            let xname = xnames.last().map_or("x", String::as_str);
            let wrap = |n: &str| {
                if n.contains([' ', '/', '(', '*', '+', '-']) {
                    format!("({n})")
                } else {
                    n.to_owned()
                }
            };
            for (name, v, _) in &mut values {
                *v = match layer.derive {
                    Derive::Slope => crate::analysis::slope(&xs, v),
                    _ => crate::analysis::integral(&xs, v),
                };
                *name = match layer.derive {
                    Derive::Slope => format!("d{}/d{}", wrap(name), wrap(xname)),
                    _ => format!("∫{} d{}", wrap(name), wrap(xname)),
                };
            }
            if !errs.is_empty() {
                return Err(Error::at(
                    format!("{word} can't carry error bars along; leave out err="),
                    layer.err[0].span.clone(),
                ));
            }
        }
        if layer.between {
            if layer.kind != PlotKind::Area || values.len() != 2 {
                return Err(Error::at(
                    "between fills an area between two columns: area FILE y=low,high between",
                    layer.file.span.clone(),
                ));
            }
            let (high_name, high, _) = values.pop().unwrap();
            let (low_name, low, _) = values.pop().unwrap();
            let n = series.len();
            let mut s = Series::new(
                PlotKind::Area,
                per_column(&layer.label, 0).unwrap_or_else(|| format!("{low_name} – {high_name}")),
                xs.clone(),
                high,
                per_column(&layer.color, 0).unwrap_or(style.palette[n % style.palette.len()]),
            );
            s.base = Some(low);
            s.width = layer.width.unwrap_or(style.line_width);
            s.alpha = layer.alpha.unwrap_or(1.0);
            s.alpha_set = layer.alpha.is_some();
            s.right = layer.right_at(0);
            series.push(s);
            info.push(Pending {
                layer: li,
                right: layer.right_at(0),
                auto_label: layer.label.is_empty(),
                stem: table.file_stem(),
                filter: layer.filter.as_ref().map(|f| f.value.clone()),
                yname: high_name,
                color_given: !layer.color.is_empty(),
            });
            continue;
        }

        let single = values.len() == 1;
        // Lists (`color=orange,blue`) go with the y columns in order; more entries than
        // columns is a mistake worth pointing out.
        for (option, given) in [
            ("label", layer.label.len()),
            ("color", layer.color.len()),
            ("dash", layer.style.len()),
            ("marker", layer.marker.len()),
            ("axis", layer.right.len()),
        ] {
            if given > 1 && given > values.len() {
                return Err(Error::at(
                    format!(
                        "{given} values in {option}= but {} y column{} — one each, in the same order",
                        values.len(),
                        if values.len() == 1 { "" } else { "s" }
                    ),
                    layer.file.span.clone(),
                ));
            }
        }
        for (k, (name, v, _)) in values.into_iter().enumerate() {
            let label = match layer.label.as_slice() {
                [l] if single => l.clone(),
                [l] => format!("{l} {name}"),
                list => list.get(k).cloned().unwrap_or_else(|| name.clone()),
            };
            let n = series.len();
            // A style file's markers apply to the types that draw markers anyway, or ask for them.
            let style_marker = (!style.markers.is_empty()
                && (matches!(layer.kind, PlotKind::Scatter) || layer.markers))
                .then(|| style.markers[n % style.markers.len()]);
            let marker = match (layer.kind, per_column(&layer.marker, k).or(style_marker)) {
                (_, Some(m)) => m,
                // Each series its own shape, so they differ without colour too.
                (PlotKind::Scatter | PlotKind::Resid | PlotKind::Qq, None) => {
                    Marker::CYCLE[n % Marker::CYCLE.len()]
                }
                (PlotKind::Line | PlotKind::Step | PlotKind::Polar, None) if layer.markers => {
                    Marker::CYCLE[n % Marker::CYCLE.len()]
                }
                // Strips tell their groups apart by position (and colour): plain dots.
                (PlotKind::Strip, None) => Marker::Circle,
                _ => Marker::None,
            };
            let mut s = Series::new(
                layer.kind,
                label,
                xs.clone(),
                v,
                per_column(&layer.color, k).unwrap_or(style.palette[n % style.palette.len()]),
            );
            // Lines take the next dash pattern (solid, dashed, dotted, dash-dot), counted
            // among the panel's lines only, so the first stays solid.
            let lines_before = series.iter().filter(|s| is_line_kind(s.kind)).count();
            s.style = per_column(&layer.style, k)
                .or_else(|| {
                    (!style.line_styles.is_empty())
                        .then(|| style.line_styles[n % style.line_styles.len()])
                })
                .unwrap_or(if is_line_kind(layer.kind) {
                    LineStyle::CYCLE[lines_before % LineStyle::CYCLE.len()]
                } else {
                    LineStyle::Solid
                });
            s.width = layer.width.unwrap_or(style.line_width);
            s.marker = marker;
            s.alpha = layer.alpha.unwrap_or(1.0);
            s.alpha_set = layer.alpha.is_some();
            s.right = layer.right_at(k);
            s.step = layer.step;
            s.yerr = errs.get(if errs.len() == 1 { 0 } else { k }).cloned();
            s.yerr_lo = errs_low
                .get(if errs_low.len() == 1 { 0 } else { k })
                .cloned();
            s.band = layer.band;
            s.xerr = xerr.clone();
            if let Some(c) = &cvals {
                if c.len() != s.x.len() {
                    return Err(Error::at(
                        "c= needs one value per point: it can't follow stat=, slope or integral",
                        layer
                            .c
                            .as_ref()
                            .map_or(layer.file.span.clone(), |c| c.span.clone()),
                    ));
                }
                s.cvalues = Some(c.clone());
            }
            s.by_layer = layer.group.as_ref().map(|g| g.column.span.start);
            series.push(s);
            info.push(Pending {
                layer: li,
                right: layer.right_at(k),
                auto_label: layer
                    .group
                    .as_ref()
                    .map_or(per_column(&layer.label, k).is_none(), |g| !g.labelled),
                stem: table.file_stem(),
                filter: layer.filter.as_ref().map(|f| f.value.clone()),
                yname: name,
                color_given: per_column(&layer.color, k).is_some(),
            });
        }
    }

    // Auto labels that clash get told apart: by file name ("run_a: temp") when they come
    // from different files, then by their row filter ("temp (t < 30)").
    let clashing = |series: &[Series], i: usize, differ: &dyn Fn(usize) -> bool| -> bool {
        info[i].auto_label
            && (0..series.len()).any(|j| j != i && series[j].label == series[i].label && differ(j))
    };
    let by_file: Vec<bool> = (0..series.len())
        .map(|i| clashing(&series, i, &|j| info[j].stem != info[i].stem))
        .collect();
    for (i, clash) in by_file.into_iter().enumerate() {
        if clash {
            series[i].label = format!("{}: {}", info[i].stem, series[i].label);
        }
    }
    let by_filter: Vec<bool> = (0..series.len())
        .map(|i| info[i].filter.is_some() && clashing(&series, i, &|_| true))
        .collect();
    for (i, clash) in by_filter.into_iter().enumerate() {
        if let (true, Some(f)) = (clash, &info[i].filter) {
            series[i].label = format!("{} ({f})", series[i].label);
        }
    }

    // stats: the numbers behind each series (before counting or curves) in its legend entry.
    if spec.stats {
        if series
            .iter()
            .any(|s| s.grid.is_some() || s.kind == PlotKind::Polar)
        {
            return Err(Error::new(
                "summary writes N and mean ± sd in the legend or under groups: heatmap, \
                 contour and polar plots have neither",
            ));
        }
        for s in series.iter_mut().filter(|s| s.grid.is_none()) {
            s.note = data::Stats::of(&s.y).map(|st| st.legend_text());
        }
    }

    // Counting for the plot types that need it. Next to a density curve, histograms show
    // density too, so the two share a scale.
    for s in series.iter_mut().filter(|s| s.kind == PlotKind::Fft) {
        let (fs, vs) = crate::spectrum::spectrum(&s.x, &s.y, spec.fft_scale, spec.fft_window)
            .map_err(|m| Error::new(format!("fft of \"{}\": {m}", s.label)))?;
        s.x = fs;
        s.y = vs;
        s.yerr = None;
        s.xerr = None;
        s.yerr_lo = None;
        s.xerr_lo = None;
    }
    let has_kde = series.iter().any(|s| s.kind == PlotKind::Kde);
    let density = spec.density || has_kde;
    histograms(spec, density, &mut series)?;
    for s in series.iter_mut().filter(|s| s.kind == PlotKind::Kde) {
        let (xs, ys) = kde_curve(&s.y, spec.bw, spec.logx).ok_or_else(|| {
            Error::new(format!(
                "kde: \"{}\" needs at least two different values",
                s.label
            ))
        })?;
        s.x = xs;
        s.y = ys;
    }
    // norm=: each curve scaled (after kde, fft, slope and baseline, before fits).
    let mut normed_kde = false;
    for (i, s) in series.iter_mut().enumerate() {
        let Some(n) = &spec.layers[info[i].layer].norm else {
            continue;
        };
        if !matches!(
            s.kind,
            PlotKind::Line
                | PlotKind::Scatter
                | PlotKind::Step
                | PlotKind::Area
                | PlotKind::Kde
                | PlotKind::Fft
        ) {
            let hint = if s.kind == PlotKind::Hist {
                " (histograms: density gives area 1)"
            } else {
                ""
            };
            return Err(Error::at(
                format!("norm= scales curves: line, scatter, step, area, kde or fft{hint}"),
                n.span.clone(),
            ));
        }
        let name = crate::render::mathtext::plain(&s.label);
        let finite: Vec<f64> = s.y.iter().copied().filter(|v| v.is_finite()).collect();
        let (lo, hi) = finite
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
                (a.min(v), b.max(v))
            });
        let fail = |m: String| Error::at(format!("norm of \"{name}\": {m}"), n.span.clone());
        let (shift, scale) = match n.value {
            Norm::Max if hi > 0.0 => (0.0, hi),
            Norm::Max => return Err(fail("its largest value isn't above zero".into())),
            Norm::First => match finite.first() {
                Some(f) if *f != 0.0 => (0.0, *f),
                _ => return Err(fail("its first value is zero".into())),
            },
            Norm::Range if hi > lo => (lo, hi - lo),
            Norm::Range => return Err(fail("all its values are the same".into())),
            Norm::Area => {
                let xnum = s.kind == PlotKind::Kde
                    || s.kind == PlotKind::Fft
                    || xaxis.as_ref().is_some_and(|(k, _)| *k == AxisKind::Number);
                if !xnum {
                    return Err(fail("norm=area needs numbers on x".into()));
                }
                let p = crate::analysis::sorted(&s.x, &s.y);
                let a: f64 = p
                    .windows(2)
                    .map(|w| (w[1].0 - w[0].0) * (w[0].1 + w[1].1) / 2.0)
                    .sum();
                if !(a.abs() > 0.0 && a.is_finite()) {
                    return Err(fail("its area is zero".into()));
                }
                (0.0, a.abs())
            }
        };
        let f = |v: &mut f64| *v = (*v - shift) / scale;
        s.y.iter_mut().for_each(f);
        if let Some(b) = &mut s.base {
            b.iter_mut().for_each(f);
        }
        for e in [&mut s.yerr, &mut s.yerr_lo].into_iter().flatten() {
            e.iter_mut().for_each(|v| *v /= scale.abs());
        }
        // A kde's column is its x axis (the values aren't scaled, only the density).
        if s.kind == PlotKind::Kde {
            normed_kde = true;
        } else {
            let y = &mut info[i].yname;
            *y = format!("Normalised {y}");
        }
    }
    if has_box {
        if box_grouped {
            for s in &mut series {
                let mut groups: Vec<f64> = s.x.clone();
                groups.retain(|g| g.is_finite());
                groups.sort_by(f64::total_cmp);
                groups.dedup();
                s.boxes = groups
                    .iter()
                    .filter_map(|g| {
                        let vals: Vec<f64> =
                            s.x.iter()
                                .zip(&s.y)
                                .filter(|(x, _)| *x == g)
                                .map(|(_, y)| *y)
                                .collect();
                        BoxStats::of(*g, &vals).map(|b| with_shape(b, &vals, s.kind, spec.bw))
                    })
                    .collect();
            }
        } else {
            categories = series.iter().map(|s| s.label.clone()).collect();
            for (k, s) in series.iter_mut().enumerate() {
                let (kind, values) = (s.kind, s.y.clone());
                s.boxes = BoxStats::of(k as f64, &values)
                    .map(|b| with_shape(b, &values, kind, spec.bw))
                    .into_iter()
                    .collect();
            }
        }
    }
    // Strips without x: each column is a group, named like a box plot's.
    if has_strip && !box_grouped && !has_box {
        categories = series.iter().map(|s| s.label.clone()).collect();
        for (k, s) in series.iter_mut().enumerate() {
            s.x = vec![k as f64; s.y.len()];
        }
    }
    // stats on groups: N and mean ± sd under each group's name (when one series each).
    let mut cat_notes: Vec<String> = Vec::new();
    if spec.stats && (has_box || has_strip) {
        let mut per_cat: Vec<(usize, Vec<f64>)> = vec![(0, Vec::new()); categories.len()];
        // A strip over boxes shows the same values: count the first type only.
        let first_kind = series.iter().find(|s| !s.is_fit).map(|s| s.kind);
        for (k, s) in series
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_fit && Some(s.kind) == first_kind)
        {
            let mut touched = vec![false; categories.len()];
            for (x, y) in s.x.iter().zip(&s.y) {
                let c = if box_grouped || s.kind == PlotKind::Strip {
                    x.round() as usize
                } else {
                    k
                };
                if let Some(cat) = per_cat.get_mut(c) {
                    cat.1.push(*y);
                    touched[c] = true;
                }
            }
            for (c, t) in touched.into_iter().enumerate() {
                per_cat[c].0 += usize::from(t);
            }
        }
        if per_cat.iter().all(|(n, _)| *n <= 1) {
            cat_notes = per_cat
                .iter()
                .map(|(_, v)| {
                    data::Stats::of(v).map_or_else(String::new, |st| {
                        format!("N = {}\n{} ± {}", st.n, st.show(st.mean), st.show(st.sd))
                    })
                })
                .collect();
            for s in &mut series {
                s.note = None;
            }
        }
    }
    // Strips of several series in the same groups sit side by side, like grouped bars.
    let strip_idx: Vec<usize> = (0..series.len())
        .filter(|&i| series[i].kind == PlotKind::Strip)
        .collect();
    let mut strip_dodged = false;
    if strip_idx.len() > 1 {
        let mut seen = vec![0usize; categories.len()];
        for &i in &strip_idx {
            let mut here = vec![false; categories.len()];
            for x in &series[i].x {
                if let Some(h) = here.get_mut(x.round() as usize) {
                    *h = true;
                }
            }
            for (c, h) in here.into_iter().enumerate() {
                seen[c] += usize::from(h);
            }
        }
        strip_dodged = seen.iter().any(|n| *n > 1);
        if strip_dodged {
            let each = 0.8 / strip_idx.len() as f64;
            for (j, &i) in strip_idx.iter().enumerate() {
                let shift = -0.4 + each * (j as f64 + 0.5);
                let s = &mut series[i];
                for x in &mut s.x {
                    *x += shift;
                }
                s.spread = each * 0.42;
            }
        }
    }
    // A strip over boxes or violins: the boxes are a grey backdrop and the dots take the
    // colours from the first (unless colours were given).
    if has_strip && has_box {
        let backdrop = mix(style.foreground, style.background, 0.55);
        let mut next = 0;
        for (i, s) in series.iter_mut().enumerate() {
            if info.get(i).is_some_and(|p| p.color_given) || s.is_fit {
                continue;
            }
            if s.kind == PlotKind::Strip {
                s.color = style.palette[next % style.palette.len()];
                next += 1;
            } else {
                s.color = backdrop;
            }
        }
    }
    // highlight=: the named groups (series) or bars keep their colour, the rest go grey.
    let mut bold_categories = Vec::new();
    if let Some(h) = &spec.highlight {
        let grey = mix(style.foreground, style.background, 0.68);
        let known: Vec<String> = series
            .iter()
            .map(|s| crate::render::mathtext::plain(&s.label).trim().to_owned())
            .chain(categories.iter().cloned())
            .collect();
        let names = split_names(&h.value, &known);
        let wanted = |name: &str| {
            let name = crate::render::mathtext::plain(name);
            names.iter().any(|w| w.eq_ignore_ascii_case(name.trim()))
        };
        let series_hit: Vec<bool> = series
            .iter()
            .map(|s| !s.is_fit && s.grid.is_none() && wanted(&s.label))
            .collect();
        let cat_hit: Vec<bool> = categories.iter().map(|c| wanted(c)).collect();
        for name in &names {
            if !known.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                let names: Vec<String> = if categories.is_empty() {
                    series
                        .iter()
                        .map(|s| crate::render::mathtext::plain(&s.label))
                        .collect()
                } else {
                    categories.clone()
                };
                return Err(Error::at(
                    format!(
                        "highlight: no bar or group \"{name}\"{} (there are: {})",
                        name_hint(name, &names),
                        names.join(", ")
                    ),
                    h.span.clone(),
                ));
            }
        }
        bold_categories = (0..categories.len()).filter(|&c| cat_hit[c]).collect();
        if series_hit.iter().any(|h| *h) {
            // Highlighted groups take the first colours, in order (unless given).
            let mut next = 0;
            for (i, s) in series.iter_mut().enumerate() {
                if s.is_fit || s.grid.is_some() {
                    continue;
                }
                if series_hit[i] {
                    if !info.get(i).is_some_and(|p| p.color_given) {
                        s.color = style.palette[next % style.palette.len()];
                    }
                    next += 1;
                } else {
                    s.color = grey;
                }
            }
        } else {
            let at = |x: f64| cat_hit.get(x.round() as usize).copied().unwrap_or(false);
            for s in series.iter_mut().filter(|s| !s.is_fit && s.grid.is_none()) {
                s.colors = if s.boxes.is_empty() {
                    s.x.iter()
                        .map(|x| if at(*x) { s.color } else { grey })
                        .collect()
                } else {
                    s.boxes
                        .iter()
                        .map(|b| if at(b.pos) { s.color } else { grey })
                        .collect()
                };
            }
        }
    }
    // mean: a line across each strip group at its mean.
    let mut levels: Vec<Mark> = Vec::new();
    let mut mean_reports: Vec<String> = Vec::new();
    for (m, median) in [(&spec.mean, false), (&spec.median, true)] {
        let Some(m) = m else {
            continue;
        };
        if !has_strip {
            return Err(Error::at(
                if median {
                    "median draws a line at each group's median on strip plots (box plots show it already)"
                } else {
                    "mean draws a line at each group's mean on strip plots (box plots show the median)"
                },
                m.span.clone(),
            ));
        }
        for &i in &strip_idx {
            let s = &series[i];
            let mut xs: Vec<f64> = s.x.iter().copied().filter(|x| x.is_finite()).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup();
            for x in xs {
                let v: Vec<f64> =
                    s.x.iter()
                        .zip(&s.y)
                        .filter(|(a, b)| **a == x && b.is_finite())
                        .map(|(_, b)| *b)
                        .collect();
                let Some(st) = data::Stats::of(&v) else {
                    continue;
                };
                let half = s.spread * 0.9;
                let at = if median { st.median } else { st.mean };
                levels.push(Mark::Level {
                    from: (x - half, at),
                    to: (x + half, at),
                    color: style.foreground,
                    label: (m.value == MeanMark::Label).then(|| st.show(at)),
                    dashed: median,
                });
                let cat = categories
                    .get(x.round() as usize)
                    .cloned()
                    .unwrap_or_default();
                let group = if strip_dodged {
                    format!("{} · {cat}", s.label)
                } else {
                    cat
                };
                let group = crate::render::mathtext::plain(&group);
                mean_reports.push(match BoxStats::of(0.0, &v) {
                    Some(b) if median => format!(
                        "{group}: median = {} (Q1–Q3 {} to {}, N = {})",
                        st.show(st.median),
                        st.show(b.q1),
                        st.show(b.q3),
                        st.n
                    ),
                    _ => format!(
                        "{group}: mean = {} ± {} (sd, N = {})",
                        st.show(st.mean),
                        st.show(st.sd),
                        st.n
                    ),
                });
            }
        }
    }
    if spec.stacked {
        stack(&mut series)?;
    }
    // showbaseline: each fitted baseline as a dashed curve in its series' colour, over the
    // series' x range.
    for (i, coef) in &baseline_curves {
        let s = &series[*i];
        let (lo, hi) =
            s.x.iter()
                .filter(|v| v.is_finite())
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
                    (a.min(v), b.max(v))
                });
        if hi > lo {
            levels.push(Mark::Path {
                points: (0..=100)
                    .map(|k| {
                        let x = lo + (hi - lo) * f64::from(k) / 100.0;
                        (x, crate::analysis::poly(coef, x))
                    })
                    .collect(),
                color: s.color,
                right: s.right,
            });
        }
    }
    let Fits {
        curves,
        reports: fit_reports,
        strip: residuals,
        fitted,
    } = fits(spec, &series, &info)?;
    if spec.residuals && curves.is_empty() {
        return Err(Error::new(
            "residuals shows how far the points are from a fit: add fit=linear (or another)",
        ));
    }
    if let Some(i) = (0..series.len())
        .find(|&i| series[i].kind.is_residual() && !fitted.iter().any(|f| f.0 == i))
    {
        return Err(Error::at(
            format!(
                "{} shows how far each point is from a fit: add fit=linear (or another)",
                series[i].kind.name()
            ),
            spec.layers[info[i].layer].file.span.clone(),
        ));
    }
    if spec.resid_hist && !spec.layers.iter().all(|l| l.kind == PlotKind::Resid) {
        return Err(Error::new(
            "hist after the file counts a fit's residuals into a histogram: it goes with \
             resid (resid FILE x=… y=… fit=linear hist); for a column's histogram start \
             with hist",
        ));
    }
    // resid and qq: the points become their residuals (qq: sorted, against normal
    // quantiles, with a reference line in place of the fit curve).
    let mut zero_lines: Vec<bool> = Vec::new();
    let mut qq_lines: Vec<(usize, Series)> = Vec::new();
    for (i, _, each) in &fitted {
        let i = *i;
        let kind = series[i].kind;
        if !kind.is_residual() {
            continue;
        }
        if xaxis.as_ref().is_some_and(|(k, _)| *k != AxisKind::Number) {
            return Err(Error::at(
                format!("{} needs numbers on x (not dates or names)", kind.name()),
                spec.layers[info[i].layer].file.span.clone(),
            ));
        }
        let auto = info[i].auto_label;
        let s = &mut series[i];
        if auto {
            s.label = format!("{} − fit", s.label);
        }
        let y = &mut info[i].yname;
        *y = format!("{y} − fit");
        if kind == PlotKind::Resid && spec.resid_hist {
            // Counted into bins below; a histogram keeps its values in `y` until then.
            s.y = each.iter().copied().filter(|v| v.is_finite()).collect();
            s.x.clear();
            s.yerr = None;
            s.xerr = None;
            s.yerr_lo = None;
            s.xerr_lo = None;
            s.colors.clear();
            s.marker = Marker::None;
            s.kind = PlotKind::Hist;
            continue;
        } else if kind == PlotKind::Resid {
            s.y.clone_from(each);
            if !zero_lines.contains(&s.right) {
                zero_lines.push(s.right);
            }
        } else {
            let Some((z, v, (a, b))) = crate::analysis::qq(each) else {
                return Err(Error::at(
                    "qq needs at least 3 residuals",
                    spec.layers[info[i].layer].file.span.clone(),
                ));
            };
            let (z0, z1) = (z[0], z[z.len() - 1]);
            s.x = z;
            s.y = v;
            s.yerr = None;
            s.xerr = None;
            s.yerr_lo = None;
            s.xerr_lo = None;
            s.colors.clear();
            let mut line = Series::new(
                PlotKind::Line,
                String::new(),
                vec![z0, z1],
                vec![a + b * z0, a + b * z1],
                s.color,
            );
            line.style = LineStyle::Dashed;
            line.width = s.width * 0.9;
            line.right = s.right;
            line.is_fit = true;
            qq_lines.push((i, line));
        }
        s.kind = PlotKind::Scatter;
    }
    if spec.resid_hist {
        histograms(spec, spec.density, &mut series)?;
    }
    let mut curves = curves;
    curves.retain(|(at, _)| !spec.layers[info[*at].layer].kind.is_residual());
    curves.extend(qq_lines);
    curves.sort_by_key(|c| c.0);
    // fitstats: each fit's numbers as lines of a small table in a corner.
    let mut stat_notes: Vec<String> = Vec::new();
    if spec.fit_stats.is_some() {
        if fitted.is_empty() {
            return Err(Error::new(
                "fitstats lists a fit's numbers: add fit=linear (or another)",
            ));
        }
        let many = fitted.len() > 1;
        for (i, f, each) in &fitted {
            let r: Vec<f64> = each.iter().copied().filter(|v| v.is_finite()).collect();
            let rmse = (r.iter().map(|v| v * v).sum::<f64>() / r.len().max(1) as f64).sqrt();
            if many {
                let label = series[*i].label.trim_end_matches(" − fit").to_owned();
                stat_notes.push(format!("{label}:"));
            }
            for line in f.stat_lines(rmse) {
                stat_notes.push(if many { format!("  {line}") } else { line });
            }
        }
    }
    for (at, f) in curves.into_iter().rev() {
        series.insert(at + 1, f);
    }
    let mut fit_reports = fit_reports;
    fit_reports.extend(mean_reports);
    fit_reports.extend(baseline_reports);
    // Notes the panel adds itself: equations placed with `fitlabel=`, and `corr`.
    let mut notes = spec.notes.clone();
    notes.extend(stat_notes.into_iter().map(|text| Note {
        text,
        x: f64::NAN,
        y: f64::NAN,
        arrow: false,
        corner: spec.fit_stats,
    }));
    if spec.fit_label != FitLabel::Legend {
        for s in series.iter_mut().filter(|s| s.is_fit) {
            let text = std::mem::take(&mut s.label);
            let (x, y, corner) = match spec.fit_label {
                FitLabel::Corner(c) => (f64::NAN, f64::NAN, Some(c)),
                FitLabel::At(x, y) => (x, y, None),
                _ => continue,
            };
            notes.push(Note {
                text,
                x,
                y,
                arrow: false,
                corner,
            });
        }
    }
    if let Some(kind) = spec.corr {
        let data: Vec<&Series> = series
            .iter()
            .filter(|s| !s.is_fit && s.grid.is_none() && s.boxes.is_empty())
            .collect();
        let name = match kind {
            Corr::Pearson => "r",
            Corr::Spearman => "r_s",
        };
        // Groups of one by= layer count together on the plot ("(all)"); each group's own
        // value goes to the Terminal.
        // (by= layer, label, x values, y values)
        type Set = (Option<usize>, String, Vec<f64>, Vec<f64>);
        let mut sets: Vec<Set> = Vec::new();
        for s in &data {
            if let Some(b) = s.by_layer {
                if let Some((r, n)) = correlation(&s.x, &s.y, kind) {
                    fit_reports.push(format!(
                        "{}: {name} = {r:.4} (N = {n})",
                        crate::render::mathtext::plain(&s.label)
                    ));
                }
                match sets.iter_mut().find(|e| e.0 == Some(b)) {
                    Some(e) => {
                        e.2.extend(&s.x);
                        e.3.extend(&s.y);
                    }
                    None => sets.push((Some(b), "all".into(), s.x.clone(), s.y.clone())),
                }
            } else {
                sets.push((None, s.label.clone(), s.x.clone(), s.y.clone()));
            }
        }
        let many = sets.len() > 1;
        for (by, label, xs, ys) in sets {
            let Some((r, n)) = correlation(&xs, &ys, kind) else {
                continue;
            };
            let plain = crate::render::mathtext::plain(&label);
            let (who, after) = match (by, many) {
                (Some(_), false) => (String::new(), " (all)".to_owned()),
                (_, true) => (format!("{plain}: "), String::new()),
                (None, false) => (String::new(), String::new()),
            };
            let value = format!("{r:.2}").replace('-', "\u{2212}");
            fit_reports.push(format!("{who}{name} = {r:.4} (N = {n}){after}"));
            notes.push(Note {
                text: format!("{who}${name} = {value}${after}"),
                x: f64::NAN,
                y: f64::NAN,
                arrow: false,
                // Best: the emptiest corner, chosen when drawing.
                corner: Some(LegendPos::Best),
            });
        }
    }
    let mut marks = read_curves(
        spec,
        &mut series,
        xaxis.as_ref().map_or(AxisKind::Number, |(k, _)| *k),
        spec.xlabel
            .clone()
            .or_else(|| xnames.first().cloned())
            .unwrap_or_else(|| "x".to_owned()),
        style.foreground,
        &mut fit_reports,
    )?;
    marks.extend(levels);

    fn same(mut names: impl Iterator<Item = String>) -> Option<String> {
        let first = names.next()?;
        names.all(|n| n == first).then_some(first)
    }
    let label_or = |explicit: &Option<String>, auto: Option<String>| match explicit {
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s.clone()),
        None => auto,
    };
    let data_names: Vec<(bool, String)> = info.iter().map(|p| (p.right, p.yname.clone())).collect();
    let left_names = data_names
        .iter()
        .filter(|(r, _)| !r)
        .map(|(_, n)| n.clone());
    let right_names = data_names
        .iter()
        .filter(|(r, _)| *r)
        .map(|(_, n)| n.clone());
    let hist = spec
        .layers
        .iter()
        .all(|l| matches!(l.kind, PlotKind::Hist | PlotKind::Kde))
        || spec.resid_hist;
    let grids = !znames.is_empty() && spec.layers.iter().all(|l| l.kind.is_grid());
    let fft = spec.layers.iter().all(|l| l.kind == PlotKind::Fft);
    let (auto_x, auto_y) = if fft {
        (
            Some(
                if fft_hz {
                    "Frequency (Hz)"
                } else {
                    "Frequency"
                }
                .to_owned(),
            ),
            Some(
                match spec.fft_scale {
                    FftScale::Amplitude => "Amplitude",
                    FftScale::Power => "Power density",
                    FftScale::Db => "Amplitude (dB)",
                }
                .to_owned(),
            ),
        )
    } else if hist {
        (
            same(data_names.iter().map(|(_, n)| n.clone())),
            Some(
                if normed_kde {
                    "Relative density"
                } else if density {
                    "Density"
                } else {
                    "Count"
                }
                .to_owned(),
            ),
        )
    } else if grids || (!znames.is_empty() && xnames.is_empty()) {
        (
            same(znames.iter().map(|z| z.0.clone())).filter(|n| !n.is_empty()),
            same(znames.iter().map(|z| z.1.clone())).filter(|n| !n.is_empty()),
        )
    } else if polar {
        (None, None)
    } else if has_box || has_strip {
        (
            if box_grouped {
                same(xnames.into_iter())
            } else {
                None
            },
            same(data_names.iter().map(|(_, n)| n.clone())),
        )
    } else if spec.layers.iter().all(|l| l.kind == PlotKind::Qq) {
        (
            Some("Normal quantile".to_owned()),
            same(left_names.into_iter()),
        )
    } else {
        (same(xnames.into_iter()), same(left_names.into_iter()))
    };
    let shown = series
        .iter()
        .filter(|s| s.grid.is_none() && !(s.is_fit && s.label.is_empty()))
        .count();
    let legend = spec.legend.unwrap_or(if polar && shown > 1 {
        LegendPos::Out
    } else if (shown > 1 || (spec.stats && cat_notes.is_empty()) || spec.area.is_some())
        && !(has_box && !box_grouped)
        // Strips of one series per group: the axis names them already.
        && !(has_strip && !strip_dodged)
    {
        settings.default_legend
    } else {
        LegendPos::Off
    });

    let xaxis_kind = xaxis.as_ref().map_or(AxisKind::Number, |(k, _)| *k);
    if polar && (spec.logx || spec.logy) {
        return Err(Error::new("polar plots have no log axes"));
    }
    // The colour range: all grid values, unless zlim= says otherwise.
    let zvals = series
        .iter()
        .filter_map(|s| s.grid.as_ref())
        .flat_map(|g| g.z.iter().copied())
        .chain(
            series
                .iter()
                .filter_map(|s| s.cvalues.as_ref())
                .flat_map(|c| c.iter().copied()),
        )
        .filter(|v| v.is_finite());
    let (zlo, zhi) = zvals.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
        (a.min(v), b.max(v))
    });
    let (mut zlo, mut zhi) = if zlo <= zhi { (zlo, zhi) } else { (0.0, 1.0) };
    if let Some(v) = spec.zlim.0 {
        zlo = v;
    }
    if let Some(v) = spec.zlim.1 {
        zhi = v;
    }
    if zhi <= zlo {
        zhi = zlo + 1.0;
    }
    let levels: Vec<f64> = if series.iter().any(|s| s.kind == PlotKind::Contour) {
        let t = crate::render::ticks::linear(zlo, zhi, spec.levels.unwrap_or(10));
        t.major
            .into_iter()
            .filter(|v| *v > zlo && *v < zhi)
            .collect()
    } else {
        Vec::new()
    };
    if spec.logx && xaxis_kind != AxisKind::Number && !spec.horizontal {
        return Err(Error::new(format!(
            "logx needs numbers on the x axis, but it holds {}",
            axis_kind_name(xaxis_kind)
        )));
    }
    // On its side, logx is the values' axis.
    let (logx_data, logy_data) = if spec.horizontal {
        (false, spec.logx)
    } else {
        (spec.logx, spec.logy)
    };
    for (axis, on, values) in [
        (
            "x",
            logx_data,
            series.iter().flat_map(|s| s.x.iter()).collect::<Vec<_>>(),
        ),
        (
            if spec.horizontal { "x" } else { "y" },
            logy_data,
            series
                .iter()
                .filter(|s| !s.right)
                .flat_map(|s| s.y.iter())
                .collect::<Vec<_>>(),
        ),
    ] {
        if on && !values.iter().any(|v| **v > 0.0) {
            return Err(Error::new(format!(
                "log{axis} needs values above zero, and there are none to plot"
            )));
        }
    }

    // x2="1240/x": a top axis whose values come from the bottom one; it must keep rising or
    // falling over the data, or its ticks couldn't be placed.
    let x2 = match &spec.x2 {
        None => None,
        Some(f) => {
            if xaxis_kind != AxisKind::Number || polar {
                return Err(Error::at(
                    "a top axis (x2=) needs a bottom axis of plain numbers",
                    f.span.clone(),
                ));
            }
            let expr =
                expr::parse(&f.value).map_err(|m| Error::at(format!("x2: {m}"), f.span.clone()))?;
            let axis = TopAxis {
                expr,
                var: "x",
                label: spec.x2label.clone().filter(|l| !l.is_empty()),
                alias: auto_x.clone(),
            };
            let xs: Vec<f64> = series
                .iter()
                .flat_map(|s| s.x.iter().copied())
                .filter(|v| v.is_finite())
                .collect();
            let lo = spec
                .xlim
                .0
                .unwrap_or_else(|| xs.iter().copied().fold(f64::INFINITY, f64::min));
            let hi = spec
                .xlim
                .1
                .unwrap_or_else(|| xs.iter().copied().fold(f64::NEG_INFINITY, f64::max));
            if lo.is_finite() && hi.is_finite() && hi > lo {
                let samples: Vec<f64> = (0..=64)
                    .map(|i| lo + (hi - lo) * f64::from(i) / 64.0)
                    .collect();
                let vals = axis.values(&samples).map_err(|m| {
                    let m = if m.starts_with("no column") {
                        "write the formula with x for the bottom axis's values, e.g. x2=\"1240/x\""
                            .to_owned()
                    } else {
                        m
                    };
                    Error::at(format!("x2: {m}"), f.span.clone())
                })?;
                let rising = vals.windows(2).all(|w| w[1] > w[0]);
                let falling = vals.windows(2).all(|w| w[1] < w[0]);
                if vals.iter().any(|v| !v.is_finite()) || !(rising || falling) {
                    return Err(Error::at(
                        "x2 must keep rising or keep falling across the x range (and be defined everywhere on it)",
                        f.span.clone(),
                    ));
                }
            }
            Some(axis)
        }
    };

    // y2="y*1.8+32": a right axis whose values come from the left one, the same way.
    let y2conv = match &spec.y2 {
        None => None,
        Some(f) => {
            if series.iter().any(|s| s.right) {
                return Err(Error::at(
                    "y2= converts the left axis into other units, so it can't go with \
                     axis=right columns (those have their own right axis)",
                    f.span.clone(),
                ));
            }
            if polar || spec.layers.iter().any(|l| l.kind.is_grid()) {
                return Err(Error::at(
                    "a converted right axis (y2=) needs a y axis of plain numbers",
                    f.span.clone(),
                ));
            }
            let expr =
                expr::parse(&f.value).map_err(|m| Error::at(format!("y2: {m}"), f.span.clone()))?;
            let axis = TopAxis {
                expr,
                var: "y",
                label: None,
                alias: None,
            };
            let ys: Vec<f64> = series
                .iter()
                .flat_map(|s| s.y.iter().copied())
                .filter(|v| v.is_finite())
                .collect();
            let lo = spec
                .ylim
                .0
                .unwrap_or_else(|| ys.iter().copied().fold(f64::INFINITY, f64::min));
            let hi = spec
                .ylim
                .1
                .unwrap_or_else(|| ys.iter().copied().fold(f64::NEG_INFINITY, f64::max));
            if lo.is_finite() && hi.is_finite() && hi > lo {
                let samples: Vec<f64> = (0..=64)
                    .map(|i| lo + (hi - lo) * f64::from(i) / 64.0)
                    .collect();
                let vals = axis.values(&samples).map_err(|m| {
                    let m = if m.starts_with("no column") {
                        "write the formula with y for the left axis's values, e.g. y2=\"y*1.8+32\""
                            .to_owned()
                    } else {
                        m
                    };
                    Error::at(format!("y2: {m}"), f.span.clone())
                })?;
                let rising = vals.windows(2).all(|w| w[1] > w[0]);
                let falling = vals.windows(2).all(|w| w[1] < w[0]);
                if vals.iter().any(|v| !v.is_finite()) || !(rising || falling) {
                    return Err(Error::at(
                        "y2 must keep rising or keep falling across the y range (and be defined everywhere on it)",
                        f.span.clone(),
                    ));
                }
            }
            Some(axis)
        }
    };

    let mut refs = second_axis_refs(spec, &series, x2.as_ref(), y2conv.as_ref())?;
    // resid: a zero line on each axis that has residuals.
    for right in zero_lines {
        let lines = if right {
            &mut refs.hlines2
        } else {
            &mut refs.hlines
        };
        if !lines.contains(&0.0) {
            lines.push(0.0);
        }
    }
    let mut spans = spec.spans.clone();
    spans.extend(baseline_spans);
    // cross=50: a reference line at each level, on each axis that has curves.
    if let Some(Spanned {
        value: Cross::Levels(levels),
        ..
    }) = &spec.cross
    {
        for right in [false, true] {
            if !series.iter().any(|s| !s.is_fit && s.right == right) {
                continue;
            }
            let lines = if right {
                &mut refs.hlines2
            } else {
                &mut refs.hlines
            };
            for l in levels {
                if !lines.contains(l) {
                    lines.push(*l);
                }
            }
        }
    }

    // inset=5:8: an x range of a number axis (on a log axis, above zero).
    let inset = match &spec.inset {
        None => None,
        Some(i) => {
            let v = i.value;
            if v.x.0.is_nan() {
                return Err(Error::at(
                    "inset_at needs an inset= range too, e.g. inset=5:8",
                    i.span.clone(),
                ));
            }
            if xaxis_kind != AxisKind::Number || polar {
                return Err(Error::at(
                    "an inset needs a number x axis (not dates, names or polar)",
                    i.span.clone(),
                ));
            }
            if (spec.logx && v.x.0 <= 0.0) || (spec.logy && v.y.is_some_and(|y| y.0 <= 0.0)) {
                return Err(Error::at(
                    "on a log axis the inset's range must be above 0",
                    i.span.clone(),
                ));
            }
            Some(v)
        }
    };

    // A tick step needs an ordinary number axis.
    for (step, ok, axis) in [
        (
            &spec.xstep,
            (xaxis_kind == AxisKind::Number || spec.horizontal) && !spec.logx && !polar,
            "x",
        ),
        (
            &spec.ystep,
            !spec.logy && ycategories.is_empty() && !polar && !spec.horizontal,
            "y",
        ),
        (&spec.y2step, true, "right y"),
    ] {
        if let Some(s) = step
            && !ok
        {
            return Err(Error::at(
                format!(
                    "a tick step only works when the {axis} axis shows plain numbers (not dates, names, log or polar); leave out the third part"
                ),
                s.span.clone(),
            ));
        }
    }

    // On its side, the values run along x and the groups down y.
    let (auto_x, auto_y) = if spec.horizontal {
        (auto_y, auto_x)
    } else {
        (auto_x, auto_y)
    };
    let panel = Panel {
        title: spec.title.clone().filter(|t| !t.is_empty()),
        xlabel: label_or(&spec.xlabel, auto_x),
        ylabel: label_or(&spec.ylabel, auto_y),
        xaxis: xaxis_kind,
        categories,
        series,
        xlim: spec.xlim,
        ylim: spec.ylim,
        xstep: spec.xstep.as_ref().map(|s| s.value),
        ystep: spec.ystep.as_ref().map(|s| s.value),
        y2step: spec.y2step.as_ref().map(|s| s.value),
        hlines: refs.hlines,
        vlines: refs.vlines,
        spans,
        yspans: refs.yspans,
        hlines2: refs.hlines2,
        yspans2: refs.yspans2,
        shapes: spec.shapes.clone(),
        marks,
        ref_labels: refs.labels,
        notes,
        x2,
        y2conv,
        inset,
        legend,
        fit_reports,
        residuals: if spec.residuals {
            residuals
        } else {
            Vec::new()
        },
        residual_of_prev: false,
        hints: Vec::new(),
        grid: spec.grid.unwrap_or(style.grid),
        grid_axis: spec.grid_axis,
        logx: spec.logx,
        logy: spec.logy,
        logy2: spec.logy2,
        axis_color: spec.axis_color,
        y2label: label_or(&spec.y2label, same(right_names.into_iter())),
        y2lim: spec.y2lim,
        stacked: spec.stacked,
        box_grouped,
        ycategories,
        cmap: spec
            .cmap
            .as_deref()
            .and_then(Colormap::from_name)
            .unwrap_or(Colormap::DEFAULT),
        zrange: (zlo, zhi),
        zlabel: label_or(
            &spec.zlabel,
            same(znames.iter().map(|z| z.2.clone()).chain(cnames)),
        ),
        levels,
        filled: spec.filled,
        values: spec.values,
        values_format: spec.values_format.clone(),
        polar,
        radians: spec.radians,
        horizontal: spec.horizontal,
        bold_categories,
        cat_notes,
        jitter: spec.jitter,
        ybreak: spec.ybreak.as_ref().map(|b| b.value),
    };
    if let Some(b) = &spec.ybreak {
        let no = |why: &str| Err(Error::at(format!("ybreak {why}"), b.span.clone()));
        if panel.logy {
            return no("skips part of a plain axis: it can't be combined with logy");
        }
        if panel.polar || panel.horizontal || !panel.ycategories.is_empty() {
            return no("works on an upright number axis (not polar, horizontal or heatmap rows)");
        }
        if panel.has_right_axis() || panel.y2conv.is_some() {
            return no("doesn't work with a right-hand axis yet: plot the left axis alone");
        }
        let (lo, hi) = crate::render::left_y_range(&panel);
        let (a, z) = b.value;
        if !(lo < a && z < hi) {
            use crate::data::short;
            return no(&format!(
                "must lie inside the y axis, which runs from {} to {} here",
                short(lo),
                short(hi)
            ));
        }
    }
    Ok(panel)
}

/// A polynomial (coefficients from the constant up) written out for the Terminal:
/// `y = 2.31 + 0.0042·x`.
fn poly_text(coef: &[f64]) -> String {
    let mut s = String::from("y = ");
    for (k, c) in coef.iter().enumerate() {
        let v = data::short(c.abs());
        let x = match k {
            0 => String::new(),
            1 => "·x".to_owned(),
            k => format!("·x^{k}"),
        };
        match (k, *c < 0.0) {
            (0, false) => s.push_str(&v),
            (0, true) => s.push_str(&format!("−{v}")),
            (_, false) => s.push_str(&format!(" + {v}{x}")),
            (_, true) => s.push_str(&format!(" − {v}{x}")),
        }
    }
    s
}

/// `a` mixed with `b`: 0 is all `a`, 1 all `b`.
fn mix(a: Color, b: Color, t: f64) -> Color {
    let m = |x: u8, y: u8| (f64::from(x) * (1.0 - t) + f64::from(y) * t).round() as u8;
    Color(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// At most this many crossings are ringed for one pair of curves or one level (noisy data
/// can cross hundreds of times); the Terminal still gets the count.
const MAX_CROSSINGS: usize = 12;

/// One axis of a long-form grid: positions, labels, each row's place, and the column name.
type GridAxis = (Vec<f64>, Vec<String>, Vec<usize>, String);

type YColumns = Vec<(Col, std::ops::Range<usize>)>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlations() {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let y = [82.5, 68.723, 68.723, 66.83, 67.325, 54.924, 30.253];
        // By hand: Spearman on ranks 7, 5.5, 5.5, 3, 4, 2, 1 → −0.95499.
        let (rs, n) = correlation(&x, &y, Corr::Spearman).unwrap();
        assert!((rs + 0.954_994).abs() < 1e-5, "{rs}");
        assert_eq!(n, 7);
        let (r, _) =
            correlation(&x, &[2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0], Corr::Pearson).unwrap();
        assert!((r - 1.0).abs() < 1e-12);
        assert!(correlation(&x[..2], &y[..2], Corr::Pearson).is_none());
        assert!(correlation(&x, &[1.0; 7], Corr::Pearson).is_none());
    }

    #[test]
    fn wildcards_match_names() {
        assert!(wildcard("run_*.csv", "run_a.csv"));
        assert!(wildcard("run_?.csv", "run_b.csv"));
        assert!(wildcard("*", "anything.csv"));
        assert!(!wildcard("run_?.csv", "run_ab.csv"));
        assert!(!wildcard("run_*.csv", "run_a.tsv"));
    }
}
