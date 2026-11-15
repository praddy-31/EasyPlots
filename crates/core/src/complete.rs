//! Suggestions while typing a command in the launcher: plot types first, then files, then
//! columns inside `x=`/`y=`, option names and option values.
//!
//! The launcher sends the text and cursor position; this returns what could go there and
//! which part of the text a chosen suggestion replaces.

use crate::data::ReadOptions;
use crate::search::filter;
use crate::spec::PlotKind;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Completion {
    /// Byte range of the text that a suggestion replaces.
    pub start: usize,
    pub end: usize,
    pub items: Vec<Suggestion>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Suggestion {
    /// Shown in the list.
    pub label: String,
    /// Put into the text.
    pub insert: String,
    /// Grey hint on the right (column type, what an option does).
    pub detail: String,
}

/// Where completion gets files and columns from (the launcher's file index and CSV cache).
pub trait Source {
    /// File names as they should be typed, best match first.
    fn files(&mut self, query: &str, limit: usize) -> Vec<String>;
    /// Column names and kinds ("numbers", "dates", "text") of a file as typed, read with
    /// that layer's `skiplines=` and `sheet=`.
    fn columns(&mut self, file: &str, read: &ReadOptions) -> Option<Vec<(String, String)>>;
    /// Saved plots (for `again`), as they should be typed, newest first.
    fn plots(&mut self, _query: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
    /// Style file names (for `--style=`).
    fn styles(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// Presets as (name, command).
    fn presets(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
}

const LIMIT: usize = 40;

const OPTIONS: &[(&str, &str)] = &[
    ("x=", "x column (hist, kde: the values to count)"),
    ("y=", "y column(s), comma-separated"),
    ("label=", "legend text"),
    ("color=", "colour, or one per y column: orange,blue"),
    ("dash=", "solid, dashed, dotted, dashdot"),
    (
        "marker=",
        "point shape: o s ^ v d, open-circle…, x +; one per column",
    ),
    ("markers", "draw markers on the line"),
    ("where=", "keep rows, e.g. \"temp > 20\""),
    (
        "err=",
        "error bar column(s); with stat=: sd, sem, ci95 or iqr",
    ),
    ("xerr=", "horizontal error bar column"),
    ("band", "error as a shaded band around the line"),
    (
        "fit=",
        "fitted curve: linear, poly2, exp… or a formula: \"a*exp(-x/b)+c\"",
    ),
    ("fitband", "shade the fit's 95% confidence band"),
    ("through0", "fit a polynomial through 0, 0"),
    ("weighted", "fit: points count by 1/error² (err=)"),
    ("fitrange=", "fit only this x range, e.g. 2:8"),
    ("start=", "rough guesses for a formula fit, e.g. a:1,b:10"),
    (
        "residuals",
        "a strip under the plot: each point's distance from the fit",
    ),
    (
        "fitstats",
        "the fit's numbers in a corner: parameters ± error, R², RMSE, N",
    ),
    ("smooth=", "moving average over this many rows, e.g. 5"),
    (
        "baseline=",
        "take off a baseline fitted over x ranges: 0:5,90:100 (poly2,0:5,… for a curve)",
    ),
    (
        "showbaseline",
        "draw the fitted baseline instead of taking it off, to check it",
    ),
    (
        "norm=",
        "scale each curve: max (top at 1), first, area (area 1), range (0 to 1)",
    ),
    (
        "cross=",
        "curves: where the curves cross each other · or numbers: where each crosses that y (cross=50) · one or the other",
    ),
    (
        "crosslabel=",
        "what is written at each crossing: x, y, xy or off",
    ),
    (
        "hist",
        "resid: count the residuals into a histogram (with bins=)",
    ),
    (
        "ybreak=",
        "leave an empty range out of the y axis (a broken axis), e.g. 20:80",
    ),
    ("slope", "plot the slope dy/dx instead of y"),
    ("integral", "plot the running area under y instead of y"),
    (
        "peaks",
        "mark each curve's highest peak (peaks=3: the 3 most prominent; peaks=storage: only curve storage)",
    ),
    (
        "dips",
        "mark each curve's lowest dip (dips=3: the 3 deepest; dips=storage: only curve storage)",
    ),
    (
        "onset",
        "mark where a step starts (two tangents); onset=20:40 sets the flat range, onset=storage one curve",
    ),
    (
        "area=",
        "shade and measure the area under the curve, e.g. 2:8",
    ),
    (
        "skiplines=",
        "lines above the header row to ignore (a file's preamble), e.g. 3",
    ),
    (
        "skiprows=",
        "first data rows below the header to leave out (a warm-up), e.g. 10",
    ),
    ("every=", "keep every n-th row, e.g. 5"),
    (
        "clip=",
        "values outside this range count as missing, e.g. 0:100",
    ),
    ("dropna", "leave out rows with a blank in a drawn column"),
    (
        "summary",
        "writes N and mean ± sd: in the legend, or under each group (box, violin, strip, bar with stat=)",
    ),
    (
        "by=",
        "one series per value of a column, each its own colour, e.g. by=bed",
    ),
    (
        "groups=",
        "keep only these groups, in this order, e.g. groups=Cold,Hot",
    ),
    (
        "horizontal",
        "lay bars, boxes, violins or strips along x, the groups down the side",
    ),
    (
        "highlight=",
        "colour these bars or groups, the rest grey, e.g. highlight=\"Bed temperature\"",
    ),
    (
        "mean",
        "a line at each strip group's mean (mean=label: with its value)",
    ),
    (
        "median",
        "a dashed line at each strip group's median (median=label: with its value)",
    ),
    (
        "jitter",
        "strip: spread the dots at random instead of side by side",
    ),
    (
        "stat=",
        "one value per file or group: mean or median (bars with error bars)",
    ),
    ("sheet=", "workbook sheet: a name or number"),
    ("axis=", "right: a second y axis"),
    ("step=", "where steps change: pre, mid, post"),
    ("between", "area between two y columns"),
    ("stacked", "stack bars, areas or histograms"),
    ("bins=", "histogram bins, e.g. 30"),
    ("density", "histogram as density"),
    ("z=", "value column (heatmap, contour)"),
    (
        "c=",
        "scatter: colour each point by a number column, with a colour bar",
    ),
    ("cmap=", "colour map: viridis, magma, coolwarm…"),
    ("zlim=", "colour range, e.g. 0:1"),
    ("zlabel=", "colour bar label"),
    ("levels=", "number of contour levels"),
    ("filled", "filled contours"),
    (
        "values",
        "write each value on its bar, point or heatmap cell",
    ),
    (
        "values=",
        "values with these decimals, e.g. %.1f or %.0f%; %.1f+- adds the error",
    ),
    (
        "fitlabel=",
        "where the fit's equation goes: legend, tr, tl, br, bl, @x,y, off",
    ),
    (
        "fitdigits=",
        "significant figures in the fit's equation, e.g. 4",
    ),
    (
        "fitnames=",
        "the equation's names: columns (yield = … dose) or labels (from xlabel= and ylabel=)",
    ),
    (
        "corr",
        "correlation r of x and y in a corner (corr=spearman: r_s)",
    ),
    ("bw=", "smoothing width for kde and violin"),
    ("angle=", "polar angles: deg or rad"),
    ("width=", "line width in points"),
    ("alpha=", "opacity, 0–1"),
    ("title=", "plot title"),
    ("xlabel=", "x axis label"),
    ("ylabel=", "y axis label"),
    ("xlim=", "x range and tick step, e.g. 0:10 or 0:100:20"),
    ("ylim=", "y range and tick step, e.g. :5 or 0:1:0.25"),
    (
        "hline=",
        "dashed line across at these y values, e.g. 0 or 20:\"limit\"",
    ),
    (
        "vline=",
        "dashed line down at these x values (a label after :)",
    ),
    ("span=", "shaded x region, e.g. 2:5 or 2:5:\"heating\""),
    ("yspan=", "shaded y region, e.g. 20:25"),
    (
        "hline2=",
        "dashed line at a right-axis value (axis=right or y2=)",
    ),
    ("yspan2=", "shaded region in right-axis units"),
    ("vline2=", "dashed line at a top-axis (x2=) value"),
    (
        "note=",
        "text at a point or corner, e.g. note=\"start\"@0.5,20 or note=\"n = 6\"@tr",
    ),
    (
        "arrow=",
        "text with an arrow to a point, e.g. arrow=\"peak\"@12,30",
    ),
    ("x2=", "top x axis from a formula of x, e.g. \"1240/x\""),
    ("x2label=", "top x axis label"),
    ("y2=", "right y axis from a formula of y, e.g. \"y*1.8+32\""),
    (
        "rect=",
        "a rectangle: x range, y range, e.g. 2:5:20:25 (or 2:5:20:25:\"A\")",
    ),
    (
        "circle=",
        "a ring around a point, e.g. @3.5,25.6 or \"peak\"@3.5,25.6",
    ),
    (
        "segment=",
        "a straight line between two points, e.g. 0,20:10,30",
    ),
    (
        "inset=",
        "zoomed copy of an x range inside the plot, e.g. 5:8 (or 5:8:20:25)",
    ),
    (
        "inset_at=",
        "the inset's corner: tr, tl, br or bl (default: the emptiest)",
    ),
    ("scale=", "fft: amplitude (default), power or db"),
    ("window=", "fft: hann (default) or none"),
    ("logx", "logarithmic x axis"),
    ("logy", "logarithmic y axis"),
    ("logy2", "logarithmic right y axis"),
    (
        "legend=",
        "this panel's legend: best, tr, tl, br, bl, out, off",
    ),
    ("grid=", "on, off, x or y (lines along one axis only)"),
    ("y2label=", "right y axis label"),
    ("y2lim=", "right y range and tick step"),
    ("axiscolor=", "off: keep both y axes black"),
    ("+", "overlay another file"),
    (";", "start another panel"),
    ("--out=", "file name for this plot"),
    ("--format=", "png, pdf or svg"),
    (
        "--size=",
        "figure size, e.g. 6x4in, or a journal's: acs, aip, aps-double…",
    ),
    ("--dpi=", "PNG resolution"),
    ("--theme=", "paper, light, dark or palette"),
    ("--font=", "STIX Two, Latin Modern or Source Sans"),
    ("--style=", "a style file from the styles folder"),
    ("--layout=", "panel rows x columns, e.g. 2x1"),
    ("--sharex", "panels share the x axis"),
    ("--sharey", "panels share the y axis"),
    ("--suptitle=", "title over all panels"),
    ("--letters", "(a), (b)… on the panels"),
    (
        "--widths=",
        "relative widths of the panel columns, e.g. 2,1",
    ),
    ("--heights=", "relative heights of the panel rows, e.g. 3,1"),
    (
        "--legend=",
        "one legend for the whole figure: top (the default for several panels), bottom, right; off: one per panel",
    ),
];

/// Does this option do anything for this plot type? Only those are suggested, so the list
/// after `hist FILE` doesn't offer `marker=` or `cmap=` (owner's request). Typing another
/// option still works; this only trims the suggestions.
fn fits(option: &str, kind: PlotKind) -> bool {
    use PlotKind::*;
    let name = option.trim_end_matches('=');
    let lines = matches!(kind, Line | Step | Kde | Polar | Area | Fft);
    let xy = matches!(kind, Line | Scatter | Bar | Step | Area);
    let grid = matches!(kind, Heatmap | Contour);
    match name {
        "label" | "alpha" => kind != Heatmap,
        "color" => !grid,
        "style" | "dash" | "width" => lines,
        "marker" => matches!(kind, Line | Scatter | Step | Polar | Strip),
        "markers" => matches!(kind, Line | Scatter | Step | Polar),
        "err" => matches!(kind, Line | Scatter | Bar | Step),
        "xerr" => matches!(kind, Line | Scatter),
        "fit" | "fitband" | "through0" | "weighted" | "fitrange" | "start" | "fitstats"
        | "fitnames" => {
            matches!(kind, Line | Scatter | Step | Resid | Qq)
        }
        "residuals" => matches!(kind, Line | Scatter | Step),
        "baseline" | "showbaseline" => matches!(kind, Line | Scatter | Step | Area),
        "norm" => matches!(kind, Line | Scatter | Step | Area | Kde | Fft),
        "cross" | "crosslabel" => matches!(kind, Line | Scatter | Step | Area | Kde | Fft | Resid),
        "smooth" => xy || matches!(kind, Polar | Fft),
        "slope" | "integral" => matches!(kind, Line | Scatter | Step | Area),
        "peaks" | "dips" | "onset" | "area" => {
            matches!(kind, Line | Scatter | Step | Area | Kde | Fft)
        }
        "axis" | "y2label" | "y2lim" => xy,
        "step" => kind == Step,
        "band" => matches!(kind, Line | Step),
        "hline" | "vline" | "span" | "yspan" | "hline2" | "vline2" | "yspan2" | "note"
        | "arrow" | "rect" | "circle" | "segment" => kind != Polar,
        "x2" | "x2label" => !matches!(kind, Polar | Bar | Box | Violin | Strip),
        "y2" => !matches!(kind, Polar | Heatmap | Contour),
        "stat" => matches!(kind, Bar | Scatter | Line),
        "summary" => !grid && kind != Polar,
        "inset" | "inset_at" => !matches!(kind, Polar | Box | Violin | Strip),
        "by" | "groups" | "highlight" => !grid && kind != Polar,
        "horizontal" => kind.is_grouped(),
        "mean" | "median" | "jitter" => kind == Strip,
        "between" => kind == Area,
        "stacked" => matches!(kind, Bar | Area | Hist),
        "bins" | "density" => matches!(kind, Hist | Resid),
        "hist" => kind == Resid,
        "ybreak" => !grid && kind != Polar,
        "z" => grid,
        "c" => kind == Scatter,
        "cmap" | "zlim" | "zlabel" => grid || kind == Scatter,
        "levels" | "filled" => kind == Contour,
        "values" => matches!(kind, Heatmap | Bar | Line | Scatter | Step),
        "bw" => matches!(kind, Kde | Violin),
        "angle" => kind == Polar,
        "scale" | "window" => kind == Fft,
        "logx" | "logy" => !grid && kind != Polar,
        _ => true,
    }
}

/// What an option only works with, when the panel being typed doesn't have it yet:
/// `fitrange=` → "fit=". The launcher shows "needs fit=" beside the suggestion (the
/// engine would refuse the command with the same reason after ↵).
fn needs(option: &str, panel: &[&str]) -> Option<&'static str> {
    let has = |keys: &[&str]| {
        panel.iter().any(|w| {
            let w = w.to_ascii_lowercase();
            keys.iter().any(|k| w.starts_with(k))
        })
    };
    let (need, present) = match option.trim_end_matches('=') {
        "fitband" | "through0" | "weighted" | "fitrange" | "start" | "fitstats" | "fitlabel"
        | "fitdigits" | "fitnames" | "residuals" => ("fit=", has(&["fit="])),
        "showbaseline" => ("baseline=", has(&["baseline="])),
        "crosslabel" => ("cross=", has(&["cross="])),
        "y2label" | "y2lim" | "logy2" => ("axis=right", has(&["axis=", "y2="])),
        "inset_at" => ("inset=", has(&["inset="])),
        _ => return None,
    };
    (!present).then_some(need)
}

fn values(key: &str) -> Vec<&'static str> {
    if key == "color" || key == "colour" {
        return crate::spec::NAMED_COLORS.iter().map(|(n, _)| *n).collect();
    }
    let v: &[&str] = match key {
        "legend" => &["best", "tr", "tl", "br", "bl", "out", "off"],
        "norm" => &["max", "first", "area", "range"],
        "stat" => &["mean", "median"],
        "scale" => &["amplitude", "power", "db"],
        "inset_at" => &["tr", "tl", "br", "bl"],
        "window" => &["hann", "none"],
        "style" | "dash" => &["solid", "dashed", "dotted", "dashdot"],
        "marker" => &[
            "o",
            "s",
            "^",
            "v",
            "d",
            "open-circle",
            "open-square",
            "open-triangle",
            "open-triangle-down",
            "open-diamond",
            "x",
            "+",
            "none",
        ],
        "grid" => &["on", "off", "y", "x"],
        "fitlabel" => &["legend", "tr", "tl", "br", "bl", "off"],
        "corr" => &["pearson", "spearman"],
        "crosslabel" => &["x", "y", "xy", "off"],
        "cross" => &["curves"],
        "fitnames" => &["columns", "labels"],
        "mean" | "median" => &["label", "line", "off"],
        "axiscolor" => &["off", "on"],
        "values" => &["%.0f", "%.1f", "%.2f", "%.0f%", "%.1f+-"],
        "axis" => &["right", "left"],
        "fit" => &crate::spec::Fit::NAMES,
        "step" => &["post", "pre", "mid"],
        "cmap" => &crate::colormap::NAMES,
        "angle" => &["deg", "rad"],
        "levels" => &["5", "10", "20"],
        "bins" => &["auto", "10", "20", "30", "50"],
        "--layout" => &["1x2", "2x1", "2x2", "1x3", "3x1"],
        "--format" => &["png", "pdf", "svg"],
        "--legend" => &["top", "bottom", "right", "off"],
        "--widths" | "--heights" => &["2,1", "3,1", "1,2", "2,1,1"],
        "--theme" => &crate::style::THEMES,
        "--font" => &["auto", "STIX Two", "Latin Modern", "Source Sans"],
        "--size" => &[
            "6x4in",
            "15x10cm",
            "column",
            "double",
            "acs",
            "acs-double",
            "aip",
            "aip-double",
            "aps",
            "aps-double",
            "rsc",
            "rsc-double",
            "nature",
            "nature-double",
            "science",
            "science-mid",
            "science-double",
            "elsevier",
            "elsevier-mid",
            "elsevier-double",
        ],
        "--dpi" => &["150", "300", "600"],
        _ => &[],
    };
    v.to_vec()
}

/// A few values that need a word of explanation in the list.
fn value_note(key: &str, value: &str) -> &'static str {
    match (key, value) {
        ("cross", "curves") => "where the curves cross each other · or type a level: cross=50",
        ("fitnames", "columns") => "yield = 89 − 13.2·dose (the default)",
        ("fitnames", "labels") => "the names in xlabel= and ylabel=, units left out",
        _ => "",
    }
}

/// A word of the command with its byte range.
struct Word {
    text: String,
    start: usize,
    end: usize,
}

/// Split `text` into words like the command parser does (quotes keep spaces together).
fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut current: Option<Word> = None;
    let mut quote: Option<char> = None;
    for (i, c) in text.char_indices() {
        let in_quotes = quote.is_some();
        if !in_quotes && c.is_whitespace() {
            out.extend(current.take());
            continue;
        }
        let w = current.get_or_insert_with(|| Word {
            text: String::new(),
            start: i,
            end: i,
        });
        w.end = i + c.len_utf8();
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            _ => w.text.push(c),
        }
    }
    out.extend(current);
    out
}

fn is_type(w: &str) -> bool {
    PlotKind::from_name(w).is_some()
}

fn type_items(typed: &str, detail: Option<&str>) -> Vec<Suggestion> {
    let names = PlotKind::ALL.map(PlotKind::name);
    filter(&names, typed)
        .into_iter()
        .map(|k| Suggestion {
            label: k.to_owned(),
            insert: format!("{k} "),
            detail: detail.map_or_else(
                || {
                    PlotKind::from_name(k)
                        .map_or("", PlotKind::describe)
                        .to_owned()
                },
                str::to_owned,
            ),
        })
        .collect()
}

pub fn complete(input: &str, cursor: usize, src: &mut dyn Source) -> Completion {
    let mut cursor = cursor.min(input.len());
    while !input.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let all = words(input);
    // The word the cursor is in (or right after); otherwise an empty word at the cursor.
    let current = all
        .iter()
        .position(|w| w.start <= cursor && cursor <= w.end && w.start < cursor);
    let (start, end, typed, before): (usize, usize, String, &[Word]) = match current {
        Some(i) => {
            let w = &all[i];
            (w.start, w.end, input[w.start..cursor].to_owned(), &all[..i])
        }
        None => {
            let i = all
                .iter()
                .position(|w| w.start >= cursor)
                .unwrap_or(all.len());
            (cursor, cursor, String::new(), &all[..i])
        }
    };
    let typed_clean: String = typed.chars().filter(|c| *c != '"' && *c != '\'').collect();

    // Walk the earlier words to know what's expected here and which file is current.
    let mut expect_type = true;
    let mut expect_file = false;
    let mut file: Option<String> = None;
    let mut file_at = 0;
    // The plot type of the layer being typed; after `+` without a type, the panel's first.
    let mut kind: Option<PlotKind> = None;
    let mut panel_kind: Option<PlotKind> = None;
    for (i, w) in before.iter().enumerate() {
        let t = w.text.as_str();
        if t == "+" {
            expect_type = true;
            expect_file = true;
            kind = panel_kind;
        } else if t == ";" {
            expect_type = true;
            expect_file = false;
            panel_kind = None;
        } else if expect_type && is_type(t) {
            kind = PlotKind::from_name(t);
            if panel_kind.is_none() {
                panel_kind = kind;
            }
            expect_type = false;
            expect_file = true;
        } else if expect_file && !t.contains('=') && !t.starts_with("--") {
            file = Some(t.to_owned());
            file_at = i;
            expect_type = false;
            expect_file = false;
        } else {
            expect_type = false;
        }
    }

    let done = |items: Vec<Suggestion>| Completion { start, end, items };

    // First word: plot type (or `again`).
    if before.is_empty() {
        let mut items = type_items(&typed_clean, None);
        for (word, detail) in [
            ("again", "edit a saved plot"),
            ("preset", "a saved preset with another file"),
        ] {
            if !typed_clean.is_empty() && word.starts_with(&typed_clean.to_ascii_lowercase()) {
                items.push(Suggestion {
                    label: word.into(),
                    insert: format!("{word} "),
                    detail: detail.into(),
                });
            }
        }
        return done(items);
    }
    // `preset NAME FILE`: preset names, then files.
    if before[0].text.eq_ignore_ascii_case("preset") {
        if before.len() == 1 {
            let presets = src.presets();
            let names: Vec<&str> = presets.iter().map(|(n, _)| n.as_str()).collect();
            return done(
                filter(&names, &typed_clean)
                    .into_iter()
                    .map(|n| Suggestion {
                        label: n.to_owned(),
                        insert: format!("{n} "),
                        detail: presets
                            .iter()
                            .find(|p| p.0 == n)
                            .map(|p| p.1.clone())
                            .unwrap_or_default(),
                    })
                    .collect(),
            );
        }
        if before.len() == 2 {
            return done(
                src.files(&typed_clean, LIMIT)
                    .into_iter()
                    .map(|f| Suggestion {
                        insert: quote_if_needed(&f),
                        detail: file_detail(&f),
                        label: f,
                    })
                    .collect(),
            );
        }
        return done(Vec::new());
    }
    // `again PLOT`: saved plots.
    if before[0].text.eq_ignore_ascii_case("again") {
        if before.len() > 1 {
            return done(Vec::new());
        }
        return done(
            src.plots(&typed_clean, LIMIT)
                .into_iter()
                .map(|p| Suggestion {
                    insert: quote_if_needed(&p),
                    label: p,
                    detail: "saved plot".into(),
                })
                .collect(),
        );
    }
    // After `;`: a new panel's plot type.
    if expect_type && !expect_file {
        return done(type_items(&typed_clean, Some("plot type for this panel")));
    }

    // A file is expected (after the type or after +).
    if expect_file && !typed_clean.contains('=') && !typed_clean.starts_with('-') {
        let mut items: Vec<Suggestion> = Vec::new();
        if expect_type {
            items.extend(type_items(&typed_clean, Some("plot type for this layer")));
        }
        items.extend(
            src.files(&typed_clean, LIMIT)
                .into_iter()
                .map(|f| Suggestion {
                    insert: format!("{} ", quote_if_needed(&f)),
                    detail: file_detail(&f),
                    label: f,
                }),
        );
        return done(items);
    }

    // key=value
    if let Some((key, value)) = typed_clean.split_once('=') {
        let key_lower = key.to_ascii_lowercase();
        // An option this plot type doesn't use: say so instead of offering values (owner:
        // `kde … err=` listed columns, or nothing, with no reason).
        if let Some(k) = kind
            && !key_lower.starts_with('-')
            && OPTIONS
                .iter()
                .any(|(o, _)| o.trim_end_matches('=') == key_lower)
            && !fits(&key_lower, k)
        {
            return done(vec![Suggestion {
                label: format!("{key_lower}= isn't used by {}", k.name()),
                insert: typed.clone(),
                detail: String::new(),
            }]);
        }
        // peaks= / dips= / onset=: the panel's curves (its y columns), one name each.
        if matches!(key_lower.as_str(), "peaks" | "dips" | "onset") {
            let panel_from = before
                .iter()
                .rposition(|w| w.text == ";")
                .map_or(0, |i| i + 1);
            let mut curves: Vec<String> = Vec::new();
            for w in &before[panel_from..] {
                if let Some(ys) = w.text.strip_prefix("y=") {
                    for y in ys.split(',') {
                        let y = y.trim_matches(|c| c == '"' || c == '\'').to_owned();
                        if !y.is_empty() && !curves.contains(&y) {
                            curves.push(y);
                        }
                    }
                }
            }
            let names: Vec<&str> = curves.iter().map(String::as_str).collect();
            let (prefix, part) = match value.rsplit_once(',') {
                Some((p, last)) => (format!("{p},"), last),
                None => (String::new(), value),
            };
            return done(
                filter(&names, part)
                    .into_iter()
                    .map(|n| Suggestion {
                        label: n.to_owned(),
                        insert: format!("{key}={prefix}{} ", quote_if_needed(n)),
                        detail: "only this curve (a number: how many)".into(),
                    })
                    .collect(),
            );
        }
        // Lists of columns (`x=t1,t2`, `y=a,b`); a single x or z ends with a space.
        let multi = matches!(key_lower.as_str(), "y" | "err" | "yerr");
        let listed = multi || matches!(key_lower.as_str(), "x" | "xerr");
        // Options that only take number columns: error bars and colour-by values.
        let numeric = matches!(key_lower.as_str(), "err" | "yerr" | "xerr" | "z" | "c");
        if listed || key_lower == "z" || key_lower == "by" || key_lower == "c" {
            // `skiplines=`, `sheet=` and `stat=` anywhere in this layer, also after the cursor.
            let mut read = ReadOptions::default();
            let mut has_stat = false;
            for w in all[file_at..].iter().skip(1) {
                let t = w.text.trim_matches(|c| c == '"' || c == '\'');
                if t == "+" || t == ";" {
                    break;
                }
                if let Some((k, v)) = t.split_once('=') {
                    let v = v.trim_matches(|c| c == '"' || c == '\'');
                    match k.to_ascii_lowercase().as_str() {
                        "skiplines" => read.skip = v.parse().unwrap_or(0),
                        "sheet" => read.sheet = Some(v.to_owned()),
                        "stat" => has_stat = true,
                        _ => {}
                    }
                }
            }
            let mut columns = file
                .as_deref()
                .and_then(|f| src.columns(f, &read))
                .unwrap_or_default();
            if numeric {
                columns.retain(|(_, kind)| kind == "numbers");
            }
            // For y=a,b,… only the part after the last comma is being typed.
            let (prefix, part) = match value.rsplit_once(',') {
                Some((p, last)) if listed => (format!("{p},"), last),
                _ => (String::new(), value),
            };
            let names: Vec<&str> = columns.iter().map(|(n, _)| n.as_str()).collect();
            // With `stat=`, `err=` usually names a spread, not a column: offer those
            // first, the file's number columns after (owner, 2026-11-14).
            let mut spread: Vec<Suggestion> = Vec::new();
            if has_stat && matches!(key_lower.as_str(), "err" | "yerr") {
                let words = [
                    ("sd", "standard deviation of each group"),
                    ("sem", "standard error of the mean"),
                    ("ci95", "95% confidence interval"),
                    ("iqr", "quartiles (Q1 to Q3)"),
                ];
                spread = words
                    .iter()
                    .filter(|(w, _)| w.starts_with(&part.to_ascii_lowercase()))
                    .map(|(w, d)| Suggestion {
                        label: (*w).to_owned(),
                        insert: format!("{key}={prefix}{w} "),
                        detail: (*d).to_owned(),
                    })
                    .collect();
            }
            let items = filter(&names, part)
                .into_iter()
                .map(|name| {
                    let kind = columns
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, k)| k.clone())
                        .unwrap_or_default();
                    let tail = if multi || !prefix.is_empty() { "" } else { " " };
                    Suggestion {
                        label: name.to_owned(),
                        insert: format!("{key}={prefix}{}{tail}", quote_if_needed(name)),
                        detail: kind,
                    }
                })
                .collect::<Vec<_>>();
            // A whole column name typed: offer a comma for another one.
            let mut items = items;
            if !spread.is_empty() {
                spread.extend(items);
                items = spread;
            }
            if multi && let Some(name) = names.iter().find(|n| n.eq_ignore_ascii_case(part.trim()))
            {
                let insert = format!("{key}={prefix}{},", quote_if_needed(name));
                items.insert(
                    1.min(items.len()),
                    Suggestion {
                        label: format!("{name}, …"),
                        insert,
                        detail: "add another column (one line each)".into(),
                    },
                );
            }
            return done(items);
        }
        if key_lower == "--style" {
            let styles = src.styles();
            let names: Vec<&str> = styles.iter().map(String::as_str).collect();
            return done(
                filter(&names, value)
                    .into_iter()
                    .map(|v| Suggestion {
                        label: v.to_owned(),
                        insert: format!("{key}={} ", quote_if_needed(v)),
                        detail: "style file".into(),
                    })
                    .collect(),
            );
        }
        let is_colour = key_lower == "color" || key_lower == "colour";
        // Lists (`color=orange,blue`): only the part after the last comma is being typed.
        let listed =
            is_colour || key_lower == "style" || key_lower == "dash" || key_lower == "marker";
        let (prefix, value) = match value.rsplit_once(',') {
            Some((p, last)) if listed => (format!("{p},"), last),
            _ => (String::new(), value),
        };
        let items = filter(&values(&key_lower), value)
            .into_iter()
            .map(|v| Suggestion {
                label: v.to_owned(),
                insert: format!("{key}={prefix}{} ", quote_if_needed(v)),
                // Colours carry their hex value so the launcher can show a swatch.
                detail: if is_colour {
                    crate::spec::Color::parse(v)
                        .map(|c| c.hex())
                        .unwrap_or_default()
                } else {
                    value_note(&key_lower, v).to_owned()
                },
            })
            .collect();
        return done(items);
    }

    // An option name (only those that do something for this plot type).
    let names: Vec<&str> = OPTIONS
        .iter()
        .map(|(o, _)| *o)
        .filter(|o| kind.is_none_or(|k| fits(o, k)))
        .collect();
    // The words of the panel the cursor is in (between the `;` either side).
    let panel_from = all
        .iter()
        .rposition(|w| w.text == ";" && w.end <= cursor)
        .map_or(0, |i| i + 1);
    let panel_words: Vec<&str> = all[panel_from..]
        .iter()
        .take_while(|w| w.text != ";")
        .map(|w| w.text.as_str())
        .collect();
    // Options that clash with one already written (the engine refuses both together):
    // `markers` and `marker=`, `legend=` and `--legend=` (owner, 2026-11-15).
    let written = |w: &str| {
        all.iter().any(|x| {
            let t = x.text.to_ascii_lowercase();
            t == w || (w.ends_with('=') && t.starts_with(w))
        })
    };
    let clash = |o: &str| match o {
        "marker=" => written("markers"),
        "markers" => written("marker="),
        "legend=" => written("--legend="),
        "--legend=" => written("legend="),
        _ => false,
    };
    let names: Vec<&str> = names.into_iter().filter(|o| !clash(o)).collect();
    let items = filter(&names, &typed_clean)
        .into_iter()
        .map(|o| {
            let detail = OPTIONS
                .iter()
                .find(|(n, _)| *n == o)
                .map_or("", |(_, d)| *d);
            let detail = match needs(o, &panel_words) {
                Some(need) => format!("needs {need} · {detail}"),
                None => detail.to_owned(),
            };
            let insert = if o.ends_with('=') {
                o.to_owned()
            } else {
                format!("{o} ")
            };
            Suggestion {
                label: o.to_owned(),
                insert,
                detail,
            }
        })
        .collect();
    done(items)
}

/// Quote names containing spaces, commas or quotes so the parser reads them as one word.
/// "file", or for a pattern "every matching file".
fn file_detail(f: &str) -> String {
    if f.contains(['*', '?']) {
        "every matching file in that folder".into()
    } else {
        "file".into()
    }
}

pub fn quote_if_needed(s: &str) -> String {
    if s.is_empty()
        || s.chars()
            .any(|c| c.is_whitespace() || ",\"';=()`".contains(c))
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl Source for Fake {
        fn files(&mut self, query: &str, _limit: usize) -> Vec<String> {
            filter(&["simple.csv", "lab/run 3.csv"], query)
                .into_iter()
                .map(String::from)
                .collect()
        }
        fn columns(&mut self, file: &str, read: &ReadOptions) -> Option<Vec<(String, String)>> {
            if read.skip == 2 {
                return Some(vec![("after skip".into(), "numbers".into())]);
            }
            (file == "simple.csv").then(|| {
                vec![
                    ("time".into(), "numbers".into()),
                    ("temp".into(), "numbers".into()),
                    ("Voltage (V)".into(), "numbers".into()),
                    ("site".into(), "text".into()),
                ]
            })
        }
    }

    #[test]
    fn colour_lists_complete_the_last_entry() {
        let c = complete("line simple.csv y=temp,time color=orange,bl", 43, &mut Fake);
        let blue = c
            .items
            .iter()
            .find(|s| s.label == "blue")
            .expect("blue offered");
        assert_eq!(blue.insert, "color=orange,blue ");
    }

    #[test]
    fn suggests_only_options_of_the_plot_type() {
        let hist = labels("hist simple.csv x=temp ");
        assert!(hist.contains(&"bins=".to_owned()) && hist.contains(&"density".to_owned()));
        assert!(!hist.contains(&"marker=".to_owned()) && !hist.contains(&"cmap=".to_owned()));
        let line = labels("line simple.csv x=time y=temp ");
        assert!(line.contains(&"marker=".to_owned()) && line.contains(&"fit=".to_owned()));
        assert!(!line.contains(&"bins=".to_owned()) && !line.contains(&"levels=".to_owned()));
        // A layer added with + and no type is the panel's first type; ; starts afresh.
        let overlay = labels("hist simple.csv x=temp + simple.csv x=time ");
        assert!(overlay.contains(&"bins=".to_owned()));
        let next = labels("hist simple.csv x=temp ; contour simple.csv x=time y=temp ");
        assert!(next.contains(&"levels=".to_owned()) && !next.contains(&"bins=".to_owned()));
        // Options that apply to every type stay, e.g. titles and figure options.
        assert!(hist.contains(&"title=".to_owned()) && hist.contains(&"--size=".to_owned()));
    }

    #[test]
    fn round_12_values_and_names() {
        // cross always takes a value: curves (explained), numbers are typed.
        let c = complete("line simple.csv y=temp cross=", 29, &mut Fake);
        assert_eq!(c.items[0].label, "curves");
        assert!(c.items[0].detail.contains("cross=50"));
        assert!(labels("line simple.csv y=temp cro").contains(&"cross=".to_owned()));
        assert!(labels("line simple.csv y=temp summ").contains(&"summary".to_owned()));
        assert!(!labels("line simple.csv y=temp sta").contains(&"stats".to_owned()));
        // peaks= offers the panel's curves.
        let p = labels("line simple.csv x=time y=temp,\"Voltage (V)\" peaks=");
        assert_eq!(p, vec!["temp".to_owned(), "Voltage (V)".to_owned()]);
        assert_eq!(
            apply("line simple.csv y=temp peaks=te", "temp"),
            "line simple.csv y=temp peaks=temp "
        );
        // An option the plot type doesn't use says so (owner: kde … err= showed nothing).
        assert_eq!(
            labels("kde simple.csv x=temp err="),
            vec!["err= isn't used by kde".to_owned()]
        );
        assert_eq!(
            apply("kde simple.csv x=temp err=", "err= isn't used by kde"),
            "kde simple.csv x=temp err="
        );
        // fitnames= with its two values explained.
        let f = complete(
            "scatter simple.csv x=time y=temp fit=linear fitnames=",
            54,
            &mut Fake,
        );
        assert_eq!(
            f.items.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            ["columns", "labels"]
        );
    }

    #[test]
    fn clashing_options_are_not_offered() {
        assert!(!labels("line simple.csv y=temp markers mark").contains(&"marker=".to_owned()));
        assert!(!labels("line simple.csv y=temp marker=o mark").contains(&"markers".to_owned()));
        assert!(!labels("line simple.csv y=temp --legend=top leg").contains(&"legend=".to_owned()));
        assert!(labels("line simple.csv y=temp mark").contains(&"markers".to_owned()));
    }

    fn labels(input: &str) -> Vec<String> {
        complete(input, input.len(), &mut Fake)
            .items
            .into_iter()
            .map(|s| s.label)
            .collect()
    }

    fn apply(input: &str, pick: &str) -> String {
        let c = complete(input, input.len(), &mut Fake);
        let s = c.items.iter().find(|s| s.label == pick).expect(pick);
        format!("{}{}{}", &input[..c.start], s.insert, &input[c.end..])
    }

    #[test]
    fn first_word_is_a_type() {
        assert_eq!(&labels("")[..3], ["line", "scatter", "bar"]);
        assert!(labels("").contains(&"hist".to_owned()));
        assert_eq!(apply("sc", "scatter"), "scatter ");
    }

    #[test]
    fn then_a_file() {
        assert_eq!(labels("line "), ["simple.csv", "lab/run 3.csv"]);
        assert_eq!(
            apply("line run", "lab/run 3.csv"),
            "line \"lab/run 3.csv\" "
        );
    }

    #[test]
    fn columns_inside_x_and_y() {
        assert_eq!(
            apply("line simple.csv x=ti", "time"),
            "line simple.csv x=time "
        );
        assert_eq!(
            apply("line simple.csv x=time y=temp,Vo", "Voltage (V)"),
            "line simple.csv x=time y=temp,\"Voltage (V)\""
        );
        // Paired x columns: after the comma, columns again.
        assert_eq!(
            apply("line simple.csv x=time,", "temp"),
            "line simple.csv x=time,temp"
        );
        assert!(labels("line simple.csv x=time y=temp,").contains(&"time".to_owned()));
        // The columns come from the file as read with this layer's skiplines=, even typed later.
        let input = "line simple.csv y= skiplines=2";
        let c = complete(input, 18, &mut Fake);
        assert_eq!(c.items[0].label, "after skip");
    }

    #[test]
    fn round_8_options_are_suggested() {
        // The owner saw nothing for these in the launcher (2026-11-14): pin each one down.
        assert!(labels("scatter simple.csv x=time y=temp ybr").contains(&"ybreak=".to_owned()));
        assert!(labels("line simple.csv x=time y=temp crossl").contains(&"crosslabel=".to_owned()));
        assert!(labels("line simple.csv x=time y=temp --leg").contains(&"--legend=".to_owned()));
        assert!(labels("line simple.csv x=time y=temp --wid").contains(&"--widths=".to_owned()));
        assert!(labels("scatter simple.csv x=time y=temp c").contains(&"c=".to_owned()));
        // After the `=`: values for norm=, number columns (only) for c=.
        assert_eq!(
            labels("line simple.csv x=time y=temp norm="),
            ["max", "first", "area", "range"]
        );
        let c = labels("scatter simple.csv x=time y=temp c=");
        assert!(c.contains(&"temp".to_owned()) && !c.contains(&"site".to_owned()));
    }

    #[test]
    fn err_suggests_spreads_with_stat_and_number_columns() {
        // With stat=, err= usually names a spread: those first, number columns after.
        let with = labels("bar simple.csv x=site y=temp stat=mean err=");
        assert_eq!(&with[..4], ["sd", "sem", "ci95", "iqr"]);
        assert!(with.contains(&"temp".to_owned()) && !with.contains(&"site".to_owned()));
        assert_eq!(
            apply("bar simple.csv x=site y=temp stat=mean err=se", "sem"),
            "bar simple.csv x=site y=temp stat=mean err=sem "
        );
        // Without stat= only columns make sense, and text columns never do.
        let without = labels("line simple.csv x=time y=temp err=");
        assert!(!without.contains(&"sd".to_owned()) && !without.contains(&"site".to_owned()));
    }

    #[test]
    fn options_that_need_another_say_so() {
        let detail = |text: &str, option: &str| {
            complete(text, text.len(), &mut Fake)
                .items
                .into_iter()
                .find(|s| s.label == option)
                .map(|s| s.detail)
                .unwrap()
        };
        assert!(detail("scatter simple.csv fitr", "fitrange=").starts_with("needs fit= · "));
        assert!(!detail("scatter simple.csv fit=linear fitr", "fitrange=").contains("needs"));
        // Another panel's fit doesn't count.
        assert!(
            detail(
                "scatter simple.csv fit=linear ; scatter simple.csv fitb",
                "fitband"
            )
            .starts_with("needs fit=")
        );
        assert!(detail("line simple.csv y2l", "y2label=").starts_with("needs axis=right"));
        assert!(!detail("line simple.csv axis=right y2l", "y2label=").contains("needs"));
        assert!(detail("line simple.csv showb", "showbaseline").starts_with("needs baseline="));
    }

    #[test]
    fn options_and_values() {
        assert!(labels("line simple.csv ti").contains(&"title=".to_owned()));
        assert_eq!(
            apply("line simple.csv legend=b", "br"),
            "line simple.csv legend=br "
        );
        assert_eq!(labels("line simple.csv --format="), ["png", "pdf", "svg"]);
        let c = complete("line simple.csv color=bl", 24, &mut Fake);
        assert_eq!(c.items[0].detail, "#1f77b4");
        assert!(labels("line simple.csv lo").contains(&"logy".to_owned()));
        assert_eq!(
            apply("line simple.csv --font=lat", "Latin Modern"),
            "line simple.csv --font=\"Latin Modern\" "
        );
        assert!(labels("line simple.csv --theme=").contains(&"dark".to_owned()));
    }

    #[test]
    fn new_options_panels_and_again() {
        assert!(labels("line simple.csv fi").contains(&"fit=".to_owned()));
        assert!(labels("line simple.csv fit=").contains(&"poly2".to_owned()));
        assert!(labels("line simple.csv x=time ; ").contains(&"hist".to_owned()));
        assert_eq!(
            apply("line simple.csv x=time err=te", "temp"),
            "line simple.csv x=time err=temp"
        );
        assert_eq!(
            apply("line simple.csv x=time y=temp", "temp, …"),
            "line simple.csv x=time y=temp,"
        );
        assert!(labels("ag").contains(&"again".to_owned()));
    }

    #[test]
    fn after_plus_types_and_files() {
        let l = labels("line simple.csv + ");
        assert_eq!(&l[..3], ["line", "scatter", "bar"]);
        assert!(l.contains(&"simple.csv".to_owned()));
        // `+ y=` with no file: the columns of the file before.
        assert!(labels("line simple.csv x=time y=temp + y=").contains(&"temp".to_owned()));
    }

    #[test]
    fn cursor_in_the_middle_replaces_that_word() {
        let input = "line simple.csv x=t y=temp";
        let c = complete(input, 18 + 1, &mut Fake); // after "x=t"
        assert_eq!(&input[c.start..c.end], "x=t");
        assert_eq!(c.items[0].label, "time");
    }
}
