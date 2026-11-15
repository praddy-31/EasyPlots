//! Axis ranges and ticks: the extent of the data, room for bars, log ranges, and the extra
//! axes (a mapped right axis, a top axis, the offset note).

use super::*;

/// A tick step chosen in the command: every `step` from `from` (the range's lower limit
/// if given, else zero).
#[derive(Clone, Copy)]
pub(super) struct Step {
    pub(super) step: f64,
    pub(super) from: f64,
}

pub(super) fn step_of(step: Option<f64>, lim: (Option<f64>, Option<f64>)) -> Option<Step> {
    step.map(|step| Step {
        step,
        from: lim.0.unwrap_or(0.0),
    })
}

pub(super) fn make_ticks(
    kind: AxisKind,
    lo: f64,
    hi: f64,
    target: usize,
    categories: &[String],
    log: bool,
    step: Option<Step>,
) -> Ticks {
    let target = target.clamp(3, 9);
    if log {
        return ticks::log(lo, hi, target);
    }
    if let (AxisKind::Number, Some(s)) = (kind, step) {
        return ticks::stepped(lo, hi, s.step, s.from);
    }
    match kind {
        AxisKind::Number => ticks::linear(lo, hi, target),
        AxisKind::Date => ticks::dates(lo, hi, target),
        AxisKind::Category => ticks::categories(categories, target),
    }
}

/// X ticks whose labels don't overlap: start with roughly one label per 60 pt and back
/// off until they fit.
pub(super) fn fitting_x_ticks(panel: &Panel, lo: f64, hi: f64, width: f64, style: &Style) -> Ticks {
    let tfs = style.tick_font_size;
    let mut target = (width / 60.0) as usize;
    if panel.xaxis == AxisKind::Category {
        target = panel.categories.len().clamp(1, 60);
    }
    loop {
        let t = make_ticks(
            panel.xaxis,
            lo,
            hi,
            target,
            &panel.categories,
            panel.logx,
            step_of(panel.xstep, panel.xlim),
        );
        // The owner's own step is kept even if its labels are crowded.
        if panel.xstep.is_some() {
            return t;
        }
        let needed: f64 = t
            .labels
            .iter()
            .map(|l| mathtext::width(style.font, l, tfs) + tfs)
            .sum();
        if needed <= width || target <= 2 {
            return t;
        }
        target -= 1;
    }
}

pub(super) fn finite_extent(values: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    values
        .filter(|v| v.is_finite())
        .fold(None, |acc, v| match acc {
            None => Some((v, v)),
            Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
        })
}

pub(super) fn widen_if_flat(lo: f64, hi: f64) -> (f64, f64) {
    if hi > lo {
        return (lo, hi);
    }
    let d = if lo == 0.0 { 1.0 } else { lo.abs() * 0.1 };
    (lo - d, hi + d)
}

/// Smallest gap between neighbouring x values of the bars (their slot width).
pub(super) fn bar_slot(panel: &Panel) -> f64 {
    if panel.xaxis == AxisKind::Category {
        return 1.0;
    }
    let mut xs: Vec<f64> = panel
        .series
        .iter()
        .filter(|s| s.kind == PlotKind::Bar)
        .flat_map(|s| s.x.iter().copied())
        .filter(|v| v.is_finite())
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs.windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 0.0)
        .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.min(d))))
        .unwrap_or(if panel.xaxis == AxisKind::Date {
            86400.0
        } else {
            1.0
        })
}

/// How far the middle of series `s`'s bars sits from its x value (bars side by side).
pub(super) fn bar_offset(panel: &Panel, s: &Series) -> f64 {
    if s.kind != PlotKind::Bar || panel.stacked {
        return 0.0;
    }
    let bars: Vec<&Series> = panel
        .series
        .iter()
        .filter(|b| b.kind == PlotKind::Bar)
        .collect();
    let Some(k) = bars.iter().position(|b| std::ptr::eq(*b, s)) else {
        return 0.0;
    };
    let slot = bar_slot(panel) * 0.8;
    let each = slot / bars.len() as f64;
    -slot / 2.0 + each * (k as f64 + 0.5)
}

pub(super) fn has_bars(panel: &Panel) -> bool {
    panel.series.iter().any(|s| s.kind == PlotKind::Bar)
}

/// Axis ends for a log axis, in powers of ten: the positive values, plus a 5 % margin.
pub(super) fn log_range(
    values: impl Iterator<Item = f64>,
    lim: (Option<f64>, Option<f64>),
) -> (f64, f64) {
    let (lo, hi) = finite_extent(values.filter(|v| *v > 0.0).map(f64::log10)).unwrap_or((0.0, 1.0));
    let (lo, hi) = if hi > lo {
        (lo, hi)
    } else {
        (lo - 0.5, hi + 0.5)
    };
    let m = (hi - lo) * 0.05;
    let (mut lo, mut hi) = (lo - m, hi + m);
    if let Some(v) = lim.0.filter(|v| *v > 0.0) {
        lo = v.log10();
    }
    if let Some(v) = lim.1.filter(|v| *v > 0.0) {
        hi = v.log10();
    }
    if hi > lo {
        (lo, hi)
    } else {
        (lo - 0.5, lo + 0.5)
    }
}

/// All x positions something is drawn at (with error bars).
pub(super) fn x_values(panel: &Panel) -> Vec<f64> {
    // Reference lines and regions count, so they're always in view.
    let mut v: Vec<f64> = panel.vlines.clone();
    v.extend(panel.spans.iter().flat_map(|(a, b)| [*a, *b]));
    v.extend(
        panel
            .shapes
            .iter()
            .flat_map(|s| shape_points(s).map(|p| p.0)),
    );
    for s in &panel.series {
        if let Some(g) = &s.grid {
            v.extend(&g.xs);
        }
        v.extend(s.x.iter().copied());
        if let Some(e) = &s.xerr {
            for (x, e) in s.x.iter().zip(e) {
                v.push(x - e);
                v.push(x + e);
            }
        }
    }
    v
}

pub(super) fn x_range(panel: &Panel) -> (f64, f64) {
    if panel.logx {
        return log_range(x_values(panel).into_iter(), panel.xlim);
    }
    let hist = panel.series.iter().all(|s| s.kind == PlotKind::Hist);
    let grids = panel.series.iter().all(|s| s.grid.is_some());
    let (mut lo, mut hi) = if panel.xaxis == AxisKind::Category && grids {
        (-0.5, panel.categories.len() as f64 - 0.5)
    } else if grids {
        finite_extent(x_values(panel).into_iter()).unwrap_or((0.0, 1.0))
    } else if panel.xaxis == AxisKind::Category {
        (-0.6, panel.categories.len() as f64 - 0.4)
    } else {
        let (lo, hi) = finite_extent(x_values(panel).into_iter()).unwrap_or((0.0, 1.0));
        let (mut lo, mut hi) = widen_if_flat(lo, hi);
        if has_bars(panel) {
            let half = bar_slot(panel) * 0.6;
            lo -= half;
            hi += half;
        } else if !hist && panel.series.iter().any(|s| s.marker != Marker::None) {
            let m = (hi - lo) * 0.05;
            lo -= m;
            hi += m;
        }
        (lo, hi)
    };
    if let Some(v) = panel.xlim.0 {
        lo = v;
    }
    if let Some(v) = panel.xlim.1 {
        hi = v;
    }
    widen_if_flat(lo, hi)
}

/// Every y value drawn on one side (left or right axis): points, error bar ends, the lower
/// edges of areas, and box whiskers and outliers. Only points whose x is drawable count.
pub(super) fn y_values(panel: &Panel, right: bool) -> Vec<f64> {
    let mut v = Vec::new();
    if right {
        v.extend(&panel.hlines2);
        v.extend(panel.yspans2.iter().flat_map(|(a, b)| [*a, *b]));
    } else {
        v.extend(&panel.hlines);
        v.extend(panel.yspans.iter().flat_map(|(a, b)| [*a, *b]));
        v.extend(
            panel
                .shapes
                .iter()
                .flat_map(|s| shape_points(s).map(|p| p.1)),
        );
    }
    for s in panel.series.iter().filter(|s| s.right == right) {
        if let Some(g) = &s.grid {
            v.extend(&g.ys);
        }
        let x_ok = |x: &f64| !panel.logx || *x > 0.0 || s.kind == PlotKind::Hist;
        for (i, (x, y)) in s.x.iter().zip(&s.y).enumerate() {
            if !x_ok(x) {
                continue;
            }
            v.push(*y);
            if let Some((lo, hi)) = s.yerr_at(i) {
                v.push(y - lo);
                v.push(y + hi);
            }
            if let Some(b) = &s.base {
                v.push(b[i]);
            }
        }
        for b in &s.boxes {
            v.extend([b.low, b.high]);
            v.extend(&b.outliers);
        }
    }
    v
}

pub(super) fn y_range(panel: &Panel, right: bool) -> (f64, f64) {
    let lim = if right { panel.y2lim } else { panel.ylim };
    let ys = y_values(panel, right);
    // Heatmap rows and contour grids fill the plot exactly.
    if !right && panel.series.iter().all(|s| s.grid.is_some()) {
        let (mut lo, mut hi) = if panel.ycategories.is_empty() {
            finite_extent(ys.into_iter()).unwrap_or((0.0, 1.0))
        } else {
            (-0.5, panel.ycategories.len() as f64 - 0.5)
        };
        if let Some(v) = lim.0 {
            lo = v;
        }
        if let Some(v) = lim.1 {
            hi = v;
        }
        return widen_if_flat(lo, hi);
    }
    if (panel.logy && !right) || (panel.logy2 && right) {
        return log_range(ys.into_iter(), lim);
    }
    let (dlo, dhi) = finite_extent(ys.into_iter()).unwrap_or((0.0, 1.0));
    // Bars, histograms and areas sit on zero.
    let zero = panel.series.iter().any(|s| {
        s.right == right
            && (matches!(s.kind, PlotKind::Bar | PlotKind::Hist | PlotKind::Kde)
                || (s.kind == PlotKind::Area
                    && s.base.as_ref().is_none_or(|b| b.iter().all(|v| *v == 0.0))))
    });
    let (dlo, dhi) = if zero {
        (dlo.min(0.0), dhi.max(0.0))
    } else {
        (dlo, dhi)
    };
    let (dlo, dhi) = widen_if_flat(dlo, dhi);
    let m = (dhi - dlo) * 0.05;
    // Numbers written over the bars or points need a little more room at the top.
    let top = if panel.values { m * 2.4 } else { m };
    // No gap below zero (or above, for all-negative bars).
    let mut lo = if zero && dlo == 0.0 { 0.0 } else { dlo - m };
    let mut hi = if zero && dhi == 0.0 { 0.0 } else { dhi + top };
    if let Some(v) = lim.0 {
        lo = v;
    }
    if let Some(v) = lim.1 {
        hi = v;
    }
    widen_if_flat(lo, hi)
}

/// The colour for one side's y axis numbers and label when the panel has a second y
/// axis (`axis=right`) and every data curve on that side has the same colour; None
/// otherwise, or with `axiscolor=off`.
pub(super) fn side_color(panel: &Panel, right: bool) -> Option<Color> {
    if !panel.axis_color || !panel.has_right_axis() {
        return None;
    }
    let mut colors = panel
        .series
        .iter()
        .filter(|s| s.right == right && !s.is_fit && s.grid.is_none())
        .map(|s| s.color);
    let first = colors.next()?;
    colors.all(|c| c == first).then_some(first)
}

/// Ticks for an axis in other units (`x2=`, `y2=`): `(lo, hi)` is the main axis's range
/// as drawn (log10 values on a log axis). Returns the ticks in the other units and the
/// main axis's range in data units, for placing them with `TopAxis::inverse`.
pub(super) fn mapped_ticks(
    axis: &TopAxis,
    log: bool,
    (lo, hi): (f64, f64),
    target: usize,
) -> Option<(Ticks, (f64, f64))> {
    let data = |v: f64| if log { 10f64.powf(v) } else { v };
    let samples: Vec<f64> = (0..=64)
        .map(|i| data(lo + (hi - lo) * f64::from(i) / 64.0))
        .collect();
    let vals = axis.values(&samples).ok()?;
    let finite = vals.iter().copied().filter(|v| v.is_finite());
    let vmin = finite.clone().fold(f64::INFINITY, f64::min);
    let vmax = finite.fold(f64::NEG_INFINITY, f64::max);
    if !(vmin.is_finite() && vmax > vmin) {
        return None;
    }
    // On a log main axis a power law (1/x) is straight, so the other gets log ticks too.
    let t = if log && vmin > 0.0 {
        make_ticks(
            AxisKind::Number,
            vmin.log10(),
            vmax.log10(),
            target,
            &[],
            true,
            None,
        )
    } else {
        make_ticks(AxisKind::Number, vmin, vmax, target, &[], false, None)
    };
    Some((t, (data(lo), data(hi))))
}

/// `y2="y*1.8+32"`: a right axis whose values are the left's through a formula; ticks
/// and labels on the right side (its label is drawn with `y2label`).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_right_mapped_axis(
    out: &mut String,
    axis: &TopAxis,
    sy: &Scale,
    (ylo, yhi): (f64, f64),
    plot: Rect,
    style: &Style,
    label_gap: f64,
    dir: f64,
) {
    let font = style.font;
    let tfs = style.tick_font_size;
    let fg = style.foreground.hex();
    let Some((t, (lo, hi))) = mapped_ticks(axis, sy.log, (ylo, yhi), (plot.h / 40.0) as usize)
    else {
        return;
    };
    let at = |v: f64| {
        axis.inverse(v, lo, hi)
            .map(|y| sy.map(y))
            .filter(|y| y.is_finite())
    };
    let mut major = String::new();
    let mut minor = String::new();
    let no_minor = Vec::new();
    let tminor = if style.minor_ticks || sy.log {
        &t.minor
    } else {
        &no_minor
    };
    for (vals, len, d) in [
        (&t.major, style.major_tick, &mut major),
        (tminor, style.minor_tick, &mut minor),
    ] {
        for y in vals.iter().filter_map(|v| at(*v)) {
            let _ = write!(d, "M{} {}h{}", n(plot.right()), n(y), n(-len * dir));
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
    // Labels; on a curved scale ticks can bunch up, so skip any that would overlap.
    let cap = text::cap_height(font, tfs);
    let mut placed: Vec<(f64, &String)> = t
        .major
        .iter()
        .zip(&t.labels)
        .filter_map(|(v, l)| at(*v).map(|y| (y, l)))
        .collect();
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut last = f64::NEG_INFINITY;
    for (y, label) in placed {
        if y - cap < last + 3.0 {
            continue;
        }
        last = y;
        text_at(
            out,
            font,
            plot.right() + label_gap,
            y + cap / 2.0,
            tfs,
            "start",
            label,
        );
    }
}

/// `x2="1240/x"`: a second x axis along the top whose values are the bottom's through a
/// formula. Its ticks are round numbers of the top axis, placed where the bottom axis has
/// the matching value.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_top_axis(
    out: &mut String,
    axis: &TopAxis,
    sx: &Scale,
    (xlo, xhi): (f64, f64),
    plot: Rect,
    style: &Style,
    label_gap: f64,
    dir: f64,
) {
    let font = style.font;
    let tfs = style.tick_font_size;
    let fg = style.foreground.hex();
    let Some((t, (lo, hi))) = mapped_ticks(axis, sx.log, (xlo, xhi), (plot.w / 60.0) as usize)
    else {
        return;
    };
    let at = |v: f64| {
        axis.inverse(v, lo, hi)
            .map(|x| sx.map(x))
            .filter(|x| x.is_finite())
    };
    let mut major = String::new();
    let mut minor = String::new();
    let no_minor = Vec::new();
    let tminor = if style.minor_ticks || sx.log {
        &t.minor
    } else {
        &no_minor
    };
    for (vals, len, d) in [
        (&t.major, style.major_tick, &mut major),
        (tminor, style.minor_tick, &mut minor),
    ] {
        for x in vals.iter().filter_map(|v| at(*v)) {
            let _ = write!(d, "M{} {}v{}", n(x), n(plot.y), n(len * dir));
        }
    }
    // Screen themes draw only the left and bottom lines; the top axis needs its own.
    if !style.full_frame {
        let _ = write!(major, "M{} {}H{}", n(plot.x), n(plot.y), n(plot.right()));
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
    // Labels; on a curved scale (1/x) ticks can bunch up, so skip any that would overlap.
    let base = plot.y - label_gap - text::descent(font, tfs) * 0.6;
    let mut last_right = f64::NEG_INFINITY;
    let mut placed: Vec<(f64, &String)> = t
        .major
        .iter()
        .zip(&t.labels)
        .filter_map(|(v, l)| at(*v).map(|x| (x, l)))
        .collect();
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (x, label) in placed {
        let w = mathtext::width(font, label, tfs);
        if x - w / 2.0 < last_right + 4.0 {
            continue;
        }
        last_right = x + w / 2.0;
        text_at(out, font, x, base, tfs, "middle", label);
    }
    if let Some(label) = &axis.label {
        let fs = style.font_size;
        let y = base - text::cap_height(font, tfs) - fs * 0.45 - text::descent(font, fs);
        text_at(out, font, plot.x + plot.w / 2.0, y, fs, "middle", label);
    }
}

pub(super) fn draw_offset(
    out: &mut String,
    font: Font,
    offset: &Offset,
    x: f64,
    y: f64,
    size: f64,
    anchor: &str,
) {
    match offset {
        Offset::Text(t) => text_at(out, font, x, y, size, anchor, t),
        Offset::Power(p) => {
            let exp = ticks::format_number(f64::from(*p), 0);
            text_at(out, font, x, y, size, anchor, &format!("×10$^{{{exp}}}$"));
        }
    }
}

/// Y ticks: heatmap row names, or numbers.
pub(super) fn y_ticks(panel: &Panel, lo: f64, hi: f64, target: usize) -> Ticks {
    if let Some((a, b)) = panel
        .ybreak
        .filter(|(a, b)| lo < *a && *b < hi && !panel.logy && panel.ycategories.is_empty())
    {
        // One step for both parts, chosen for the range that is actually shown; no ticks
        // inside the gap.
        let step = match step_of(panel.ystep, panel.ylim) {
            Some(s) => s.step,
            None => {
                let shown = ticks::linear(lo, hi - (b - a), target.clamp(3, 9));
                match shown.major.as_slice() {
                    [first, second, ..] => second - first,
                    _ => (hi - lo - (b - a)) / 4.0,
                }
            }
        };
        let mut t = ticks::stepped(lo, hi, step, 0.0);
        let outside = |v: f64| v <= a || v >= b;
        // Numbers at both edges of the gap would sit on top of each other: the lower edge
        // keeps its number, the upper one becomes a small tick.
        let eps = step * 1e-6;
        let keep: Vec<bool> = t
            .major
            .iter()
            .map(|v| *v < a + eps || *v > b + eps)
            .collect();
        let upper_edge = t.major.iter().any(|v| (*v - b).abs() <= eps);
        let mut k = keep.iter();
        t.major.retain(|_| *k.next().unwrap_or(&true));
        let mut k = keep.iter();
        t.labels.retain(|_| *k.next().unwrap_or(&true));
        t.minor.retain(|v| outside(*v));
        if upper_edge {
            t.minor.push(b);
        }
        return t;
    }
    if panel.ycategories.is_empty() {
        make_ticks(
            AxisKind::Number,
            lo,
            hi,
            target,
            &[],
            panel.logy,
            step_of(panel.ystep, panel.ylim),
        )
    } else {
        // Row names need about a line each; `target` assumes one label per 40 pt.
        ticks::categories(&panel.ycategories, target * 3)
    }
}
