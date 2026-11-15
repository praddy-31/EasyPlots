//! Fitted curves and numbers read off curves: fits and their residuals, slope, integral,
//! peaks, onsets, crossings, and correlation.

use super::*;

/// `peaks`, `dips`, `onset` and `area=`: read numbers off each data curve (not fits),
/// return the marks to draw, add a line per finding for the Terminal, and put areas in the
/// legend under the curve's name.
pub(super) fn read_curves(
    spec: &PanelSpec,
    series: &mut [Series],
    xaxis: AxisKind,
    xname: String,
    fg: Color,
    reports: &mut Vec<String>,
) -> Result<Vec<Mark>, Error> {
    use crate::analysis;
    use crate::data::short;
    use crate::render::mathtext::plain;
    if let (Some(l), None) = (&spec.cross_label, &spec.cross) {
        return Err(Error::at(
            "crosslabel says what cross writes beside each crossing: add cross=curves (or cross=50)",
            l.span.clone(),
        ));
    }
    let asked: Vec<(&str, &std::ops::Range<usize>)> = [
        spec.peaks.as_ref().map(|s| ("peaks", &s.span)),
        spec.dips.as_ref().map(|s| ("dips", &s.span)),
        spec.onset.as_ref().map(|s| ("onset", &s.span)),
        spec.area.as_ref().map(|s| ("area", &s.span)),
        spec.cross.as_ref().map(|s| ("cross", &s.span)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let Some((first, span)) = asked.first() else {
        return Ok(Vec::new());
    };
    if xaxis != AxisKind::Number {
        return Err(Error::at(
            format!("{first} needs numbers on the x axis (not dates or names)"),
            (*span).clone(),
        ));
    }
    let curves: Vec<usize> = (0..series.len())
        .filter(|&i| {
            let s = &series[i];
            !s.is_fit
                && matches!(
                    s.kind,
                    PlotKind::Line
                        | PlotKind::Scatter
                        | PlotKind::Step
                        | PlotKind::Area
                        | PlotKind::Kde
                        | PlotKind::Fft
                )
        })
        .collect();
    if curves.is_empty() {
        return Err(Error::at(
            format!(
                "{first} reads numbers off curves: use it with line, scatter, step, area, kde or fft"
            ),
            (*span).clone(),
        ));
    }
    // `peaks=storage`: only the curves named (as in the legend); a name that isn't there is
    // refused with the names that are.
    let names: Vec<String> = curves.iter().map(|&i| plain(&series[i].label)).collect();
    for asked in [&spec.read_on.peaks, &spec.read_on.dips, &spec.read_on.onset] {
        for n in asked.iter() {
            if !names.iter().any(|c| c.eq_ignore_ascii_case(&n.value)) {
                return Err(Error::at(
                    format!(
                        "no curve called \"{}\" here (curves: {})",
                        n.value,
                        names.join(", ")
                    ),
                    n.span.clone(),
                ));
            }
        }
    }
    let reads = |list: &[Spanned<String>], label: &str| {
        list.is_empty() || list.iter().any(|n| n.value.eq_ignore_ascii_case(label))
    };
    let cross_label = spec.cross_label.as_ref().map_or(CrossLabel::X, |l| l.value);
    let many = curves.len() > 1;
    let xname = plain(&xname);
    let mut marks = Vec::new();
    for i in curves {
        let s = &series[i];
        let (color, right) = (s.color, s.right);
        let who = if many {
            format!("{}: ", plain(&s.label))
        } else {
            String::new()
        };
        let name = plain(&s.label);
        for (n, dips, on) in [
            (&spec.peaks, false, &spec.read_on.peaks),
            (&spec.dips, true, &spec.read_on.dips),
        ] {
            let Some(n) = n else { continue };
            if !reads(on, &name) {
                continue;
            }
            let what = if dips { "dip" } else { "peak" };
            let found = analysis::peaks(&s.x, &s.y, n.value, dips);
            if found.is_empty() {
                reports.push(format!("{who}no {what} found"));
            }
            for p in found {
                reports.push(format!(
                    "{who}{what} at {xname} = {} ({} = {})",
                    short(p.x),
                    plain(&s.label),
                    short(p.y)
                ));
                marks.push(Mark::Point {
                    x: p.x,
                    y: p.y,
                    label: short(p.x),
                    color,
                    right,
                });
            }
        }
        if let Some(o) = spec
            .onset
            .as_ref()
            .filter(|_| reads(&spec.read_on.onset, &name))
        {
            let found = analysis::onset(&s.x, &s.y, o.value).map_err(|m| {
                Error::at(
                    format!("onset of \"{}\": {m}", plain(&s.label)),
                    o.span.clone(),
                )
            })?;
            reports.push(format!("{who}onset at {xname} = {}", short(found.x)));
            let reach = (found.steepest.0 - found.x).abs().max(f64::MIN_POSITIVE);
            let (b0, b1) = (found.base_range.0.min(found.x), found.x + 0.3 * reach);
            let (t0, t1) = (found.x - 0.3 * reach, found.steepest.0 + 0.6 * reach);
            // Kept within the curve's own height, so a steep tangent doesn't run off the plot.
            let (ylo, yhi) =
                s.y.iter()
                    .filter(|v| v.is_finite())
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
                        (a.min(v), b.max(v))
                    });
            for (a, b, line) in [(b0, b1, found.baseline), (t0, t1, found.tangent)] {
                let (a, b) = within(line, (a, b), (ylo, yhi));
                marks.push(Mark::Line {
                    from: (a, line.y(a)),
                    to: (b, line.y(b)),
                    color,
                    right,
                });
            }
            marks.push(Mark::Point {
                x: found.x,
                y: found.y,
                label: format!("onset {}", short(found.x)),
                color,
                right,
            });
        }
        if let Some(a) = &spec.area {
            let (total, outline) = analysis::area(&s.x, &s.y, a.value).map_err(|m| {
                Error::at(
                    format!("area of \"{}\": {m}", plain(&s.label)),
                    a.span.clone(),
                )
            })?;
            let (lo, hi) = (outline[0].0, outline[outline.len() - 1].0);
            reports.push(format!(
                "{who}area from {xname} = {} to {} = {}",
                short(lo),
                short(hi),
                short(total)
            ));
            let text = format!("area {}", short(total));
            let s = &mut series[i];
            s.note = Some(match s.note.take() {
                Some(n) => format!("{n} · {text}"),
                None => text,
            });
            marks.push(Mark::Fill {
                outline,
                color,
                right,
            });
        }
        if let Some(Spanned {
            value: Cross::Levels(levels),
            ..
        }) = &spec.cross
        {
            let s = &series[i];
            for &level in levels {
                let found = analysis::level_crossings(&s.x, &s.y, level);
                cross_report(
                    reports,
                    &format!("{who}crosses {}", short(level)),
                    &xname,
                    &found,
                    false,
                );
                if found.is_empty() {
                    let (lo, hi) =
                        s.y.iter()
                            .filter(|v| v.is_finite())
                            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
                                (a.min(v), b.max(v))
                            });
                    if lo <= hi
                        && (level < lo || level > hi)
                        && let Some(last) = reports.last_mut()
                    {
                        last.push_str(&format!(
                            " (the curve runs from {} to {})",
                            short(lo),
                            short(hi)
                        ));
                    }
                }
                for (x, y) in found.into_iter().take(MAX_CROSSINGS) {
                    marks.push(Mark::Point {
                        x,
                        y,
                        label: cross_label.text(x, y),
                        color,
                        right,
                    });
                }
            }
        }
    }
    // cross: every pair of curves on the same y axis, ringed in the text colour.
    if let Some(Spanned {
        value: Cross::Curves,
        span,
    }) = &spec.cross
    {
        let data: Vec<usize> = (0..series.len())
            .filter(|&i| {
                let s = &series[i];
                !s.is_fit
                    && s.grid.is_none()
                    && s.boxes.is_empty()
                    && !matches!(s.kind, PlotKind::Hist | PlotKind::Bar)
            })
            .collect();
        let pairs: Vec<(usize, usize)> = data
            .iter()
            .enumerate()
            .flat_map(|(k, &a)| data[k + 1..].iter().map(move |&b| (a, b)))
            .filter(|(a, b)| series[*a].right == series[*b].right)
            .collect();
        if pairs.is_empty() {
            return Err(Error::at(
                "cross marks where two curves cross: plot at least two on the same y axis \
                 (or cross=50 for where a curve crosses y = 50)",
                span.clone(),
            ));
        }
        for (a, b) in pairs {
            let (sa, sb) = (&series[a], &series[b]);
            let found = analysis::curve_crossings(&sa.x, &sa.y, &sb.x, &sb.y);
            cross_report(
                reports,
                &format!("{} × {}", plain(&sa.label), plain(&sb.label)),
                &xname,
                &found,
                true,
            );
            for (x, y) in found.into_iter().take(MAX_CROSSINGS) {
                marks.push(Mark::Point {
                    x,
                    y,
                    label: cross_label.text(x, y),
                    color: fg,
                    right: sa.right,
                });
            }
        }
    }
    Ok(marks)
}

/// One line for the Terminal: where `what` happens (`yield × growth`, `crosses 50`).
pub(super) fn cross_report(
    reports: &mut Vec<String>,
    what: &str,
    xname: &str,
    found: &[(f64, f64)],
    with_y: bool,
) {
    use crate::data::short;
    let at: Vec<String> = found
        .iter()
        .take(MAX_CROSSINGS)
        .map(|(x, y)| {
            if with_y {
                format!("{xname} = {} (y = {})", short(*x), short(*y))
            } else {
                format!("{xname} = {}", short(*x))
            }
        })
        .collect();
    reports.push(match found.len() {
        0 => format!("{what}: no crossing"),
        n if n > MAX_CROSSINGS => format!(
            "{what}: {n} crossings (the first {MAX_CROSSINGS} marked; noisy? try smooth=): {}",
            at.join(", ")
        ),
        _ => format!("{what} at {}", at.join(", ")),
    });
}

/// The part of x range `x` where `line` stays between `y.0` and `y.1`.
pub(super) fn within(line: crate::analysis::Line, x: (f64, f64), y: (f64, f64)) -> (f64, f64) {
    if line.slope == 0.0 || !line.slope.is_finite() {
        return x;
    }
    let (e0, e1) = ((y.0 - line.at0) / line.slope, (y.1 - line.at0) / line.slope);
    let (a, b) = (x.0.max(e0.min(e1)), x.1.min(e0.max(e1)));
    if a < b { (a, b) } else { x }
}

/// What `fits` found: the curves (each to go right after its data series), the numbers
/// for the Terminal, the residuals strip's points, and each fit with its data series and
/// that series' residuals point by point (NaN where a point has none).
pub(super) struct Fits {
    pub(super) curves: Vec<(usize, Series)>,
    pub(super) reports: Vec<String>,
    pub(super) strip: Vec<Series>,
    pub(super) fitted: Vec<(usize, fit::Fitted, Vec<f64>)>,
}

/// Fitted curves for layers with `fit=`.
pub(super) fn fits(spec: &PanelSpec, series: &[Series], info: &[Pending]) -> Result<Fits, Error> {
    // Curves named after their series when several are drawn (residual plots don't
    // draw their fit, so they don't count).
    let data = series
        .iter()
        .filter(|s| !s.kind.is_residual())
        .count()
        .max(1);
    let mut out = Vec::new();
    let mut reports = Vec::new();
    let mut residuals = Vec::new();
    let mut fitted = Vec::new();
    for (i, s) in series.iter().enumerate() {
        let layer = &spec.layers[info[i].layer];
        let opts = &layer.fitting;
        let model = match (&layer.fit, &opts.formula) {
            (Some(kind), _) => fit::Model::Builtin(*kind),
            (None, Some(f)) => fit::Model::formula(&f.value)
                .map_err(|m| Error::at(format!("fit: {m}"), f.span.clone()))?,
            (None, None) => {
                if opts.band || opts.through_zero || opts.weighted || opts.range.is_some() {
                    return Err(Error::at(
                        "fitband, through0, weighted and fitrange= go with a fit: add fit=linear \
                         (or another)",
                        layer.file.span.clone(),
                    ));
                }
                continue;
            }
        };
        if !matches!(
            s.kind,
            PlotKind::Line | PlotKind::Scatter | PlotKind::Step | PlotKind::Resid | PlotKind::Qq
        ) {
            return Err(Error::at(
                "fit= works with line, scatter, step, resid and qq plots",
                layer.file.span.clone(),
            ));
        }
        if opts.weighted && s.yerr.is_none() {
            return Err(Error::at(
                "weighted counts each point by 1/error²: add err=COLUMN",
                layer.file.span.clone(),
            ));
        }
        let fit_opts = fit::FitOptions {
            through_zero: opts.through_zero,
            sigma: if opts.weighted {
                s.yerr.as_deref()
            } else {
                None
            },
            start: &opts.start,
            range: opts.range,
            log_y: spec.logy,
        };
        let span = opts
            .formula
            .as_ref()
            .map_or(layer.file.span.clone(), |f| f.span.clone());
        let f = fit::fit(model, &s.x, &s.y, &fit_opts)
            .map_err(|m| Error::at(format!("fit: {m}"), span))?;
        reports.push(if data > 1 {
            format!(
                "{}: {}",
                crate::render::mathtext::plain(&s.label),
                f.report()
            )
        } else {
            f.report()
        });
        let in_range = |x: f64| {
            opts.range
                .is_none_or(|(a, b)| x >= a.min(b) && x <= a.max(b))
        };
        let (rx, ry): (Vec<f64>, Vec<f64>) =
            s.x.iter()
                .zip(&s.y)
                .filter(|(x, y)| x.is_finite() && y.is_finite() && in_range(**x))
                .map(|(x, y)| (*x, y - f.eval(*x)))
                .filter(|(_, r)| r.is_finite())
                .unzip();
        let each: Vec<f64> =
            s.x.iter()
                .zip(&s.y)
                .map(|(x, y)| {
                    if in_range(*x) {
                        y - f.eval(*x)
                    } else {
                        f64::NAN
                    }
                })
                .map(|r| if r.is_finite() { r } else { f64::NAN })
                .collect();
        let mut r = Series::new(PlotKind::Scatter, s.label.clone(), rx, ry, s.color);
        r.marker = if s.marker == Marker::None {
            Marker::Circle
        } else {
            s.marker
        };
        r.alpha = s.alpha;
        // resid and qq plots show their residuals themselves, not in a strip.
        if !s.kind.is_residual() {
            residuals.push(r);
        }
        let usable: Vec<f64> =
            s.x.iter()
                .copied()
                .filter(|x| {
                    x.is_finite() && f.defined_at(*x) && (!spec.logx || *x > 0.0) && in_range(*x)
                })
                .collect();
        let lo = usable.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = usable.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let log = spec.logx && lo > 0.0;
        let xs: Vec<f64> = (0..=200)
            .map(|k| {
                let t = k as f64 / 200.0;
                if log {
                    10f64.powf(lo.log10() + (hi.log10() - lo.log10()) * t)
                } else {
                    lo + (hi - lo) * t
                }
            })
            .collect();
        let ys = xs.iter().map(|x| f.eval(*x)).collect();
        let band: Option<Vec<f64>> = opts.band.then(|| xs.iter().map(|x| f.band(*x)).collect());
        // The equation's names: the columns, or with fitnames=labels the axis labels
        // without their units (owner, 2026-11-15; label= names the legend entry only).
        let column_x = layer.x.as_ref().map_or("x", |x| x.value.as_str());
        let labels = spec.fit_names == FitNames::Labels;
        let (yname, xname) = if labels {
            (
                spec.ylabel
                    .as_deref()
                    .map_or(info[i].yname.as_str(), no_units),
                spec.xlabel.as_deref().map_or(column_x, no_units),
            )
        } else {
            (info[i].yname.as_str(), column_x)
        };
        let eq = f.label_named(yname, xname, spec.fit_digits.unwrap_or(3), labels);
        let label = if data > 1 {
            format!("{}: {eq}", s.label)
        } else {
            eq
        };
        let mut curve = Series::new(PlotKind::Line, label, xs, ys, s.color);
        curve.style = LineStyle::Dashed;
        curve.width = s.width * 0.9;
        curve.right = s.right;
        curve.is_fit = true;
        curve.fitted = Some(Box::new(f.clone()));
        if let Some(b) = band {
            curve.yerr = Some(b);
            curve.band = true;
        }
        out.push((i, curve));
        fitted.push((i, f, each));
    }
    Ok(Fits {
        curves: out,
        reports,
        strip: residuals,
        fitted,
    })
}

/// The residuals strip for a panel: its fits' residuals as points around a zero line, on
/// the same x axis, without the panel's decorations.
pub(super) fn residual_panel(main: &Panel) -> Panel {
    let mut p = main.clone();
    p.hints.clear();
    p.series = std::mem::take(&mut p.residuals);
    p.title = None;
    p.ylabel = Some("residual".into());
    p.ylim = (None, None);
    p.ystep = None;
    p.hlines = vec![0.0];
    p.vlines.clear();
    p.spans.clear();
    p.yspans.clear();
    p.hlines2.clear();
    p.yspans2.clear();
    p.shapes.clear();
    p.marks.clear();
    p.ref_labels.clear();
    p.notes.clear();
    p.x2 = None;
    p.y2conv = None;
    p.inset = None;
    p.legend = LegendPos::Off;
    p.fit_reports.clear();
    p.logy = false;
    p.y2label = None;
    p.stacked = false;
    p.residual_of_prev = true;
    p
}

/// Pearson's r of the points where both x and y are numbers, or Spearman's r_s (Pearson's
/// r of their ranks, ties sharing the mean rank). Also the number of points. None with
/// fewer than 3 points or no spread.
pub fn correlation(xs: &[f64], ys: &[f64], kind: Corr) -> Option<(f64, usize)> {
    let (mut a, mut b): (Vec<f64>, Vec<f64>) = xs
        .iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y))
        .unzip();
    let n = a.len();
    if n < 3 {
        return None;
    }
    if kind == Corr::Spearman {
        a = ranks(&a);
        b = ranks(&b);
    }
    let (ma, mb) = (
        a.iter().sum::<f64>() / n as f64,
        b.iter().sum::<f64>() / n as f64,
    );
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(&b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma).powi(2);
        sbb += (y - mb).powi(2);
    }
    let r = sab / (saa * sbb).sqrt();
    r.is_finite().then_some((r.clamp(-1.0, 1.0), n))
}

/// Ranks from 1, ties sharing the mean of their ranks.
pub(super) fn ranks(v: &[f64]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..v.len()).collect();
    idx.sort_by(|&i, &j| v[i].total_cmp(&v[j]));
    let mut out = vec![0.0; v.len()];
    let mut k = 0;
    while k < idx.len() {
        let mut end = k;
        while end + 1 < idx.len() && v[idx[end + 1]] == v[idx[k]] {
            end += 1;
        }
        let mean = (k + end) as f64 / 2.0 + 1.0;
        for &i in &idx[k..=end] {
            out[i] = mean;
        }
        k = end + 1;
    }
    out
}

/// An axis label without its units: `yield (%)` → `yield`, `Energy [J]` → `Energy`.
fn no_units(label: &str) -> &str {
    let t = label.trim();
    match t.rfind(['(', '[']) {
        Some(i) if i > 0 && t.ends_with([')', ']']) => t[..i].trim_end(),
        _ => t,
    }
}
