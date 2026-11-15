//! `PlotSpec`: the complete description of one figure, as written in a command.
//!
//! Both input modes (pickers and typed commands) end up here. It still refers to files and
//! columns by the names the owner typed; `figure::build` resolves them against real data.

use std::ops::Range;

/// A value together with where it was written in the command (for error messages).
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlotSpec {
    pub panels: Vec<PanelSpec>,
    pub figure: FigureOpts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanelSpec {
    pub kind: PlotKind,
    pub layers: Vec<LayerSpec>,
    pub title: Option<String>,
    pub xlabel: Option<String>,
    pub ylabel: Option<String>,
    pub xlim: (Option<f64>, Option<f64>),
    pub ylim: (Option<f64>, Option<f64>),
    /// Tick steps, the third part of `xlim=0:100:20` (and `ylim=`, `y2lim=`).
    pub xstep: Option<Spanned<f64>>,
    pub ystep: Option<Spanned<f64>>,
    pub y2step: Option<Spanned<f64>>,
    /// Reference lines across the plot (`hline=0`, `vline=12.5,20`) and shaded regions
    /// (`span=2:5` across x, `yspan=20:25` across y).
    pub hlines: Vec<f64>,
    pub vlines: Vec<f64>,
    pub spans: Vec<(f64, f64)>,
    pub yspans: Vec<(f64, f64)>,
    /// The same on the second axes: `hline2=` / `yspan2=` in right-axis units, `vline2=`
    /// in top-axis (`x2=`) units.
    pub hlines2: Vec<f64>,
    pub vlines2: Vec<f64>,
    pub yspans2: Vec<(f64, f64)>,
    /// Labels on reference lines and regions (`hline=20:"limit"`, `span=2:5:"heating"`).
    pub ref_labels: Vec<RefLabel>,
    /// Text at a point (`note="peak"@12,30`), with an arrow (`arrow="peak"@12,30`).
    pub notes: Vec<Note>,
    /// A second x axis along the top: its values from a formula of `x` (`x2="1240/x"`),
    /// and its label (`x2label=`).
    pub x2: Option<Spanned<String>>,
    pub x2label: Option<String>,
    /// A right-hand y axis in other units of the left one (`y2="y*1.8+32"`); its label is
    /// `y2label=`.
    pub y2: Option<Spanned<String>>,
    /// `rect=`, `circle=`, `segment=`: shapes drawn on the data.
    pub shapes: Vec<Shape>,
    /// `inset=5:8` (or `5:8:20:25` with a y range): a zoomed copy inside the plot.
    pub inset: Option<Spanned<Inset>>,
    pub legend: Option<LegendPos>,
    pub grid: Option<bool>,
    /// `grid=x` / `grid=y`: grid lines along one axis only (None: both).
    pub grid_axis: Option<GridLines>,
    pub logx: bool,
    pub logy: bool,
    /// `logy2`: the right-hand y axis is logarithmic.
    pub logy2: bool,
    /// `axiscolor=off`: a second y axis stays black even with one curve per side.
    pub axis_color: bool,
    /// Label and limits of the right-hand y axis (`axis=right` layers).
    pub y2label: Option<String>,
    pub y2lim: (Option<f64>, Option<f64>),
    /// `hist`: number of bins (None = chosen from the data).
    pub bins: Option<usize>,
    /// `stacked`: bars, areas and histograms sit on top of each other.
    pub stacked: bool,
    /// `density`: histograms show density (area 1) instead of counts.
    pub density: bool,
    /// Colour map for heatmaps and contours (`cmap=viridis`).
    pub cmap: Option<String>,
    /// Colour range (`zlim=0:1`) and colour bar label (`zlabel=`).
    pub zlim: (Option<f64>, Option<f64>),
    pub zlabel: Option<String>,
    /// Contours: about this many levels (`levels=10`); `filled` fills between them.
    pub levels: Option<usize>,
    pub filled: bool,
    /// `values`: write each value in its heatmap cell, above its bar or point;
    /// `values=%.1f` (or `values=1`) sets the decimals, with text around it kept
    /// (`values=%.0f%`).
    pub values: bool,
    pub values_format: Option<NumberFormat>,
    /// Where fitted equations go (`fitlabel=`) and their significant figures
    /// (`fitdigits=`).
    pub fit_label: FitLabel,
    pub fit_digits: Option<usize>,
    /// `fitnames=labels`: the equation names x and y after `xlabel=` / `ylabel=` instead of
    /// the columns (the default, `fitnames=columns`).
    pub fit_names: FitNames,
    /// `corr` / `corr=spearman`: the correlation of each scatter's x and y in a corner.
    pub corr: Option<Corr>,
    /// `summary` (was `stats`): mean ± sd, median and N in each series' legend entry, or
    /// under each group of a box, strip or violin.
    pub stats: bool,
    /// `residuals`: a strip under the plot with each point's distance from its fit.
    pub residuals: bool,
    /// Reading numbers off the curves (see `analysis.rs`): `peaks` / `peaks=3`, `dips`,
    /// `onset` (with the baseline's x range: `onset=20:40`) and `area=2:8`.
    pub peaks: Option<Spanned<usize>>,
    pub dips: Option<Spanned<usize>>,
    pub onset: Option<Spanned<Option<(f64, f64)>>>,
    pub area: Option<Spanned<(f64, f64)>>,
    /// `peaks=storage`, `dips=storage,loss`, `onset=loss`: only those curves (by their legend name);
    /// empty: every curve of the panel.
    pub read_on: ReadOn,
    /// kde / violin: smoothing width in data units (`bw=`); None = Scott's rule.
    pub bw: Option<f64>,
    /// Polar: angles in radians (`angle=rad`) instead of degrees.
    pub radians: bool,
    /// fft: what the y axis shows (`scale=`) and the window (`window=`).
    pub fft_scale: FftScale,
    pub fft_window: FftWindow,
    /// `horizontal`: bars, boxes, violins and strips lie along the x axis, the groups
    /// down the y axis (first at the top).
    pub horizontal: bool,
    /// `highlight="Bed temperature"`: those bars or groups keep their colour, the rest
    /// turn grey (a list for several).
    pub highlight: Option<Spanned<Vec<String>>>,
    /// `mean` / `mean=label`: a line at each strip group's mean (with its value).
    pub mean: Option<Spanned<MeanMark>>,
    /// `median` / `median=label`: the same at each group's median (a dashed line).
    pub median: Option<Spanned<MeanMark>>,
    /// `strip jitter`: a random spread (fixed seed) instead of the beeswarm.
    pub jitter: bool,
    /// `cross`: mark where the plotted curves cross each other; `cross=50` (a list for
    /// several): where each curve crosses that y value.
    pub cross: Option<Spanned<Cross>>,
    /// `ybreak=20:80`: leave this empty range out of the y axis (a broken axis).
    pub ybreak: Option<Spanned<(f64, f64)>>,
    /// `resid … hist`: the residuals counted into a histogram (with `bins=`, `density`)
    /// instead of drawn against x.
    pub resid_hist: bool,
    /// `crosslabel=x|y|xy|off`: what is written beside each crossing (None: x).
    pub cross_label: Option<Spanned<CrossLabel>>,
    /// `fitstats` / `fitstats=tr`: each fit's numbers (parameters ± error, R², RMSE, N) in
    /// a corner (Best: the emptiest).
    pub fit_stats: Option<LegendPos>,
}

/// Which curves `peaks=` / `dips=` / `onset=` read (names as in the legend).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadOn {
    pub peaks: Vec<Spanned<String>>,
    pub dips: Vec<Spanned<String>>,
    pub onset: Vec<Spanned<String>>,
}

/// What a fitted equation calls x and y (`fitnames=`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FitNames {
    /// The column names: `yield = 89 − 13.2·dose`.
    #[default]
    Columns,
    /// The axis labels, units in brackets left out: `test = 89 − 13.2·D`.
    Labels,
}

/// `cross=curves` (curves with each other) or `cross=50,80` (each curve with these y values).
#[derive(Debug, Clone, PartialEq)]
pub enum Cross {
    Curves,
    Levels(Vec<f64>),
}

/// `crosslabel=`: what a crossing's label shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossLabel {
    X,
    Y,
    Xy,
    Off,
}

impl CrossLabel {
    /// The text beside a crossing at (`x`, `y`).
    pub fn text(self, x: f64, y: f64) -> String {
        use crate::data::short;
        match self {
            CrossLabel::X => short(x),
            CrossLabel::Y => short(y),
            CrossLabel::Xy => format!("({}, {})", short(x), short(y)),
            CrossLabel::Off => String::new(),
        }
    }
}

/// `baseline=0:5,90:100` / `baseline=poly2,0:5,90:100`: a polynomial fitted over these x
/// ranges (none: the first and last tenth of the x range) and taken off each y column.
#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    /// 0 (`const`), 1 (`linear`, the default), 2 (`poly2`) or 3 (`poly3`).
    pub degree: usize,
    pub ranges: Vec<(f64, f64)>,
    pub span: Range<usize>,
}

/// `norm=`: each curve scaled so its largest value (`max`), first value (`first`) or area
/// (`area`) is 1, or so it runs from 0 to 1 (`range`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Norm {
    Max,
    First,
    Area,
    Range,
}

impl Norm {
    pub fn from_name(s: &str) -> Option<Norm> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "max" | "peak" | "top" => Norm::Max,
            "first" | "start" => Norm::First,
            "area" | "integral" => Norm::Area,
            "range" | "minmax" | "0:1" | "01" => Norm::Range,
            _ => return None,
        })
    }
}

/// `mean` (a line at each group's mean) or `mean=label` (with the value beside it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeanMark {
    Line,
    Label,
}

/// Which kind of reference mark a label belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    HLine,
    VLine,
    Span,
    YSpan,
}

/// A label on a reference line (`at.0`) or region (`at.0`..`at.1`).
#[derive(Debug, Clone, PartialEq)]
pub struct RefLabel {
    pub kind: RefKind,
    pub at: (f64, f64),
    pub text: String,
    /// In the second axis's units (`hline2=`, `yspan2=`: right; `vline2=`: top).
    pub second: bool,
}

/// A shape on the data, in data units.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// `rect=2:5:20:25` (x from 2 to 5, y from 20 to 25), optionally `:"label"`.
    Rect {
        x: (f64, f64),
        y: (f64, f64),
        label: Option<String>,
    },
    /// `circle=@3.5,25.6` or `circle="peak"@3.5,25.6`: a ring around a point (its size is
    /// fixed on the page, so it stays round).
    Circle {
        x: f64,
        y: f64,
        label: Option<String>,
    },
    /// `segment=0,20:10,30`: a straight line between two points.
    Segment { from: (f64, f64), to: (f64, f64) },
}

/// A zoomed copy of part of the plot, drawn inside it (`inset=5:8`, `inset_at=bl`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inset {
    pub x: (f64, f64),
    /// None: fitted to the data in the x range.
    pub y: Option<(f64, f64)>,
    /// A corner (`tr`, `tl`, `br`, `bl`); None: the emptiest one.
    pub at: Option<LegendPos>,
}

/// fft's y axis: size of each frequency, power per unit of frequency, or decibels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FftScale {
    #[default]
    Amplitude,
    Power,
    Db,
}

/// fft's window: Hann (smooth ends, the default) or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FftWindow {
    #[default]
    Hann,
    None,
}

/// Text placed at a point in data units, optionally with an arrow pointing at it.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub arrow: bool,
    /// `note="text"@tr`: in a corner of the plot instead of at a point.
    pub corner: Option<LegendPos>,
}

/// `grid=x` / `grid=y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridLines {
    /// Vertical lines at the x ticks.
    X,
    /// Horizontal lines at the y ticks.
    Y,
}

/// How a value written on the plot looks: `decimals` after the point (None: up to three
/// significant figures), with `before` and `after` around it (`values=%.0f%` → `41%`).
/// A value that rounds to zero but isn't zero is written as `<1%` (or `<0.1`…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberFormat {
    pub decimals: Option<usize>,
    pub before: String,
    pub after: String,
    /// `values=%.0f+-`: write the error bar's size after the value (`41 ± 3`, or
    /// `41 (36–47)` when the bar is longer one way).
    pub error: bool,
}

impl NumberFormat {
    /// `%.1f`, `%.0f%`, `±%.2f`, `%.1f+-` (with the error), or a plain number of decimals (`1`).
    pub fn parse(s: &str) -> Option<NumberFormat> {
        if let Ok(d) = s.trim().parse::<usize>() {
            return (d <= 10).then(|| NumberFormat {
                decimals: Some(d),
                before: String::new(),
                after: String::new(),
                error: false,
            });
        }
        let at = s.find('%')?;
        let (before, rest) = (&s[..at], &s[at + 1..]);
        let (decimals, after) = if let Some(r) = rest.strip_prefix('.') {
            let digits: String = r.chars().take_while(char::is_ascii_digit).collect();
            let r = &r[digits.len()..];
            let r = r.strip_prefix(['f', 'g']).unwrap_or(r);
            (Some(digits.parse::<usize>().ok().filter(|d| *d <= 10)?), r)
        } else {
            let r = rest.strip_prefix(['f', 'g'])?;
            (None, r)
        };
        // `+-` (easy to type) or `±` right after the number asks for the error too.
        let (error, after) = match after.strip_prefix("+-").or_else(|| after.strip_prefix('±')) {
            Some(rest) => (true, rest),
            None => (false, after),
        };
        Some(NumberFormat {
            decimals,
            before: before.to_owned(),
            after: after.to_owned(),
            error,
        })
    }
}

/// `fitlabel=`: where a fitted equation is written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitLabel {
    /// In the legend (the default).
    Legend,
    /// In a corner of the plot (`fitlabel=tr`).
    Corner(LegendPos),
    /// At a point in data units (`fitlabel=@20,50`).
    At(f64, f64),
    /// Not written (`fitlabel=off`); the numbers still go to the Terminal.
    Off,
}

/// `corr` (Pearson's r) or `corr=spearman` (Spearman's r_s, on ranks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corr {
    Pearson,
    Spearman,
}

impl PanelSpec {
    pub fn new(kind: PlotKind) -> PanelSpec {
        PanelSpec {
            kind,
            layers: Vec::new(),
            title: None,
            xlabel: None,
            ylabel: None,
            xlim: (None, None),
            ylim: (None, None),
            xstep: None,
            ystep: None,
            y2step: None,
            hlines: Vec::new(),
            vlines: Vec::new(),
            spans: Vec::new(),
            yspans: Vec::new(),
            hlines2: Vec::new(),
            vlines2: Vec::new(),
            yspans2: Vec::new(),
            ref_labels: Vec::new(),
            notes: Vec::new(),
            x2: None,
            x2label: None,
            y2: None,
            inset: None,
            shapes: Vec::new(),
            legend: None,
            grid: None,
            grid_axis: None,
            logx: false,
            logy: false,
            logy2: false,
            axis_color: true,
            y2label: None,
            y2lim: (None, None),
            bins: None,
            stacked: false,
            density: false,
            cmap: None,
            zlim: (None, None),
            zlabel: None,
            levels: None,
            filled: false,
            values: false,
            values_format: None,
            fit_label: FitLabel::Legend,
            fit_digits: None,
            fit_names: FitNames::Columns,
            corr: None,
            stats: false,
            read_on: ReadOn::default(),
            residuals: false,
            peaks: None,
            dips: None,
            onset: None,
            area: None,
            bw: None,
            fft_scale: FftScale::Amplitude,
            fft_window: FftWindow::Hann,
            radians: false,
            horizontal: false,
            highlight: None,
            mean: None,
            median: None,
            jitter: false,
            cross: None,
            cross_label: None,
            resid_hist: false,
            ybreak: None,
            fit_stats: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerSpec {
    pub kind: PlotKind,
    pub file: Spanned<String>,
    pub x: Option<Spanned<String>>,
    /// `x=t1,t2 y=a,b`: more x columns, paired with the y columns in order. Only while
    /// parsing: the layer is then split into one layer per pair.
    pub pair_x: Vec<Spanned<String>>,
    pub y: Vec<Spanned<String>>,
    /// Labels, colours, line styles and markers: one for all columns, or a list in the
    /// order of the y columns (`color=orange,blue`); missing ones fall back to defaults.
    pub label: Vec<String>,
    pub color: Vec<Color>,
    pub style: Vec<LineStyle>,
    pub width: Option<f64>,
    pub marker: Vec<Marker>,
    pub alpha: Option<f64>,
    /// `line markers`: draw markers on the line too.
    pub markers: bool,
    /// `where="temp > 20"`: keep only the rows that match.
    pub filter: Option<Spanned<String>>,
    /// `err=` / `yerr=`: error bar columns (one, or one per y column).
    pub err: Vec<Spanned<String>>,
    /// The downward lengths when the bars aren't the same both ways (set by summaries
    /// for `err=iqr`; `err` then holds the upward ones). Not written in commands.
    pub err_low: Vec<Spanned<String>>,
    /// `xerr=`: horizontal error bars.
    pub xerr: Option<Spanned<String>>,
    /// `axis=right`: measured on a second y axis on the right; a list (`axis=left,right`)
    /// gives each y column its own (like `color=`).
    pub right: Vec<bool>,
    /// `fit=linear` etc.: draw a fitted curve.
    pub fit: Option<Fit>,
    /// `stat=mean|median`: one value per file (or per group of `x=`), with error bars
    /// from `err=sd|sem|ci95|none` (default sd).
    pub stat: Option<Stat>,
    /// Your own formula (`fit="a*exp(-x/b)+c"`) and how fits are done and shown.
    pub fitting: FitSettings,
    /// `step=pre|mid|post` for step plots.
    pub step: StepWhere,
    /// `area … y=low,high between`: fill between two columns.
    pub between: bool,
    /// `heatmap`/`contour` in long form: the value column (`z=`).
    pub z: Option<Spanned<String>>,
    /// `scatter … c=temp`: colour each point by this number column (with a colour bar).
    pub c: Option<Spanned<String>>,
    /// `smooth=5`: each y column becomes the average of 5 rows around each row.
    pub smooth: Option<usize>,
    /// `err=sd band`: a shaded band of ±error around the line instead of error bars.
    pub band: bool,
    /// How the file is read: `skiplines=` (lines before the header) and `sheet=` (workbooks).
    pub read: crate::data::ReadOptions,
    /// `skiprows=10`: leave out the first data rows; `every=5`: keep every 5th row.
    pub skiprows: usize,
    pub every: usize,
    /// `clip=0:100`: y values outside are treated as missing.
    pub clip: Option<(f64, f64)>,
    /// `dropna`: leave out rows with a blank in any drawn column.
    pub dropna: bool,
    /// `slope` (dy/dx) or `integral` (running area): plotted instead of each y column.
    pub derive: Derive,
    /// `by=bed`: one series per value of this column, each with its own colour and
    /// marker; `label=`, `color=`, `marker=` and `dash=` lists follow the groups.
    pub by: Option<Spanned<String>>,
    /// `groups=Cold,Hot`: keep only these groups (of `by=`, or of a text x column), in
    /// this order.
    pub groups: Option<Spanned<Vec<String>>>,
    /// Set while building: this layer is one group of a `by=` layer.
    pub group: Option<GroupOf>,
    /// `baseline=`: a fitted baseline taken off each y column (before slope/integral).
    pub baseline: Option<Baseline>,
    /// `showbaseline`: draw the data as it is with its fitted baseline (dashed) and the
    /// fitted ranges shaded, instead of taking it off; for checking the ranges.
    pub show_baseline: bool,
    /// `norm=max|first|area|range`: scale each curve (after kde, fft, slope…).
    pub norm: Option<Spanned<Norm>>,
}

/// One group of a `by=` layer: the rows whose `column` holds `value`.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupOf {
    pub column: Spanned<String>,
    pub value: String,
    /// The label came from `label=` (not the group's own name).
    pub labelled: bool,
}

/// What a layer plots instead of its y columns: the values themselves, their slope
/// against x (`slope`), or the running area under them (`integral`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Derive {
    #[default]
    None,
    Slope,
    Integral,
}

/// The setting for column `k` of `n`: a single value applies to every column, a list goes
/// in column order (a missing entry means "the default").
pub fn per_column<T: Clone>(list: &[T], k: usize) -> Option<T> {
    match list {
        [one] => Some(one.clone()),
        _ => list.get(k).cloned(),
    }
}

impl LayerSpec {
    /// Is y column `k` on the right axis? One `axis=` applies to all columns.
    pub fn right_at(&self, k: usize) -> bool {
        match self.right.as_slice() {
            [one] => *one,
            list => list.get(k).copied().unwrap_or(false),
        }
    }

    pub fn new(kind: PlotKind, file: Spanned<String>) -> LayerSpec {
        LayerSpec {
            kind,
            file,
            x: None,
            pair_x: Vec::new(),
            y: Vec::new(),
            label: Vec::new(),
            color: Vec::new(),
            style: Vec::new(),
            width: None,
            marker: Vec::new(),
            alpha: None,
            markers: false,
            filter: None,
            err: Vec::new(),
            err_low: Vec::new(),
            xerr: None,
            right: Vec::new(),
            fit: None,
            stat: None,
            fitting: FitSettings::default(),
            step: StepWhere::Post,
            between: false,
            z: None,
            c: None,
            smooth: None,
            band: false,
            read: Default::default(),
            skiprows: 0,
            every: 1,
            clip: None,
            dropna: false,
            derive: Derive::None,
            by: None,
            groups: None,
            group: None,
            baseline: None,
            show_baseline: false,
            norm: None,
        }
    }
}

/// Where a step plot changes level: at the point (`pre`: the step comes before it), after
/// it (`post`, the default: each value holds until the next one) or half-way (`mid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepWhere {
    Pre,
    Mid,
    Post,
}

impl StepWhere {
    pub fn from_name(s: &str) -> Option<StepWhere> {
        Some(match s.to_ascii_lowercase().as_str() {
            "pre" | "before" => StepWhere::Pre,
            "mid" | "middle" => StepWhere::Mid,
            "post" | "after" => StepWhere::Post,
            _ => return None,
        })
    }
}

/// How a fit is done and shown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FitSettings {
    /// `fit="a*exp(-x/b)+c"`: a formula of x instead of a named fit.
    pub formula: Option<Spanned<String>>,
    /// `fitband`: shade the 95% confidence band of the curve.
    pub band: bool,
    /// `through0`: polynomials without a constant term.
    pub through_zero: bool,
    /// `weighted`: points count by 1/error² (needs `err=`).
    pub weighted: bool,
    /// `fitrange=2:8`: fit (and draw) only over this x range.
    pub range: Option<(f64, f64)>,
    /// `start=a:1,b:10`: starting guesses for a formula's letters.
    pub start: Vec<(String, f64)>,
}

/// Summary plots (`stat=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    Mean,
    Median,
}

impl Stat {
    pub fn name(self) -> &'static str {
        match self {
            Stat::Mean => "mean",
            Stat::Median => "median",
        }
    }
}

/// Fitted curves (`fit=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Polynomial of this degree (1 = straight line).
    Poly(usize),
    /// y = a·e^(bx)
    Exp,
    /// y = a + b·ln x
    Log,
    /// y = a·x^b
    Power,
}

impl Fit {
    pub const NAMES: [&'static str; 7] =
        ["linear", "poly2", "poly3", "poly4", "exp", "log", "power"];

    pub fn from_name(s: &str) -> Option<Fit> {
        Some(match s.to_ascii_lowercase().as_str() {
            "linear" | "lin" | "line" | "poly1" => Fit::Poly(1),
            "poly2" | "quadratic" => Fit::Poly(2),
            "poly3" | "cubic" => Fit::Poly(3),
            "poly4" => Fit::Poly(4),
            "poly5" => Fit::Poly(5),
            "exp" | "exponential" => Fit::Exp,
            "log" | "ln" | "logarithmic" => Fit::Log,
            "power" | "pow" => Fit::Power,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FigureOpts {
    /// `--out=NAME`: file name (or relative path) inside the output folder.
    pub out: Option<Spanned<String>>,
    pub format: Option<Format>,
    /// Width and height in points (1/72 inch).
    pub size: Option<(f64, f64)>,
    /// `--size=acs`: the journal size's name, kept so it brings its font size along and a
    /// saved style keeps the name.
    pub size_name: Option<String>,
    pub dpi: Option<f64>,
    pub theme: Option<String>,
    pub font: Option<String>,
    /// `--style=NAME`: a style file in the styles folder.
    pub style: Option<Spanned<String>>,
    /// `--grid=ROWSxCOLS` for several panels.
    pub grid: Option<(usize, usize)>,
    pub sharex: bool,
    pub sharey: bool,
    /// A title over all panels.
    pub suptitle: Option<String>,
    /// `--letters`: (a), (b), (c)… at each panel's top left.
    pub letters: bool,
    /// `--widths=2,1`: relative widths of the panel columns (empty: all the same).
    pub widths: Vec<f64>,
    /// `--heights=3,1`: relative heights of the panel rows.
    pub heights: Vec<f64>,
    /// `--legend=bottom|right|top`: one legend for the whole figure instead of one in
    /// each panel; `--legend=off`: each panel its own. None: figures of several panels
    /// get one on top, unless a panel says `legend=` itself.
    pub legend: Option<SharedLegend>,
}

/// Where the figure's one shared legend goes (`--legend=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedLegend {
    Bottom,
    Right,
    Top,
    /// `--legend=off`: no shared legend; each panel keeps its own.
    Panels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlotKind {
    Line,
    Scatter,
    Bar,
    Hist,
    Step,
    Area,
    Box,
    Heatmap,
    Contour,
    Kde,
    Violin,
    Polar,
    Fft,
    Strip,
    /// Each point's distance from its fit (needs `fit=`), around a zero line.
    Resid,
    /// Normal Q–Q plot of the residuals: sorted residuals against normal quantiles.
    Qq,
}

impl PlotKind {
    pub const ALL: [PlotKind; 16] = [
        PlotKind::Line,
        PlotKind::Scatter,
        PlotKind::Bar,
        PlotKind::Hist,
        PlotKind::Step,
        PlotKind::Area,
        PlotKind::Box,
        PlotKind::Heatmap,
        PlotKind::Contour,
        PlotKind::Kde,
        PlotKind::Violin,
        PlotKind::Polar,
        PlotKind::Fft,
        PlotKind::Strip,
        PlotKind::Resid,
        PlotKind::Qq,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PlotKind::Line => "line",
            PlotKind::Scatter => "scatter",
            PlotKind::Bar => "bar",
            PlotKind::Hist => "hist",
            PlotKind::Step => "step",
            PlotKind::Area => "area",
            PlotKind::Box => "box",
            PlotKind::Heatmap => "heatmap",
            PlotKind::Contour => "contour",
            PlotKind::Kde => "kde",
            PlotKind::Violin => "violin",
            PlotKind::Polar => "polar",
            PlotKind::Fft => "fft",
            PlotKind::Strip => "strip",
            PlotKind::Resid => "resid",
            PlotKind::Qq => "qq",
        }
    }

    /// What it draws, for suggestions and the picker.
    pub fn describe(self) -> &'static str {
        match self {
            PlotKind::Line => "lines through the points",
            PlotKind::Scatter => "points only",
            PlotKind::Bar => "bars from zero",
            PlotKind::Hist => "histogram of a column",
            PlotKind::Step => "steps between the points",
            PlotKind::Area => "filled down to zero",
            PlotKind::Box => "box plot: median, quartiles, outliers",
            PlotKind::Heatmap => "a table of numbers as coloured cells",
            PlotKind::Contour => "lines of equal value over a grid",
            PlotKind::Kde => "smooth density of a column",
            PlotKind::Violin => "box plot with the density's shape",
            PlotKind::Polar => "angle and radius on a round plot",
            PlotKind::Fft => "spectrum: how strong each frequency is",
            PlotKind::Strip => "every value as a dot, per group",
            PlotKind::Resid => "residuals: each point's distance from its fit (fit=)",
            PlotKind::Qq => "normal Q–Q plot of a fit's residuals (fit=)",
        }
    }

    /// Types that show a fit's residuals instead of the data (they need `fit=`).
    pub fn is_residual(self) -> bool {
        matches!(self, PlotKind::Resid | PlotKind::Qq)
    }

    /// Types drawn per group along a category axis, which `horizontal` can turn sideways.
    pub fn is_grouped(self) -> bool {
        matches!(
            self,
            PlotKind::Bar | PlotKind::Box | PlotKind::Violin | PlotKind::Strip
        )
    }

    /// Types drawn from a grid of values (`z`), with a colour bar.
    pub fn is_grid(self) -> bool {
        matches!(self, PlotKind::Heatmap | PlotKind::Contour)
    }

    pub fn from_name(s: &str) -> Option<PlotKind> {
        PlotKind::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(s))
    }
}

/// Plot types that are planned but not built yet, with the milestone that brings them.
pub const PLANNED_KINDS: &[(&str, &str)] = &[];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Pdf,
    Svg,
}

impl Format {
    pub fn from_name(s: &str) -> Option<Format> {
        match s.to_ascii_lowercase().as_str() {
            "png" => Some(Format::Png),
            "pdf" => Some(Format::Pdf),
            "svg" => Some(Format::Svg),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Pdf => "pdf",
            Format::Svg => "svg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegendPos {
    Best,
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
    /// Outside the plot, on the right.
    Out,
    Off,
}

impl LegendPos {
    pub fn from_name(s: &str) -> Option<LegendPos> {
        Some(match s.to_ascii_lowercase().as_str() {
            "best" | "auto" => LegendPos::Best,
            "tr" | "topright" | "upper right" => LegendPos::TopRight,
            "tl" | "topleft" | "upper left" => LegendPos::TopLeft,
            "br" | "bottomright" | "lower right" => LegendPos::BottomRight,
            "bl" | "bottomleft" | "lower left" => LegendPos::BottomLeft,
            "out" | "outside" | "right" => LegendPos::Out,
            "off" | "none" | "no" => LegendPos::Off,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    DashDot,
}

impl LineStyle {
    /// The order lines get patterns in when none is given, so several lines differ
    /// without colour too (a single line stays solid).
    pub const CYCLE: [LineStyle; 4] = [
        LineStyle::Solid,
        LineStyle::Dashed,
        LineStyle::Dotted,
        LineStyle::DashDot,
    ];

    pub fn from_name(s: &str) -> Option<LineStyle> {
        Some(match s.to_ascii_lowercase().as_str() {
            "solid" | "-" => LineStyle::Solid,
            "dashed" | "--" => LineStyle::Dashed,
            "dotted" | ":" => LineStyle::Dotted,
            "dashdot" | "-." => LineStyle::DashDot,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Circle,
    Square,
    Triangle,
    /// Pointing down (`v`).
    TriangleDown,
    Cross,
    Plus,
    Diamond,
    /// Outlines only (`open-circle` …), the usual second set in journals.
    OpenCircle,
    OpenSquare,
    OpenTriangle,
    OpenTriangleDown,
    OpenDiamond,
    None,
}

impl Marker {
    /// The order series get shapes in when none is given: filled shapes first, then the
    /// same outlined, then the thin ones. Each series looks different even in grey print
    /// or to colour-blind readers.
    pub const CYCLE: [Marker; 12] = [
        Marker::Circle,
        Marker::Square,
        Marker::Triangle,
        Marker::Diamond,
        Marker::TriangleDown,
        Marker::OpenCircle,
        Marker::OpenSquare,
        Marker::OpenTriangle,
        Marker::OpenDiamond,
        Marker::OpenTriangleDown,
        Marker::Cross,
        Marker::Plus,
    ];

    /// `o`, `circle`, `open-circle` (also `circle-open`, `open circle`), `v`, ….
    pub fn from_name(s: &str) -> Option<Marker> {
        let s = s.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        let open_base = s
            .strip_prefix("open-")
            .or_else(|| s.strip_suffix("-open"))
            .or_else(|| s.strip_prefix("hollow-"));
        if let Some(base) = open_base {
            return Some(match Marker::from_name(base)? {
                Marker::Circle => Marker::OpenCircle,
                Marker::Square => Marker::OpenSquare,
                Marker::Triangle => Marker::OpenTriangle,
                Marker::TriangleDown => Marker::OpenTriangleDown,
                Marker::Diamond => Marker::OpenDiamond,
                _ => return None,
            });
        }
        Some(match s.as_str() {
            "o" | "circle" => Marker::Circle,
            "s" | "square" | "rect" => Marker::Square,
            "^" | "triangle" | "triangle-up" | "up" => Marker::Triangle,
            "v" | "triangle-down" | "down" => Marker::TriangleDown,
            "x" | "cross" => Marker::Cross,
            "+" | "plus" => Marker::Plus,
            "d" | "diamond" => Marker::Diamond,
            "none" | "off" => Marker::None,
            _ => return None,
        })
    }

    /// The name written into style files and suggested in the launcher.
    pub fn name(self) -> &'static str {
        match self {
            Marker::Circle => "o",
            Marker::Square => "s",
            Marker::Triangle => "^",
            Marker::TriangleDown => "v",
            Marker::Cross => "x",
            Marker::Plus => "+",
            Marker::Diamond => "d",
            Marker::OpenCircle => "open-circle",
            Marker::OpenSquare => "open-square",
            Marker::OpenTriangle => "open-triangle",
            Marker::OpenTriangleDown => "open-triangle-down",
            Marker::OpenDiamond => "open-diamond",
            Marker::None => "none",
        }
    }

    /// Outlines only (open shapes, cross, plus).
    pub fn is_outline(self) -> bool {
        matches!(
            self,
            Marker::Cross
                | Marker::Plus
                | Marker::OpenCircle
                | Marker::OpenSquare
                | Marker::OpenTriangle
                | Marker::OpenTriangleDown
                | Marker::OpenDiamond
        )
    }

    /// The filled shape an open one is drawn from.
    pub fn solid(self) -> Marker {
        match self {
            Marker::OpenCircle => Marker::Circle,
            Marker::OpenSquare => Marker::Square,
            Marker::OpenTriangle => Marker::Triangle,
            Marker::OpenTriangleDown => Marker::TriangleDown,
            Marker::OpenDiamond => Marker::Diamond,
            m => m,
        }
    }
}

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// A colour name from `NAMED_COLORS` (any case, spaces and dashes ignored) or `#rgb` /
    /// `#rrggbb`.
    pub fn parse(s: &str) -> Option<Color> {
        let key: String = s
            .chars()
            .filter(|c| !matches!(c, ' ' | '-' | '_'))
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some((_, c)) = NAMED_COLORS.iter().find(|(n, _)| n.replace('-', "") == key) {
            return Some(*c);
        }
        let alias = match key.as_str() {
            "grey" => "gray",
            "lightgrey" => "lightgray",
            "darkgrey" => "darkgray",
            "skyblue" | "lightblue" => "sky",
            _ => "",
        };
        if let Some((_, c)) = NAMED_COLORS.iter().find(|(n, _)| *n == alias) {
            return Some(*c);
        }
        let hex = s.strip_prefix('#')?;
        let digit = |i: usize, len: usize| u8::from_str_radix(hex.get(i..i + len)?, 16).ok();
        match hex.len() {
            6 => Some(Color(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?)),
            3 => Some(Color(
                digit(0, 1)? * 17,
                digit(1, 1)? * 17,
                digit(2, 1)? * 17,
            )),
            _ => None,
        }
    }
}

/// Colour names for `color=`. Each looks like its name (slightly muted so it prints well);
/// the last four are the Okabe–Ito colours the automatic palette uses.
pub const NAMED_COLORS: &[(&str, Color)] = &[
    ("blue", Color(0x1f, 0x77, 0xb4)),
    ("orange", Color(0xff, 0x7f, 0x0e)),
    ("green", Color(0x2c, 0xa0, 0x2c)),
    ("red", Color(0xd6, 0x27, 0x28)),
    ("purple", Color(0x94, 0x67, 0xbd)),
    ("brown", Color(0x8c, 0x56, 0x4b)),
    ("pink", Color(0xe3, 0x77, 0xc2)),
    ("gray", Color(0x7f, 0x7f, 0x7f)),
    ("olive", Color(0x80, 0x80, 0x00)),
    ("cyan", Color(0x17, 0xbe, 0xcf)),
    ("black", Color(0x00, 0x00, 0x00)),
    ("navy", Color(0x1a, 0x2a, 0x6c)),
    ("sky", Color(0x56, 0xb4, 0xe9)),
    ("teal", Color(0x00, 0x80, 0x80)),
    ("dark-green", Color(0x1b, 0x5e, 0x20)),
    ("lime", Color(0x7c, 0xc2, 0x42)),
    ("yellow", Color(0xf2, 0xc8, 0x0f)),
    ("gold", Color(0xc9, 0x9a, 0x06)),
    ("dark-red", Color(0x8b, 0x1a, 0x1a)),
    ("magenta", Color(0xc2, 0x18, 0x8b)),
    ("violet", Color(0x7b, 0x4f, 0xd6)),
    ("light-gray", Color(0xc7, 0xc7, 0xc7)),
    ("dark-gray", Color(0x40, 0x40, 0x40)),
    ("white", Color(0xff, 0xff, 0xff)),
    ("vermillion", Color(0xd5, 0x5e, 0x00)),
    ("bluish-green", Color(0x00, 0x9e, 0x73)),
    ("reddish-purple", Color(0xcc, 0x79, 0xa7)),
    ("okabe-blue", Color(0x00, 0x72, 0xb2)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_names_look_like_their_name() {
        let red = Color::parse("red").unwrap();
        assert!(red.0 > 200 && red.1 < 60 && red.2 < 60);
        let purple = Color::parse("Purple").unwrap();
        assert!(purple.2 > purple.1 && purple.0 > purple.1);
        assert_eq!(Color::parse("dark green"), Color::parse("dark-green"));
        assert_eq!(Color::parse("grey"), Color::parse("gray"));
        assert_eq!(Color::parse("#f00"), Some(Color(255, 0, 0)));
        assert_eq!(Color::parse("mauve"), None);
    }
}
