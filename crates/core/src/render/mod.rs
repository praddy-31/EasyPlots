//! Drawing a `Figure` as SVG text. Everything is measured in points, so the same SVG
//! becomes a crisp PDF or a PNG at any DPI (see `export`).
//!
//! The figure is split into a grid of cells, one per panel. Each panel is first measured
//! (tick labels, axis labels, legend) to find the room it needs around its plot area; plot
//! areas in the same row and column are then lined up, and each panel is drawn back to
//! front: grid, data (clipped to the plot area), frame, ticks, labels, title and legend.

mod groups;
pub mod mathtext;
pub mod text;
pub mod ticks;

use std::fmt::Write;

use crate::colormap::text_on;
use crate::figure::{AxisKind, Figure, Grid, Mark, Panel, Series, TopAxis};
use crate::spec::{
    Color, GridLines, Inset, LegendPos, LineStyle, Marker, NumberFormat, PlotKind, RefKind, Shape,
    SharedLegend, StepWhere,
};
use crate::style::Style;
use text::Font;
use ticks::{Offset, Ticks};

mod axes;
mod legend;
mod marks;
mod multivariate;
mod univariate;
use axes::*;
use legend::*;
use marks::*;
use multivariate::*;
use univariate::*;

/// Render the figure. `desc` (normally the command) is stored inside the SVG. The font name
/// is quoted: unquoted, "Source Sans 3" is not a valid CSS family name.
/// The left y axis's range as it would be drawn (for checks before drawing).
pub(crate) fn left_y_range(panel: &Panel) -> (f64, f64) {
    y_range(panel, false)
}

pub fn render(fig: &Figure, style: &Style, desc: &str) -> String {
    render_noting(fig, style, desc).0
}

/// What drawing noticed that the figure alone doesn't say (for the plot advisor).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawn {
    /// Per panel: how many pieces of data (points, bits of line, bars) its legend box
    /// sits on. 0 when the legend is outside, off or clear of the data.
    pub legend_covers: Vec<usize>,
}

/// `render`, also saying what was noticed on the way (a legend left on top of data).
pub fn render_noting(fig: &Figure, style: &Style, desc: &str) -> (String, Drawn) {
    let mut drawn = Drawn::default();
    let mut out = String::with_capacity(64 * 1024);
    let (w, h) = (fig.width, fig.height);
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}pt" height="{}pt" viewBox="0 0 {} {}" font-family="'{}'" fill="{}">"#,
        n(w),
        n(h),
        n(w),
        n(h),
        style.font.family(),
        style.foreground.hex()
    );
    let _ = write!(out, "\n<desc>{}</desc>", escape(desc));
    let _ = write!(
        out,
        "\n<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>",
        n(w),
        n(h),
        style.background.hex()
    );

    // A title over all panels takes a band at the top.
    let mut top = 0.0;
    if let Some(t) = &fig.suptitle {
        let size = style.title_font_size * 1.1;
        top = style.pad + size * 1.5;
        text_at(
            &mut out,
            style.font,
            w / 2.0,
            style.pad + text::cap_height(style.font, size),
            size,
            "middle",
            t,
        );
    }
    let (rows, cols) = fig.grid;
    let count = fig.panels.len();
    // One legend for the whole figure takes a strip at the bottom, the top or the right;
    // the panels share the rest (`area`).
    let mut area = Rect::new(0.0, top, w, h - top);
    let shared_legend = fig
        .shared_legend
        .filter(|side| *side != SharedLegend::Panels)
        .and_then(|side| figure_legend(fig, style, side, w));
    if let Some(l) = &shared_legend {
        let band = match l.side {
            SharedLegend::Bottom => {
                area.h -= l.room;
                Rect::new(0.0, area.bottom(), w, l.room)
            }
            SharedLegend::Top => {
                let mut band = Rect::new(0.0, area.y, w, l.room);
                // The legend of one panel only: centred over that panel's column.
                let from: Vec<usize> = (0..count)
                    .filter(|i| fig.shared_from.get(*i).copied().unwrap_or(false))
                    .collect();
                if let ([only], true) = (from.as_slice(), fig.shared_from.len() == count) {
                    let cw = |c: usize| fig.col_weights.get(c).copied().unwrap_or(1.0);
                    let all: f64 = (0..cols).map(cw).sum();
                    let col = only % cols;
                    band.x = (0..col).map(cw).sum::<f64>() / all * w;
                    band.w = cw(col) / all * w;
                }
                area.y += l.room;
                area.h -= l.room;
                band
            }
            SharedLegend::Right => {
                area.w -= l.room;
                Rect::new(area.right(), area.y, l.room, area.h)
            }
            // Never reaches the drawing: the figure builder turns it into "none".
            SharedLegend::Panels => Rect::new(0.0, 0.0, 0.0, 0.0),
        };
        draw_figure_legend(&mut out, l, band, w, style);
    }
    // Row heights and column widths: the same, or in proportion to the weights
    // (residual strips, `--heights=`, `--widths=`).
    let weight = |r: usize| fig.row_weights.get(r).copied().unwrap_or(1.0);
    let total: f64 = (0..rows).map(weight).sum();
    let cweight = |c: usize| fig.col_weights.get(c).copied().unwrap_or(1.0);
    let ctotal: f64 = (0..cols).map(cweight).sum();
    let cell = |i: usize| {
        let (r, c) = (i / cols, i % cols);
        let x0: f64 = (0..c).map(cweight).sum::<f64>() / ctotal * area.w;
        let cw = cweight(c) / ctotal * area.w;
        let y0: f64 = (0..r).map(weight).sum::<f64>() / total * area.h;
        let ch = weight(r) / total * area.h;
        Rect::new(area.x + x0, area.y + y0, cw, ch)
    };

    // Shared axes: one range for all panels, labels only on the outer edge.
    let shared_x = (fig.sharex && count > 1).then(|| {
        let all: Vec<(f64, f64)> = fig.panels.iter().map(x_range).collect();
        union(&all)
    });
    let shared_y = (fig.sharey && count > 1).then(|| {
        let all: Vec<(f64, f64)> = fig.panels.iter().map(|p| y_range(p, false)).collect();
        union(&all)
    });
    // A residuals strip shares its panel's x axis; the panel's x labels go below the strip.
    let strip_below = |i: usize| fig.panels.get(i + 1).is_some_and(|q| q.residual_of_prev);
    // Horizontal panels are drawn from their sideways view (see `groups`).
    let sides: Vec<Option<groups::Sideways>> = fig
        .panels
        .iter()
        .map(|p| p.horizontal.then(|| groups::sideways(p)))
        .collect();
    let shown = |i: usize| sides[i].as_ref().map_or(&fig.panels[i], |s| &s.axes);
    let layouts: Vec<Layout> = (0..count)
        .map(|i| {
            if let Some(side) = &sides[i] {
                let opts = Shared {
                    x: Some(side.x),
                    y: Some(side.y),
                    hide_x: false,
                    hide_y: false,
                    letter_room: fig.letters,
                };
                return measure(&side.axes, cell(i), style, &opts);
            }
            let p = &fig.panels[i];
            let pair_x = if p.residual_of_prev {
                Some(x_range(&fig.panels[i - 1]))
            } else if strip_below(i) {
                Some(x_range(p))
            } else {
                None
            };
            let opts = Shared {
                x: pair_x.or(shared_x.filter(|_| fig.panels.iter().all(|q| q.logx == p.logx))),
                y: shared_y.filter(|_| fig.panels.iter().all(|q| q.logy == p.logy)),
                hide_x: (fig.sharex && i + cols < count) || strip_below(i),
                hide_y: fig.sharey && i % cols > 0,
                letter_room: fig.letters,
            };
            measure(p, cell(i), style, &opts)
        })
        .collect();

    // Line up the plot areas: the same left and right edges in each column, the same top
    // and bottom in each row.
    let mut plots: Vec<Rect> = layouts.iter().map(|l| l.plot).collect();
    if count > 1 {
        for c in 0..cols {
            let idx: Vec<usize> = (c..count).step_by(cols).collect();
            let left = idx
                .iter()
                .map(|&i| plots[i].x - cell(i).x)
                .fold(0.0, f64::max);
            let right = idx
                .iter()
                .map(|&i| cell(i).right() - plots[i].right())
                .fold(0.0, f64::max);
            for &i in &idx {
                let cl = cell(i);
                plots[i].x = cl.x + left;
                plots[i].w = (cl.w - left - right).max(10.0);
            }
        }
        for r in 0..rows {
            let idx: Vec<usize> = (r * cols..((r + 1) * cols).min(count)).collect();
            let top = idx
                .iter()
                .map(|&i| plots[i].y - cell(i).y)
                .fold(0.0, f64::max);
            let bottom = idx
                .iter()
                .map(|&i| cell(i).bottom() - plots[i].bottom())
                .fold(0.0, f64::max);
            for &i in &idx {
                let cl = cell(i);
                plots[i].y = cl.y + top;
                plots[i].h = (cl.h - top - bottom).max(10.0);
            }
        }
    }
    let mut lettered = 0;
    for (i, panel) in fig.panels.iter().enumerate() {
        // Residual strips belong to the panel above: no letter of their own.
        let letter = (fig.letters && !panel.residual_of_prev).then(|| {
            lettered += 1;
            format!("({})", (b'a' + ((lettered - 1) % 26) as u8) as char)
        });
        let covers = draw_panel(
            &mut out,
            shown(i),
            sides[i].as_ref(),
            cell(i),
            &layouts[i],
            plots[i],
            style,
            i,
            letter.as_deref(),
        );
        drawn.legend_covers.push(covers);
    }
    out.push_str("\n</svg>\n");
    (out, drawn)
}

fn union(ranges: &[(f64, f64)]) -> (f64, f64) {
    ranges
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), r| {
            (lo.min(r.0), hi.max(r.1))
        })
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x - 0.5
            && x <= self.right() + 0.5
            && y >= self.y - 0.5
            && y <= self.bottom() + 0.5
    }
    fn overlaps(&self, other: &Rect) -> bool {
        self.x <= other.right()
            && other.x <= self.right()
            && self.y <= other.bottom()
            && other.y <= self.bottom()
    }
}

/// Maps data values to positions on the page. On a log axis `lo` and `hi` are powers of
/// ten (log10 of the axis ends) and values at or below zero map to NaN (not drawn).
#[derive(Clone, Copy)]
struct Scale {
    lo: f64,
    hi: f64,
    start: f64,
    end: f64,
    log: bool,
    /// A broken axis (`ybreak=`): values from `.0` to `.1` are squeezed into `.2` points
    /// on the page (signed like `end - start`); the rest shares what is left evenly.
    brk: Option<(f64, f64, f64)>,
}

/// The height of a broken axis's gap on the page, in points.
const BREAK_GAP: f64 = 7.0;

impl Scale {
    fn linear(lo: f64, hi: f64, start: f64, end: f64) -> Scale {
        Scale {
            lo,
            hi,
            start,
            end,
            log: false,
            brk: None,
        }
    }
    /// The same scale with the range `a`–`b` squeezed into a small gap.
    fn broken(self, a: f64, b: f64) -> Scale {
        Scale {
            brk: Some((a, b, BREAK_GAP * (self.end - self.start).signum())),
            ..self
        }
    }
    fn axis(&self, v: f64) -> f64 {
        if !self.log {
            v
        } else if v > 0.0 {
            v.log10()
        } else {
            f64::NAN
        }
    }
    /// Like `map`, for a value already in axis units (log10 on a log axis).
    fn map_axis(&self, a: f64) -> f64 {
        let Some((b0, b1, gap)) = self.brk else {
            return self.start + (a - self.lo) / (self.hi - self.lo) * (self.end - self.start);
        };
        // Points per unit outside the gap, the same on both sides of it.
        let k = (self.end - self.start - gap) / ((self.hi - self.lo) - (b1 - b0));
        if a <= b0 {
            self.start + (a - self.lo) * k
        } else if a >= b1 {
            self.start + (b0 - self.lo) * k + gap + (a - b1) * k
        } else {
            self.start + (b0 - self.lo) * k + gap * (a - b0) / (b1 - b0)
        }
    }
    fn map(&self, v: f64) -> f64 {
        self.map_axis(self.axis(v))
    }
    /// Like `map`, but values a log axis can't show go to its low end (for bar bottoms and
    /// error bars that reach below zero).
    fn map_or_floor(&self, v: f64) -> f64 {
        let p = self.map(v);
        if p.is_nan() && self.log && v.is_finite() {
            self.start
        } else {
            p
        }
    }
    fn contains(&self, v: f64) -> bool {
        let a = self.axis(v);
        let eps = (self.hi - self.lo).abs() * 1e-9;
        a >= self.lo - eps && a <= self.hi + eps
    }
    /// Where bars start: zero, or the bottom of a log axis (which has no zero).
    fn baseline(&self) -> f64 {
        if self.log {
            self.start
        } else {
            self.map(0.0f64.clamp(self.lo.min(self.hi), self.hi.max(self.lo)))
        }
    }
}

const TICK_GAP: f64 = 3.5;
/// Space between the plot and a legend placed outside it.
const LEGEND_OUT_GAP: f64 = 10.0;

/// What a panel takes from the others when axes are shared.
struct Shared {
    x: Option<(f64, f64)>,
    y: Option<(f64, f64)>,
    /// Leave out the x tick labels and label (a panel below shows them).
    hide_x: bool,
    /// Leave out the y tick labels and label (the panel on the left shows them).
    hide_y: bool,
    /// Keep room above the plot for the panel letter.
    letter_room: bool,
}

/// A measured panel: its axis ranges, ticks and plot area.
struct Layout {
    x: (f64, f64),
    y: (f64, f64),
    y2: Option<(f64, f64)>,
    plot: Rect,
    xt: Ticks,
    legend_out: Option<(f64, f64)>,
    label_gap: f64,
    hide_x: bool,
    hide_y: bool,
}

fn measure(panel: &Panel, cell: Rect, style: &Style, shared: &Shared) -> Layout {
    let fs = style.font_size;
    let tfs = style.tick_font_size;
    let font = style.font;
    let (xlo, xhi) = shared.x.unwrap_or_else(|| x_range(panel));
    let (ylo, yhi) = shared.y.unwrap_or_else(|| y_range(panel, false));
    let y2 = panel.has_right_axis().then(|| y_range(panel, true));
    // Outward ticks push their labels further out.
    let out_tick = if style.ticks_inward {
        0.0
    } else {
        style.major_tick
    };
    let label_gap = TICK_GAP + out_tick;
    let legend_out =
        (panel.legend == LegendPos::Out && has_legend(panel)).then(|| legend_size(panel, style));
    if panel.polar {
        return measure_polar(panel, cell, style, shared, legend_out);
    }
    let label_widths = |t: &Ticks| {
        t.labels
            .iter()
            .map(|l| mathtext::width(font, l, tfs))
            .fold(0.0, f64::max)
    };

    // First guess at the plot area, to choose tick counts; then measure and fit.
    let guess_h = cell.h * 0.75;
    let target = (guess_h / 40.0) as usize;
    let yt = y_ticks(panel, ylo, yhi, target);
    // Notes under group names (`stats`): as wide as they are, below the x names.
    let note_w = panel
        .cat_notes
        .iter()
        .flat_map(|n| n.lines())
        .map(|l| mathtext::width(font, l, note_size(style)))
        .fold(0.0, f64::max);
    let ytw = if shared.hide_y {
        0.0
    } else if panel.ycategories.is_empty() {
        label_widths(&yt)
    } else {
        label_widths(&yt).max(note_w)
    };
    let ylabel_room = if panel.ylabel.is_some() && !shared.hide_y {
        fs * 1.25 + 2.0
    } else {
        0.0
    };
    let left = cell.x + style.pad + ylabel_room + ytw + label_gap;
    let legend_room = legend_out.map_or(0.0, |(w, _)| w + LEGEND_OUT_GAP);
    let y2label_room = if panel.y2label.is_some() {
        fs * 1.25 + 2.0
    } else {
        0.0
    };
    // The final plot height (and so tick count) isn't known yet: room for the widest
    // labels any nearby count gives (2.5 is wider than 2).
    let mapped_room = panel.y2conv.as_ref().map_or(0.0, |axis| {
        (target.saturating_sub(3).max(2)..=target + 3)
            .filter_map(|t| mapped_ticks(axis, panel.logy, (ylo, yhi), t))
            .map(|(t, _)| label_widths(&t))
            .fold(0.0, f64::max)
            + label_gap
            + y2label_room
    });
    let right_axis_room = mapped_room.max(y2.map_or(0.0, |(lo, hi)| {
        let t = make_ticks(
            AxisKind::Number,
            lo,
            hi,
            target,
            &[],
            panel.logy2,
            step_of(panel.y2step, panel.y2lim),
        );
        let label = if panel.y2label.is_some() {
            fs * 1.25 + 2.0
        } else {
            0.0
        };
        label_widths(&t) + label_gap + label
    })) + if panel.has_colorbar() {
        colorbar_room(panel, style, target)
    } else {
        0.0
    };
    let xt = fitting_x_ticks(
        panel,
        xlo,
        xhi,
        cell.right() - left - style.pad - legend_room - right_axis_room,
        style,
    );
    let xend = if panel.logx { 10f64.powf(xhi) } else { xhi };
    let xspan = if panel.logx {
        xend - 10f64.powf(xlo)
    } else {
        xhi - xlo
    };
    let last_half = xt
        .major
        .last()
        .zip(xt.labels.last())
        .filter(|(v, _)| (*v - xend).abs() <= xspan * 0.02 && !shared.hide_x)
        .map_or(0.0, |(_, l)| mathtext::width(font, l, tfs) / 2.0);
    let right = cell.right() - style.pad - (last_half.max(2.0).max(right_axis_room) + legend_room);
    let x_offset_room = xt.offset.is_some() && panel.xlabel.is_none();
    let bottom_row = if (panel.xlabel.is_some() || x_offset_room) && !shared.hide_x {
        fs * 1.35
    } else {
        0.0
    };
    let x_notes = if panel.xaxis == AxisKind::Category && panel.ycategories.is_empty() {
        note_lines(panel) as f64 * note_size(style) * 1.2
    } else {
        0.0
    };
    let tick_row = if shared.hide_x {
        out_tick + 2.0
    } else {
        label_gap + text::cap_height(font, tfs) + text::descent(font, tfs) * 0.6 + x_notes
    };
    let bottom = cell.bottom() - style.pad - bottom_row - tick_row;
    let title_room = if panel.title.is_some() || shared.letter_room {
        style.title_font_size * 1.5
    } else {
        0.0
    };
    let mut top = cell.y + style.pad + title_room + text::cap_height(font, tfs) * 0.5;
    if let Some(axis) = &panel.x2 {
        top += label_gap + text::cap_height(font, tfs) + text::descent(font, tfs) * 0.6;
        if axis.label.is_some() {
            top += fs * 1.35;
        }
    }
    if yt.offset.is_some() {
        top += tfs * 1.2;
    }
    // The y ticks are chosen again for the final height; if their labels come out wider
    // than the first guess's, move the plot's left edge to make room.
    let mut left = left;
    if !shared.hide_y {
        let last = y_ticks(panel, ylo, yhi, ((bottom - top).max(10.0) / 40.0) as usize);
        let w = if panel.ycategories.is_empty() {
            label_widths(&last)
        } else {
            label_widths(&last).max(note_w)
        };
        left += (w - ytw).max(0.0);
    }
    Layout {
        x: (xlo, xhi),
        y: (ylo, yhi),
        y2,
        plot: Rect::new(
            left,
            top,
            (right - left).max(10.0),
            (bottom - top).max(10.0),
        ),
        xt,
        legend_out,
        label_gap,
        hide_x: shared.hide_x,
        hide_y: shared.hide_y,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_panel(
    out: &mut String,
    panel: &Panel,
    side: Option<&groups::Sideways>,
    cell: Rect,
    layout: &Layout,
    plot: Rect,
    style: &Style,
    id: usize,
    letter: Option<&str>,
) -> usize {
    let fs = style.font_size;
    let tfs = style.tick_font_size;
    let font = style.font;
    let label_gap = layout.label_gap;
    let ((xlo, xhi), (ylo, yhi)) = (layout.x, layout.y);

    if panel.polar {
        draw_polar(out, panel, cell, plot, style, id, letter, layout.legend_out);
        return 0;
    }
    // Final ticks for the real size.
    let yt = y_ticks(panel, ylo, yhi, (plot.h / 40.0) as usize);
    let y2t = layout.y2.map(|(lo, hi)| {
        make_ticks(
            AxisKind::Number,
            lo,
            hi,
            (plot.h / 40.0) as usize,
            &[],
            panel.logy2,
            step_of(panel.y2step, panel.y2lim),
        )
    });
    let xt = {
        let again = fitting_x_ticks(panel, xlo, xhi, plot.w, style);
        if again.labels.len() <= layout.xt.labels.len() {
            again
        } else {
            layout.xt.clone()
        }
    };
    let sx = Scale {
        log: panel.logx,
        ..Scale::linear(xlo, xhi, plot.x, plot.right())
    };
    let sy = Scale {
        log: panel.logy,
        ..Scale::linear(ylo, yhi, plot.bottom(), plot.y)
    };
    let ybreak = panel
        .ybreak
        .filter(|(a, b)| ylo < *a && *b < yhi && !panel.logy);
    let sy = match ybreak {
        Some((a, b)) => sy.broken(a, b),
        None => sy,
    };
    let sy2 = layout.y2.map(|(lo, hi)| Scale {
        log: panel.logy2,
        ..Scale::linear(lo, hi, plot.bottom(), plot.y)
    });
    let fg = style.foreground.hex();

    let _ = write!(
        out,
        "\n<clipPath id=\"plot{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
        n(plot.x),
        n(plot.y),
        n(plot.w),
        n(plot.h)
    );

    if panel.grid {
        let mut d = String::new();
        let (vertical, horizontal) = match panel.grid_axis {
            Some(GridLines::X) => (true, false),
            Some(GridLines::Y) => (false, true),
            None => (true, true),
        };
        for v in xt.major.iter().filter(|v| vertical && sx.contains(**v)) {
            let x = sx.map(*v);
            let _ = write!(d, "M{} {}V{}", n(x), n(plot.y), n(plot.bottom()));
        }
        for v in yt.major.iter().filter(|v| horizontal && sy.contains(**v)) {
            let y = sy.map(*v);
            let _ = write!(d, "M{} {}H{}", n(plot.x), n(y), n(plot.right()));
        }
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
            style.grid_color.hex(),
            n(style.grid_width)
        );
    }

    let _ = write!(out, "\n<g clip-path=\"url(#plot{id})\">");
    match side {
        None => draw_data(out, panel, &sx, &sy, sy2.as_ref(), plot, style),
        Some(side) => {
            // Upright data through a mirror across the diagonal (x and y swap).
            let (cat, val) = groups::upright_scales(side, &sx, &sy);
            let mirrored = Rect::new(plot.y, plot.x, plot.h, plot.w);
            out.push_str("\n<g transform=\"matrix(0 1 1 0 0 0)\">");
            draw_data(out, &side.data, &cat, &val, None, mirrored, style);
            out.push_str("\n</g>");
            groups::draw_strips(out, &side.data, &cat, &val, style, true);
        }
    }
    out.push_str("\n</g>");
    // A broken axis: the squeezed range is blanked out across the plot.
    if let Some((a, b)) = ybreak {
        let (y0, y1) = (sy.map(b), sy.map(a));
        let _ = write!(
            out,
            "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            n(plot.x - 1.0),
            n(y0),
            n(plot.w + 2.0),
            n(y1 - y0),
            style.background.hex()
        );
    }
    if panel.values {
        draw_values(out, panel, &sx, &sy, sy2.as_ref(), plot, style);
    }
    match side {
        None => groups::draw_level_labels(out, panel, &sx, &sy, style, false),
        Some(side) => {
            let (cat, val) = groups::upright_scales(side, &sx, &sy);
            if side.data.values {
                groups::draw_values_sideways(out, &side.data, &cat, &val, plot, style);
            }
            groups::draw_level_labels(out, &side.data, &cat, &val, style, true);
        }
    }
    if panel.has_colorbar() {
        draw_colorbar(out, panel, plot, cell, style);
    }
    draw_ref_labels(out, panel, &sx, &sy, sy2.as_ref(), plot, style);
    draw_shape_labels(out, panel, &sx, &sy, plot, style);
    draw_notes(out, panel, &sx, &sy, sy2.as_ref(), plot, style);

    // Frame: a full box (paper) or only the left and bottom axes (screen themes), plus the
    // right axis line when there is a second y axis.
    if style.full_frame {
        let _ = write!(
            out,
            "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
            n(plot.x),
            n(plot.y),
            n(plot.w),
            n(plot.h),
            n(style.axis_width)
        );
    } else {
        let right_spine = if sy2.is_some() || panel.y2conv.is_some() {
            format!("V{}", n(plot.y))
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "\n<path d=\"M{} {}V{}H{}{right_spine}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\" stroke-linecap=\"square\"/>",
            n(plot.x),
            n(plot.y),
            n(plot.bottom()),
            n(plot.right()),
            n(style.axis_width)
        );
    }
    // Ticks point into the plot (paper) or out of it; minor ticks on log axes always.
    let dir = if style.ticks_inward { 1.0 } else { -1.0 };
    let mut major = String::new();
    let mut minor = String::new();
    let no_minor = Vec::new();
    let xminor = if style.minor_ticks || panel.logx {
        &xt.minor
    } else {
        &no_minor
    };
    let yminor = if style.minor_ticks || panel.logy {
        &yt.minor
    } else {
        &no_minor
    };
    for (vals, len, d) in [
        (&xt.major, style.major_tick, &mut major),
        (xminor, style.minor_tick, &mut minor),
    ] {
        for v in vals.iter().filter(|v| sx.contains(**v)) {
            let x = sx.map(*v);
            let _ = write!(d, "M{} {}v{}", n(x), n(plot.bottom()), n(-len * dir));
            // The top side mirrors the bottom, unless it has its own axis.
            if style.full_frame && panel.x2.is_none() {
                let _ = write!(d, "M{} {}v{}", n(x), n(plot.y), n(len * dir));
            }
        }
    }
    for (vals, len, d) in [
        (&yt.major, style.major_tick, &mut major),
        (yminor, style.minor_tick, &mut minor),
    ] {
        for v in vals.iter().filter(|v| sy.contains(**v)) {
            let y = sy.map(*v);
            let _ = write!(d, "M{} {}h{}", n(plot.x), n(y), n(len * dir));
            // The right side mirrors the left, unless it has its own axis.
            if style.full_frame && sy2.is_none() && panel.y2conv.is_none() {
                let _ = write!(d, "M{} {}h{}", n(plot.right()), n(y), n(-len * dir));
            }
        }
    }
    if let (Some(t), Some(s2)) = (&y2t, &sy2) {
        let y2minor = if style.minor_ticks || panel.logy2 {
            &t.minor
        } else {
            &no_minor
        };
        for (vals, len, d) in [
            (&t.major, style.major_tick, &mut major),
            (y2minor, style.minor_tick, &mut minor),
        ] {
            for v in vals.iter().filter(|v| s2.contains(**v)) {
                let _ = write!(
                    d,
                    "M{} {}h{}",
                    n(plot.right()),
                    n(s2.map(*v)),
                    n(-len * dir)
                );
            }
        }
    }
    for (d, width) in [(major, style.tick_width), (minor, style.minor_tick_width)] {
        if !d.is_empty() {
            let _ = write!(
                out,
                "\n<path d=\"{d}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
                n(width)
            );
        }
    }
    // A broken axis: the axis line is cut at the gap, with a slanted stroke at each end.
    if let Some((a, b)) = ybreak {
        let (top, bottom) = (sy.map(b), sy.map(a));
        let sides: &[f64] = if style.full_frame {
            &[plot.x, plot.right()]
        } else {
            &[plot.x]
        };
        for &x in sides {
            let _ = write!(
                out,
                "\n<path d=\"M{} {}V{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
                n(x),
                n(top + 0.5),
                n(bottom - 0.5),
                style.background.hex(),
                n(style.axis_width + 1.5)
            );
            let _ = write!(
                out,
                "\n<path d=\"M{l} {}L{r} {}M{l} {}L{r} {}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
                n(top + 1.6),
                n(top - 1.6),
                n(bottom + 1.6),
                n(bottom - 1.6),
                n(style.axis_width),
                l = n(x - 4.0),
                r = n(x + 4.0),
            );
        }
    }

    if let Some(axis) = &panel.x2 {
        draw_top_axis(out, axis, &sx, (xlo, xhi), plot, style, label_gap, dir);
    }
    if let Some(axis) = &panel.y2conv {
        draw_right_mapped_axis(out, axis, &sy, (ylo, yhi), plot, style, label_gap, dir);
    }

    // Tick labels.
    let cap = text::cap_height(font, tfs);
    let category_x = panel.xaxis == AxisKind::Category && panel.ycategories.is_empty();
    if !layout.hide_x {
        for (v, label) in xt.major.iter().zip(&xt.labels) {
            if sx.contains(*v) {
                let bold = category_x && panel.bold_categories.contains(&(*v as usize));
                bold_open(out, bold);
                text_at(
                    out,
                    font,
                    sx.map(*v),
                    plot.bottom() + label_gap + cap,
                    tfs,
                    "middle",
                    label,
                );
                bold_close(out, bold);
                if category_x && let Some(note) = panel.cat_notes.get(*v as usize) {
                    let nfs = note_size(style);
                    for (k, line) in note.lines().enumerate() {
                        text_at(
                            out,
                            font,
                            sx.map(*v),
                            plot.bottom() + label_gap + cap + nfs * 1.2 * (k as f64 + 1.0),
                            nfs,
                            "middle",
                            line,
                        );
                    }
                }
            }
        }
    }
    // With a second y axis, each side's numbers and label take its curve's colour (when
    // all the curves on that side share one), so it's clear which axis is whose.
    let left_color = side_color(panel, false);
    let right_color = side_color(panel, true);
    let open = |out: &mut String, c: Option<Color>| {
        if let Some(c) = c {
            let _ = write!(out, "\n<g fill=\"{}\">", c.hex());
        }
    };
    let close = |out: &mut String, c: Option<Color>| {
        if c.is_some() {
            out.push_str("\n</g>");
        }
    };
    if !layout.hide_y {
        open(out, left_color);
        let category_y = !panel.ycategories.is_empty();
        for (v, label) in yt.major.iter().zip(&yt.labels) {
            if sy.contains(*v) {
                let bold = category_y && panel.bold_categories.contains(&(*v as usize));
                let note = category_y
                    .then(|| panel.cat_notes.get(*v as usize))
                    .flatten()
                    .filter(|n| !n.is_empty());
                let nfs = note_size(style);
                // With a note under it, the name moves up so the pair stays centred.
                let lift = note.map_or(0.0, |n| nfs * 1.2 * n.lines().count() as f64 / 2.0);
                bold_open(out, bold);
                text_at(
                    out,
                    font,
                    plot.x - label_gap,
                    sy.map(*v) + cap / 2.0 - lift,
                    tfs,
                    "end",
                    label,
                );
                bold_close(out, bold);
                for (k, line) in note.into_iter().flat_map(|n| n.lines()).enumerate() {
                    text_at(
                        out,
                        font,
                        plot.x - label_gap,
                        sy.map(*v) + cap / 2.0 - lift + nfs * 1.2 * (k as f64 + 1.0),
                        nfs,
                        "end",
                        line,
                    );
                }
            }
        }
        close(out, left_color);
    }
    if let (Some(t), Some(s2)) = (&y2t, &sy2) {
        open(out, right_color);
        for (v, label) in t.major.iter().zip(&t.labels) {
            if s2.contains(*v) {
                text_at(
                    out,
                    font,
                    plot.right() + label_gap,
                    s2.map(*v) + cap / 2.0,
                    tfs,
                    "start",
                    label,
                );
            }
        }
        if let Some(offset) = &t.offset {
            draw_offset(out, font, offset, plot.right(), plot.y - 4.0, tfs, "end");
        }
        close(out, right_color);
    }

    // Axis labels, offsets and title.
    let row_baseline = cell.bottom() - style.pad - text::descent(font, fs) * 0.8;
    if !layout.hide_x {
        if let Some(label) = &panel.xlabel {
            text_at(
                out,
                font,
                plot.x + plot.w / 2.0,
                row_baseline,
                fs,
                "middle",
                label,
            );
        }
        if let Some(offset) = &xt.offset {
            draw_offset(out, font, offset, plot.right(), row_baseline, tfs, "end");
        }
    }
    if let Some(offset) = &yt.offset
        && !layout.hide_y
    {
        draw_offset(out, font, offset, plot.x, plot.y - 4.0, tfs, "start");
    }
    let rotated = |out: &mut String, x: f64, label: &str| {
        let y = plot.y + plot.h / 2.0;
        let _ = write!(
            out,
            "\n<g transform=\"translate({} {}) rotate(-90)\">",
            n(x),
            n(y)
        );
        text_at(out, font, 0.0, 0.0, fs, "middle", label);
        out.push_str("</g>");
    };
    if let Some(label) = &panel.ylabel
        && !layout.hide_y
    {
        open(out, left_color);
        rotated(
            out,
            cell.x + style.pad + text::cap_height(font, fs) * 1.05,
            label,
        );
        close(out, left_color);
    }
    if let Some(label) = panel
        .y2label
        .as_ref()
        .filter(|_| sy2.is_some() || panel.y2conv.is_some())
    {
        let legend_room = layout.legend_out.map_or(0.0, |(w, _)| w + LEGEND_OUT_GAP);
        open(out, right_color);
        rotated(
            out,
            cell.right() - style.pad - legend_room - text::descent(font, fs),
            label,
        );
        close(out, right_color);
    }
    let title_y = cell.y + style.pad + text::cap_height(font, style.title_font_size);
    if let Some(title) = &panel.title {
        text_at(
            out,
            font,
            plot.x + plot.w / 2.0,
            title_y,
            style.title_font_size,
            "middle",
            title,
        );
    }
    if let Some(letter) = letter {
        let _ = write!(
            out,
            "\n<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-weight=\"bold\">{}</text>",
            n(cell.x + style.pad),
            n(title_y),
            n(style.title_font_size),
            escape(letter)
        );
    }

    let plot_scales = Scales {
        x: &sx,
        y: &sy,
        y2: sy2.as_ref(),
    };
    let inset = panel
        .inset
        .and_then(|i| draw_inset(out, panel, &i, &plot_scales, plot, style, id));
    let mut legend_box = None;
    if panel.legend != LegendPos::Off && has_legend(panel) {
        let mut legend_plot = plot;
        if sy2.is_some() && panel.legend == LegendPos::Out {
            // Outside, the legend goes beyond the right axis's labels.
            legend_plot.w = cell.right()
                - style.pad
                - layout.legend_out.map_or(0.0, |(w, _)| w + LEGEND_OUT_GAP)
                - plot.x;
        }
        // Too wide for the panel: the fits' R² on a second line, then smaller text.
        let fitted = fit_legend(panel, legend_plot.w, style);
        let (lpanel, lstyle) = fitted.as_ref().map_or((panel, style), |(p, s)| (p, s));
        legend_box = Some(draw_legend(
            out,
            lpanel,
            legend_plot,
            &plot_scales,
            lstyle,
            layout.legend_out,
            inset,
        ));
    }
    // How much data the legend box hides (inside the plot only).
    let covers = match legend_box {
        Some(b) if panel.legend != LegendPos::Out => obstacles(panel, &plot_scales)
            .iter()
            .filter(|o| o.overlaps(&b))
            .count(),
        _ => 0,
    };
    // Rings and their labels last: the labels keep clear of the legend and the inset.
    let covered: Vec<Rect> = legend_box.into_iter().chain(inset).collect();
    draw_mark_points(out, panel, &plot_scales, plot, style, &covered);
    covers
}

/// Everything inside the plot area: regions, grids, filled things, reference lines, lines
/// and markers. Used for the panel and again for an inset.
fn draw_data(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    plot: Rect,
    style: &Style,
) {
    draw_spans(out, panel, sx, sy, sy2, plot, style);
    let scale_of = |s: &Series| -> &Scale {
        match (sy2, s.right) {
            (Some(r), true) => r,
            _ => sy,
        }
    };
    // Grids first (heatmap cells, filled contours, contour lines), then filled things
    // (areas, bars, histograms, boxes), then lines and points on top.
    for s in &panel.series {
        if let Some(g) = &s.grid {
            if s.kind == PlotKind::Heatmap {
                draw_heatmap(out, g, panel, sx, sy, style);
            } else {
                draw_contour(out, s, g, panel, sx, sy, style);
            }
        }
    }
    for s in panel
        .series
        .iter()
        .filter(|s| matches!(s.kind, PlotKind::Area | PlotKind::Kde))
    {
        draw_area(out, s, sx, scale_of(s), panel.stacked);
    }
    draw_mark_fills(out, panel, sx, sy, sy2, plot);
    draw_bars(out, panel, sx, sy, sy2);
    let hists = panel
        .series
        .iter()
        .filter(|s| s.kind == PlotKind::Hist)
        .count();
    for s in panel.series.iter().filter(|s| s.kind == PlotKind::Hist) {
        draw_hist(out, s, sx, scale_of(s), hists > 1 && !panel.stacked, style);
    }
    let boxes: Vec<&Series> = panel
        .series
        .iter()
        .filter(|s| s.kind == PlotKind::Box)
        .collect();
    for (k, s) in boxes.iter().enumerate() {
        let (each, shift) = if panel.box_grouped {
            let each = 0.8 / boxes.len() as f64;
            (each, -0.4 + each * (k as f64 + 0.5))
        } else {
            (0.7, 0.0)
        };
        for one in groups::each_colour(s) {
            draw_boxes(out, &one, sx, scale_of(s), each * 0.75, shift, style);
        }
    }
    let violins: Vec<&Series> = panel
        .series
        .iter()
        .filter(|s| s.kind == PlotKind::Violin)
        .collect();
    let widest = violins
        .iter()
        .flat_map(|s| s.boxes.iter().flat_map(|b| b.shape.iter().map(|p| p.1)))
        .fold(0.0, f64::max);
    for (k, s) in violins.iter().enumerate() {
        let (each, shift) = if panel.box_grouped {
            let each = 0.8 / violins.len() as f64;
            (each, -0.4 + each * (k as f64 + 0.5))
        } else {
            (0.8, 0.0)
        };
        for one in groups::each_colour(s) {
            draw_violins(out, &one, sx, scale_of(s), each * 0.9, shift, widest, style);
        }
    }
    draw_ref_lines(out, panel, sx, sy, sy2, plot, style);
    draw_shapes(out, panel, sx, sy, style);
    draw_mark_lines(out, panel, sx, sy, sy2, style);
    groups::draw_levels(out, panel, sx, sy, style);
    for s in &panel.series {
        let sy = scale_of(s);
        if s.band {
            draw_band(out, s, sx, sy);
        }
        match s.kind {
            PlotKind::Line | PlotKind::Fft => draw_line(out, s, sx, sy),
            PlotKind::Step => draw_line(out, &stepped(s), sx, sy),
            _ => {}
        }
        if (s.yerr.is_some() && !s.band) || s.xerr.is_some() {
            draw_error_bars(out, panel, s, sx, sy, style);
        }
        if s.marker != Marker::None
            && !matches!(s.kind, PlotKind::Box | PlotKind::Violin | PlotKind::Strip)
        {
            match &s.cvalues {
                Some(c) => draw_colored_markers(out, panel, s, c, sx, sy, style.marker_size),
                None => draw_markers(out, s, sx, sy, style.marker_size),
            }
        }
    }
    // Strips: upright here; on their side they're drawn after the turn (dots stay round,
    // triangles point up).
    if !panel.horizontal {
        groups::draw_strips(out, panel, sx, sy, style, false);
    }
}

/// The y range of the data with x inside `x` (left or right axis), with a little room;
/// in axis units (log10 on a log axis). None if nothing is there.
fn window_range(panel: &Panel, right: bool, x: (f64, f64), sy: &Scale) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut take = |v: f64| {
        let a = sy.axis(v);
        if a.is_finite() {
            lo = lo.min(a);
            hi = hi.max(a);
        }
    };
    for s in panel
        .series
        .iter()
        .filter(|s| s.right == right && s.grid.is_none())
    {
        let filled = matches!(s.kind, PlotKind::Bar | PlotKind::Hist | PlotKind::Area);
        for (k, (xv, yv)) in s.x.iter().zip(&s.y).enumerate() {
            if *xv < x.0 || *xv > x.1 {
                continue;
            }
            take(*yv);
            if let Some((lo, hi)) = s.yerr_at(k) {
                take(yv - lo.abs());
                take(yv + hi.abs());
            }
            if filled && !sy.log {
                take(0.0);
            }
        }
    }
    if !lo.is_finite() {
        return None;
    }
    let pad = if hi > lo {
        (hi - lo) * 0.08
    } else {
        lo.abs().max(1.0) * 0.1
    };
    Some((lo - pad, hi + pad))
}

/// `inset=5:8`: a zoomed copy of part of the plot in one of its corners (the emptiest,
/// or `inset_at=`), with a thin box on the zoomed region and two guide lines to the
/// inset. Returns the area it covers, for the legend to avoid.
#[allow(clippy::too_many_arguments)]
fn draw_inset(
    out: &mut String,
    panel: &Panel,
    inset: &Inset,
    scales: &Scales,
    plot: Rect,
    style: &Style,
    id: usize,
) -> Option<Rect> {
    let (sx, sy) = (scales.x, scales.y);
    let font = style.font;
    let tfs = style.tick_font_size * 0.85;
    let cap = text::cap_height(font, tfs);
    let fg = style.foreground.hex();
    let xr = (sx.axis(inset.x.0), sx.axis(inset.x.1));
    // The y range on the left axis, or on the right one when everything is there.
    let left = panel.series.iter().any(|s| !s.right && s.grid.is_none()) || scales.y2.is_none();
    let sy = if left { sy } else { scales.y2.unwrap_or(sy) };
    let yr = match inset.y {
        Some((a, b)) => (sy.axis(a), sy.axis(b)),
        None => window_range(panel, !left, inset.x, sy)?,
    };
    if ![xr.0, xr.1, yr.0, yr.1].iter().all(|v| v.is_finite()) || xr.1 <= xr.0 || yr.1 <= yr.0 {
        return None;
    }
    let xt = make_ticks(AxisKind::Number, xr.0, xr.1, 3, &[], sx.log, None);
    let yt = make_ticks(AxisKind::Number, yr.0, yr.1, 3, &[], sy.log, None);
    let label_w = yt
        .labels
        .iter()
        .map(|l| mathtext::width(font, l, tfs))
        .fold(0.0, f64::max);
    let (left_room, bottom_room) = (label_w + 4.0, cap + 5.0);
    let (iw, ih) = (plot.w * 0.38, plot.h * 0.36);
    let margin = 6.0;
    let outer_at = |corner: LegendPos| -> Rect {
        let (w, h) = (iw + left_room, ih + bottom_room);
        let x = match corner {
            LegendPos::TopLeft | LegendPos::BottomLeft => plot.x + margin,
            _ => plot.right() - margin - w,
        };
        let y = match corner {
            LegendPos::BottomLeft | LegendPos::BottomRight => plot.bottom() - margin - h,
            _ => plot.y + margin,
        };
        Rect::new(x, y, w, h)
    };
    // The zoomed region on the main plot.
    let zx = (sx.map_axis(xr.0), sx.map_axis(xr.1));
    let zy = (sy.map_axis(yr.1), sy.map_axis(yr.0));
    let zoom = Rect::new(
        zx.0.max(plot.x),
        zy.0.max(plot.y),
        (zx.1.min(plot.right()) - zx.0.max(plot.x)).max(1.0),
        (zy.1.min(plot.bottom()) - zy.0.max(plot.y)).max(1.0),
    );
    let corners = [
        LegendPos::TopRight,
        LegendPos::TopLeft,
        LegendPos::BottomRight,
        LegendPos::BottomLeft,
    ];
    let corner = inset.at.unwrap_or_else(|| {
        let obstacles = obstacles(panel, scales);
        corners
            .into_iter()
            .min_by_key(|c| {
                let r = outer_at(*c);
                let legend_here = panel.legend == *c;
                obstacles.iter().filter(|o| o.overlaps(&r)).count()
                    + usize::from(r.overlaps(&zoom)) * 100_000
                    + usize::from(legend_here && has_legend(panel)) * 50_000
            })
            .unwrap_or(LegendPos::TopRight)
    });
    let outer = outer_at(corner);
    let ir = Rect::new(outer.x + left_room, outer.y, iw, ih);

    // Box on the zoomed region, and guide lines from its corners to the inset's: the
    // pair of corners facing each other.
    let (a, b) = if ir.x > zoom.right() {
        (
            (zoom.right(), zoom.y, zoom.right(), zoom.bottom()),
            (ir.x, ir.y, ir.x, ir.bottom()),
        )
    } else if ir.right() < zoom.x {
        (
            (zoom.x, zoom.y, zoom.x, zoom.bottom()),
            (ir.right(), ir.y, ir.right(), ir.bottom()),
        )
    } else if ir.bottom() < zoom.y {
        (
            (zoom.x, zoom.y, zoom.right(), zoom.y),
            (ir.x, ir.bottom(), ir.right(), ir.bottom()),
        )
    } else {
        (
            (zoom.x, zoom.bottom(), zoom.right(), zoom.bottom()),
            (ir.x, ir.y, ir.right(), ir.y),
        )
    };
    let lw = style.axis_width * 0.7;
    let _ = write!(
        out,
        "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
        n(zoom.x),
        n(zoom.y),
        n(zoom.w),
        n(zoom.h),
        n(lw)
    );
    // An inset put on top of its own region (inset_at=) gets no guide lines.
    if !outer.overlaps(&zoom) {
        let _ = write!(
            out,
            "\n<path d=\"M{} {}L{} {}M{} {}L{} {}\" fill=\"none\" stroke=\"{fg}\" stroke-opacity=\"0.5\" stroke-width=\"{}\"/>",
            n(a.0),
            n(a.1),
            n(b.0),
            n(b.1),
            n(a.2),
            n(a.3),
            n(b.2),
            n(b.3),
            n(lw)
        );
    }

    // The inset: a plain background (so the data behind doesn't show), the data again on
    // its own scales, a frame, inward ticks and small labels.
    let _ = write!(
        out,
        "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
        n(outer.x - 2.0),
        n(outer.y - 2.0),
        n(outer.w + 4.0),
        n(outer.h + 4.0),
        style.background.hex()
    );
    let sxi = Scale {
        log: sx.log,
        ..Scale::linear(xr.0, xr.1, ir.x, ir.right())
    };
    let syi = Scale {
        log: sy.log,
        ..Scale::linear(yr.0, yr.1, ir.bottom(), ir.y)
    };
    let sy2i = if left {
        scales.y2.and_then(|s2| {
            window_range(panel, true, inset.x, s2)
                .map(|(lo, hi)| Scale::linear(lo, hi, ir.bottom(), ir.y))
        })
    } else {
        Some(syi)
    };
    let _ = write!(
        out,
        "\n<clipPath id=\"inset{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>\
         \n<g clip-path=\"url(#inset{id})\">",
        n(ir.x),
        n(ir.y),
        n(ir.w),
        n(ir.h)
    );
    draw_data(out, panel, &sxi, &syi, sy2i.as_ref(), ir, style);
    out.push_str("\n</g>");
    let len = style.major_tick * 0.7;
    let mut d = String::new();
    for v in xt.major.iter().filter(|v| sxi.contains(**v)) {
        let _ = write!(d, "M{} {}v{}", n(sxi.map(*v)), n(ir.bottom()), n(-len));
    }
    for v in yt.major.iter().filter(|v| syi.contains(**v)) {
        let _ = write!(d, "M{} {}h{}", n(ir.x), n(syi.map(*v)), n(len));
    }
    let _ = write!(
        out,
        "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>\
         \n<path d=\"{d}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
        n(ir.x),
        n(ir.y),
        n(ir.w),
        n(ir.h),
        n(style.axis_width),
        n(style.tick_width)
    );
    for (v, label) in xt.major.iter().zip(&xt.labels) {
        if sxi.contains(*v) {
            text_at(
                out,
                font,
                sxi.map(*v),
                ir.bottom() + 2.0 + cap,
                tfs,
                "middle",
                label,
            );
        }
    }
    for (v, label) in yt.major.iter().zip(&yt.labels) {
        if syi.contains(*v) {
            text_at(
                out,
                font,
                ir.x - 3.0,
                syi.map(*v) + cap / 2.0,
                tfs,
                "end",
                label,
            );
        }
    }
    Some(outer)
}

/// The scales of a panel, for things that look at all series (legend placement).
struct Scales<'a> {
    x: &'a Scale,
    y: &'a Scale,
    y2: Option<&'a Scale>,
}

impl Scales<'_> {
    fn y_of(&self, s: &Series) -> &Scale {
        match (self.y2, s.right) {
            (Some(r), true) => r,
            _ => self.y,
        }
    }
}

const LEGEND_ROW: f64 = 1.35;
const LEGEND_HANDLE: f64 = 20.0;
const LEGEND_GAP: f64 = 5.0;
const LEGEND_INNER: f64 = 4.0;
/// Space kept between a legend inside the plot and the plot's edge.
const LEGEND_MARGIN: f64 = 6.0;

/// A label, with `$…$` maths if it has any.
/// Labels that mix in fallback fonts are placed piece by piece from the left (see
/// `mathtext::to_svg`).
fn text_at(out: &mut String, font: Font, x: f64, y: f64, size: f64, anchor: &str, s: &str) {
    let (x, anchor, left) = if mathtext::needs_fallback(font, &mathtext::plain(s)) {
        let w = mathtext::width(font, s, size);
        let left = match anchor {
            "middle" => x - w / 2.0,
            "end" => x - w,
            _ => x,
        };
        (left, "start", Some(left))
    } else {
        (x, anchor, None)
    };
    let _ = write!(
        out,
        "\n<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"{anchor}\">{}</text>",
        n(x),
        n(y),
        n(size),
        mathtext::to_svg(font, s, size, left)
    );
}

/// Text in bold (a `highlight=`ed group's name).
fn bold_open(out: &mut String, on: bool) {
    if on {
        out.push_str("\n<g font-weight=\"bold\">");
    }
}

fn bold_close(out: &mut String, on: bool) {
    if on {
        out.push_str("\n</g>");
    }
}

/// How solid a filled shape is: `alpha=` as written (1 is solid), else the shape's own
/// see-through default.
fn fill_alpha(s: &Series, default: f64) -> f64 {
    if s.alpha_set { s.alpha } else { default }
}

fn opacity(what: &str, alpha: f64) -> String {
    if alpha < 1.0 {
        format!(" {what}-opacity=\"{}\"", n(alpha))
    } else {
        String::new()
    }
}

/// A coordinate with at most two decimals and no trailing zeros.
fn n(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

// ---------- M6: grids, violins and polar plots ----------

const CB_GAP: f64 = 10.0;
const CB_WIDTH: f64 = 10.0;

/// A point of a triangle: page position and value.
type Corner = (f64, f64, f64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_formats() {
        let pct = NumberFormat::parse("%.0f%").unwrap();
        assert_eq!(format_value(41.2, Some(&pct)), "41%");
        assert_eq!(format_value(0.3, Some(&pct)), "<1%");
        assert_eq!(format_value(0.0, Some(&pct)), "0%");
        // Exactly half rounds to an even "0" when written: still "<1%" (holding pressure).
        assert_eq!(format_value(0.5, Some(&pct)), "<1%");
        assert_eq!(format_value(-0.5, Some(&pct)), ">\u{2212}1%");
        assert_eq!(format_value(-0.2, Some(&pct)), ">\u{2212}1%");
        let one = NumberFormat::parse("%.1f").unwrap();
        assert_eq!(format_value(-2.25, Some(&one)), "\u{2212}2.2");
        assert_eq!(format_value(9.76, None), "9.76");
    }

    #[test]
    fn number_formatting() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(1.256), "1.26");
        assert_eq!(n(-0.001), "0");
        assert_eq!(n(10.5), "10.5");
    }

    #[test]
    fn decimation_keeps_extremes() {
        let xs: Vec<f64> = (0..100_000).map(|i| i as f64).collect();
        let ys: Vec<f64> = (0..100_000)
            .map(|i| if i == 54_321 { 10.0 } else { (i % 7) as f64 })
            .collect();
        let sx = Scale::linear(0.0, 100_000.0, 0.0, 400.0);
        let sy = Scale::linear(0.0, 10.0, 100.0, 0.0);
        let pts = decimate(&xs, &ys, &sx, &sy);
        assert!(pts.len() < 10_000, "{}", pts.len());
        // The single spike survives.
        assert!(pts.iter().flatten().any(|(_, py)| *py == 0.0));
    }

    #[test]
    fn log_scale() {
        let s = Scale {
            log: true,
            ..Scale::linear(0.0, 2.0, 0.0, 200.0)
        };
        assert_eq!(s.map(10.0), 100.0);
        assert!(s.map(0.0).is_nan());
        assert!(s.contains(100.0) && !s.contains(1000.0));
    }

    #[test]
    fn escaping() {
        assert_eq!(escape("a<b & \"c\""), "a&lt;b &amp; &quot;c&quot;");
    }
}
