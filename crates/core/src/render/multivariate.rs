//! Drawing several variables together: lines, markers, areas, bars, bands, error bars,
//! heatmaps, contours, the numbers written on them, and polar plots.

use super::*;

pub(super) fn draw_line(out: &mut String, s: &Series, sx: &Scale, sy: &Scale) {
    let points = decimate(&s.x, &s.y, sx, sy);
    let mut d = String::with_capacity(points.len() * 16);
    let mut pen_down = false;
    for p in points {
        let Some((px, py)) = p else {
            pen_down = false;
            continue;
        };
        let _ = write!(d, "{}{} {}", if pen_down { 'L' } else { 'M' }, n(px), n(py));
        pen_down = true;
    }
    if d.is_empty() {
        return;
    }
    let w = s.width;
    let (dash, cap) = match s.style {
        LineStyle::Solid => (String::new(), "round"),
        LineStyle::Dashed => (
            format!(" stroke-dasharray=\"{} {}\"", n(3.7 * w), n(1.6 * w)),
            "butt",
        ),
        LineStyle::Dotted => (
            format!(" stroke-dasharray=\"0.01 {}\"", n(2.0 * w)),
            "round",
        ),
        LineStyle::DashDot => (
            format!(
                " stroke-dasharray=\"{} {} {} {}\"",
                n(6.4 * w),
                n(1.6 * w),
                n(1.0 * w),
                n(1.6 * w)
            ),
            "butt",
        ),
    };
    let _ = write!(
        out,
        "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"{cap}\"{dash}{}/>",
        s.color.hex(),
        n(w),
        opacity("stroke", s.alpha)
    );
}

/// Page positions of a line's points (`None` = gap). When there are far more points than
/// the page can show, each 0.25 pt slice keeps only its first, lowest, highest and last
/// point, which looks identical but draws much faster. Only for x values in order.
pub(super) fn decimate(xs: &[f64], ys: &[f64], sx: &Scale, sy: &Scale) -> Vec<Option<(f64, f64)>> {
    let width = (sx.end - sx.start).abs();
    let slice = 0.25;
    let sorted = xs
        .iter()
        .filter(|x| x.is_finite())
        .zip(xs.iter().filter(|x| x.is_finite()).skip(1))
        .all(|(a, b)| a <= b);
    let mapped = xs.iter().zip(ys).map(|(x, y)| {
        let (px, py) = (sx.map(*x), sy.map(*y));
        (px.is_finite() && py.is_finite()).then_some((px, py))
    });
    if xs.len() < (width / slice) as usize * 4 || !sorted {
        return mapped.collect();
    }
    let mut out = Vec::new();
    let mut bucket: Vec<(f64, f64)> = Vec::new();
    let mut current = f64::NAN;
    let flush = |bucket: &mut Vec<(f64, f64)>, out: &mut Vec<Option<(f64, f64)>>| {
        if bucket.len() <= 4 {
            out.extend(bucket.drain(..).map(Some));
            return;
        }
        let first = bucket[0];
        let last = bucket[bucket.len() - 1];
        let (mut lo, mut hi) = (0, 0);
        for (i, p) in bucket.iter().enumerate() {
            if p.1 < bucket[lo].1 {
                lo = i;
            }
            if p.1 > bucket[hi].1 {
                hi = i;
            }
        }
        let (a, b) = if lo < hi { (lo, hi) } else { (hi, lo) };
        out.extend([first, bucket[a], bucket[b], last].map(Some));
        bucket.clear();
    };
    for p in mapped {
        match p {
            None => {
                flush(&mut bucket, &mut out);
                out.push(None);
                current = f64::NAN;
            }
            Some((px, py)) => {
                let key = (px / slice).floor();
                if key != current {
                    flush(&mut bucket, &mut out);
                    current = key;
                }
                bucket.push((px, py));
            }
        }
    }
    flush(&mut bucket, &mut out);
    out
}

pub(super) fn draw_markers(out: &mut String, s: &Series, sx: &Scale, sy: &Scale, size: f64) {
    let mut d = String::with_capacity(s.x.len().min(20_000) * 40);
    // Markers closer than 0.2 pt to one already drawn would be hidden under it anyway.
    let mut drawn = std::collections::HashSet::new();
    for (x, y) in s.x.iter().zip(&s.y) {
        let (px, py) = (sx.map(*x), sy.map(*y));
        if px.is_finite()
            && py.is_finite()
            && drawn.insert(((px * 5.0).round() as i64, (py * 5.0).round() as i64))
        {
            marker_path(&mut d, s.marker, px, py, size);
        }
    }
    if d.is_empty() {
        return;
    }
    write_marker_path(out, &d, s.marker, s.color, s.alpha, size);
}

/// Markers coloured by a value each (`c=`): the panel's colour map over its colour range.
/// Points are drawn in 64 shades, low values first; a point without a value is grey.
pub(super) fn draw_colored_markers(
    out: &mut String,
    panel: &Panel,
    s: &Series,
    values: &[f64],
    sx: &Scale,
    sy: &Scale,
    size: f64,
) {
    const SHADES: usize = 64;
    let mut paths = vec![String::new(); SHADES + 1];
    for ((x, y), v) in s.x.iter().zip(&s.y).zip(values) {
        let (px, py) = (sx.map(*x), sy.map(*y));
        if !px.is_finite() || !py.is_finite() {
            continue;
        }
        let shade = if v.is_finite() {
            let t = zfrac(panel, *v).clamp(0.0, 1.0);
            ((t * SHADES as f64) as usize).min(SHADES - 1)
        } else {
            SHADES
        };
        marker_path(&mut paths[shade], s.marker, px, py, size);
    }
    // Missing values first, so that coloured points lie on top of them.
    let grey = Color(160, 160, 160);
    if !paths[SHADES].is_empty() {
        write_marker_path(out, &paths[SHADES], s.marker, grey, s.alpha, size);
    }
    for (k, d) in paths[..SHADES].iter().enumerate() {
        if !d.is_empty() {
            let c = panel.cmap.at((k as f64 + 0.5) / SHADES as f64);
            write_marker_path(out, d, s.marker, c, s.alpha, size);
        }
    }
}

pub(super) fn write_marker_path(
    out: &mut String,
    d: &str,
    marker: Marker,
    color: Color,
    alpha: f64,
    size: f64,
) {
    if marker.is_outline() {
        // Open shapes get a slightly thinner outline than the cross and plus.
        let w = if matches!(marker, Marker::Cross | Marker::Plus) {
            0.25
        } else {
            0.18
        };
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"miter\"{}/>",
            color.hex(),
            n(size * w),
            opacity("stroke", alpha)
        );
    } else {
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"{}\" stroke=\"none\"{}/>",
            color.hex(),
            opacity("fill", alpha)
        );
    }
}

/// Append one marker centred on (x, y); `size` is roughly its diameter in points.
pub(super) fn marker_path(d: &mut String, marker: Marker, x: f64, y: f64, size: f64) {
    if marker.solid() != marker {
        // An open shape: the filled one's outline, a little smaller so that with its
        // stroke it covers the same area.
        marker_path(d, marker.solid(), x, y, size * 0.85);
        return;
    }
    let r = size / 2.0;
    let _ = match marker {
        Marker::Circle => write!(
            d,
            "M{} {}a{r2} {r2} 0 1 0 {} 0a{r2} {r2} 0 1 0 {} 0Z",
            n(x - r),
            n(y),
            n(2.0 * r),
            n(-2.0 * r),
            r2 = n(r)
        ),
        Marker::Square => {
            let a = r * 0.88;
            write!(
                d,
                "M{} {}h{s}v{s}h-{s}Z",
                n(x - a),
                n(y - a),
                s = n(2.0 * a)
            )
        }
        Marker::Triangle => write!(
            d,
            "M{} {}L{} {}L{} {}Z",
            n(x),
            n(y - r * 1.15),
            n(x + r),
            n(y + r * 0.75),
            n(x - r),
            n(y + r * 0.75)
        ),
        Marker::TriangleDown => write!(
            d,
            "M{} {}L{} {}L{} {}Z",
            n(x),
            n(y + r * 1.15),
            n(x + r),
            n(y - r * 0.75),
            n(x - r),
            n(y - r * 0.75)
        ),
        Marker::Diamond => write!(
            d,
            "M{} {}L{} {}L{} {}L{} {}Z",
            n(x),
            n(y - r * 1.2),
            n(x + r * 0.85),
            n(y),
            n(x),
            n(y + r * 1.2),
            n(x - r * 0.85),
            n(y)
        ),
        Marker::Cross => write!(
            d,
            "M{} {}l{s} {s}M{} {}l{s} -{s}",
            n(x - r),
            n(y - r),
            n(x - r),
            n(y + r),
            s = n(2.0 * r)
        ),
        Marker::Plus => write!(
            d,
            "M{} {}h{s}M{} {}v{s}",
            n(x - r),
            n(y),
            n(x),
            n(y - r),
            s = n(2.0 * r)
        ),
        Marker::None
        | Marker::OpenCircle
        | Marker::OpenSquare
        | Marker::OpenTriangle
        | Marker::OpenTriangleDown
        | Marker::OpenDiamond => Ok(()),
    };
}

/// A step plot's corner points, in data units, ready for `draw_line`.
pub(super) fn stepped(s: &Series) -> Series {
    let mut x = Vec::with_capacity(s.x.len() * 2);
    let mut y = Vec::with_capacity(s.y.len() * 2);
    let pts: Vec<(f64, f64)> = s.x.iter().copied().zip(s.y.iter().copied()).collect();
    for (i, &(px, py)) in pts.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| pts[j]);
        match prev {
            Some((qx, qy))
                if qx.is_finite() && qy.is_finite() && px.is_finite() && py.is_finite() =>
            {
                match s.step {
                    StepWhere::Post => {
                        x.extend([px, px]);
                        y.extend([qy, py]);
                    }
                    StepWhere::Pre => {
                        x.extend([qx, px]);
                        y.extend([py, py]);
                    }
                    StepWhere::Mid => {
                        let m = (qx + px) / 2.0;
                        x.extend([m, m, px]);
                        y.extend([qy, py, py]);
                    }
                }
            }
            _ => {
                x.push(px);
                y.push(py);
            }
        }
    }
    Series {
        x,
        y,
        marker: Marker::None,
        ..s.clone()
    }
}

/// An area: filled between the line and zero (or its `base`), with the line on top.
pub(super) fn draw_area(out: &mut String, s: &Series, sx: &Scale, sy: &Scale, stacked: bool) {
    let mut d = String::new();
    let mut run: Vec<(f64, f64, f64)> = Vec::new();
    let flush = |run: &mut Vec<(f64, f64, f64)>, d: &mut String| {
        if run.len() >= 2 {
            for (i, (x, top, _)) in run.iter().enumerate() {
                let _ = write!(d, "{}{} {}", if i == 0 { 'M' } else { 'L' }, n(*x), n(*top));
            }
            for (x, _, bottom) in run.iter().rev() {
                let _ = write!(d, "L{} {}", n(*x), n(*bottom));
            }
            d.push('Z');
        }
        run.clear();
    };
    for (i, (x, y)) in s.x.iter().zip(&s.y).enumerate() {
        let base = s.base.as_ref().map_or(0.0, |b| b[i]);
        let (px, py, pb) = (sx.map(*x), sy.map(*y), sy.map_or_floor(base));
        if px.is_finite() && py.is_finite() && pb.is_finite() {
            run.push((px, py, pb));
        } else {
            flush(&mut run, &mut d);
        }
    }
    flush(&mut run, &mut d);
    if d.is_empty() {
        return;
    }
    let fill = if stacked { 0.85 } else { 0.3 };
    let _ = write!(
        out,
        "\n<path d=\"{d}\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"none\"/>",
        s.color.hex(),
        n(fill_alpha(s, fill))
    );
    let mut edge = s.clone();
    if stacked {
        edge.width = s.width * 0.6;
    }
    draw_line(out, &edge, sx, sy);
    if s.base.as_ref().is_some_and(|b| b.iter().any(|v| *v != 0.0)) && !stacked {
        let lower = Series {
            y: s.base.clone().unwrap_or_default(),
            ..edge
        };
        draw_line(out, &lower, sx, sy);
    }
}

/// Bars: side by side when several bar series share the x positions, or on top of each
/// other when stacked (their `base` says where each one starts).
pub(super) fn draw_bars(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
) {
    let bars: Vec<&Series> = panel
        .series
        .iter()
        .filter(|s| s.kind == PlotKind::Bar)
        .collect();
    if bars.is_empty() {
        return;
    }
    let slot = bar_slot(panel) * 0.8;
    let each = if panel.stacked {
        slot
    } else {
        slot / bars.len() as f64
    };
    for (k, s) in bars.iter().enumerate() {
        let sy = match (sy2, s.right) {
            (Some(r), true) => r,
            _ => sy,
        };
        let shift = if panel.stacked {
            -slot / 2.0
        } else {
            -slot / 2.0 + each * k as f64
        };
        // One path per colour (`highlight=` colours bars one by one).
        let mut paths: Vec<(Color, String)> = Vec::new();
        for (i, (x, y)) in s.x.iter().zip(&s.y).enumerate() {
            if !(x.is_finite() && y.is_finite()) {
                continue;
            }
            let c = s.color_at(i);
            let d = match paths.iter().position(|(pc, _)| *pc == c) {
                Some(k) => &mut paths[k].1,
                None => {
                    paths.push((c, String::new()));
                    &mut paths.last_mut().expect("just pushed").1
                }
            };
            let x0 = sx.map(x + shift);
            let x1 = sx.map(x + shift + each);
            let y0 = s
                .base
                .as_ref()
                .map_or_else(|| sy.baseline(), |b| sy.map_or_floor(b[i]));
            let y1 = sy.map(*y);
            if !(x0.is_finite() && x1.is_finite() && y1.is_finite() && y0.is_finite()) {
                continue;
            }
            let _ = write!(d, "M{} {}H{}V{}H{}Z", n(x0), n(y0), n(x1), n(y1), n(x0));
        }
        if paths.is_empty() {
            paths.push((s.color, String::new()));
        }
        for (c, d) in paths {
            let _ = write!(
                out,
                "\n<path d=\"{d}\" fill=\"{}\" stroke=\"none\"{}/>",
                c.hex(),
                opacity("fill", s.alpha)
            );
        }
    }
}

/// `band`: a see-through fill from y − error to y + error, behind the line. A gap in the
/// data starts a new piece of band.
pub(super) fn draw_band(out: &mut String, s: &Series, sx: &Scale, sy: &Scale) {
    if s.yerr.is_none() {
        return;
    }
    let mut d = String::new();
    let mut piece: Vec<(f64, f64, f64)> = Vec::new();
    let mut flush = |piece: &mut Vec<(f64, f64, f64)>| {
        if piece.len() >= 2 {
            for (i, (px, _, hi)) in piece.iter().enumerate() {
                let _ = write!(d, "{}{} {}", if i == 0 { 'M' } else { 'L' }, n(*px), n(*hi));
            }
            for (px, lo, _) in piece.iter().rev() {
                let _ = write!(d, "L{} {}", n(*px), n(*lo));
            }
            d.push('Z');
        }
        piece.clear();
    };
    for (i, (x, y)) in s.x.iter().zip(&s.y).enumerate() {
        let len = |e: f64| if e.is_finite() { e.abs() } else { 0.0 };
        let (down, up) = s.yerr_at(i).map_or((0.0, 0.0), |(d, u)| (len(d), len(u)));
        let (px, lo, hi) = (sx.map(*x), sy.map_or_floor(y - down), sy.map(y + up));
        if px.is_finite() && lo.is_finite() && hi.is_finite() {
            piece.push((px, lo, hi));
        } else {
            flush(&mut piece);
        }
    }
    flush(&mut piece);
    if !d.is_empty() {
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"none\"/>",
            s.color.hex(),
            n(fill_alpha(s, 0.22))
        );
    }
}

/// Error bars with small caps, behind the markers.
pub(super) fn draw_error_bars(
    out: &mut String,
    panel: &Panel,
    s: &Series,
    sx: &Scale,
    sy: &Scale,
    style: &Style,
) {
    let cap = style.marker_size * 0.45;
    // On bars side by side: at the middle of this series' bar, not of the group.
    let dx = bar_offset(panel, s);
    let mut d = String::new();
    for (i, (x, y)) in s.x.iter().zip(&s.y).enumerate() {
        let (px, py) = (sx.map(*x + dx), sy.map(*y));
        if !(px.is_finite() && py.is_finite()) {
            continue;
        }
        if let Some((down, up)) = s
            .yerr_at(i)
            .filter(|(d, u)| d.is_finite() && u.is_finite() && (*d > 0.0 || *u > 0.0))
        {
            let (lo, hi) = (sy.map_or_floor(y - down), sy.map(y + up));
            if lo.is_finite() && hi.is_finite() {
                let _ = write!(
                    d,
                    "M{} {}V{}M{} {}h{}M{} {}h{}",
                    n(px),
                    n(lo),
                    n(hi),
                    n(px - cap),
                    n(lo),
                    n(2.0 * cap),
                    n(px - cap),
                    n(hi),
                    n(2.0 * cap)
                );
            }
        }
        if let Some((down, up)) = s
            .xerr_at(i)
            .filter(|(d, u)| d.is_finite() && u.is_finite() && (*d > 0.0 || *u > 0.0))
        {
            let (lo, hi) = (sx.map_or_floor(x - down), sx.map(x + up));
            if lo.is_finite() && hi.is_finite() {
                let _ = write!(
                    d,
                    "M{} {}H{}M{} {}v{}M{} {}v{}",
                    n(lo),
                    n(py),
                    n(hi),
                    n(lo),
                    n(py - cap),
                    n(2.0 * cap),
                    n(hi),
                    n(py - cap),
                    n(2.0 * cap)
                );
            }
        }
    }
    if d.is_empty() {
        return;
    }
    // On a bar the error bar is in the text colour, so it shows against the fill.
    let colour = if s.kind == PlotKind::Bar {
        style.foreground
    } else {
        s.color
    };
    let _ = write!(
        out,
        "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{}/>",
        colour.hex(),
        n((s.width * 0.7).max(0.7)),
        opacity("stroke", s.alpha)
    );
}

/// Heatmap cells, one path per colour (256 shades), plus the values if asked.
pub(super) fn draw_heatmap(
    out: &mut String,
    g: &Grid,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    style: &Style,
) {
    let mut by_shade: std::collections::BTreeMap<u8, String> = Default::default();
    for (j, y) in g.ys.iter().enumerate() {
        for (i, x) in g.xs.iter().enumerate() {
            let v = g.at(i, j);
            if !v.is_finite() {
                continue;
            }
            let shade = (zfrac(panel, v).clamp(0.0, 1.0) * 255.0).round() as u8;
            let (x0, x1) = (sx.map(x - 0.5), sx.map(x + 0.5));
            let (y0, y1) = (sy.map(y - 0.5), sy.map(y + 0.5));
            // A hair of overlap hides seams between cells.
            let _ = write!(
                by_shade.entry(shade).or_default(),
                "M{} {}H{}V{}H{}Z",
                n(x0 - 0.35),
                n(y0 + 0.35),
                n(x1 + 0.35),
                n(y1 - 0.35),
                n(x0 - 0.35)
            );
        }
    }
    for (shade, d) in &by_shade {
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"{}\" shape-rendering=\"crispEdges\"/>",
            panel.cmap.at(f64::from(*shade) / 255.0).hex()
        );
    }
    if panel.values {
        let size = (style.tick_font_size * 0.9).min((sx.map(1.0) - sx.map(0.0)).abs() * 0.3);
        let cap = text::cap_height(style.font, size);
        for (j, y) in g.ys.iter().enumerate() {
            for (i, x) in g.xs.iter().enumerate() {
                let v = g.at(i, j);
                if !v.is_finite() {
                    continue;
                }
                let c = text_on(panel.cmap.at(zfrac(panel, v)));
                let _ = write!(
                    out,
                    "\n<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" fill=\"{}\">{}</text>",
                    n(sx.map(*x)),
                    n(sy.map(*y) + cap / 2.0),
                    n(size),
                    c.hex(),
                    escape(&mathtext::plain(&format_value(
                        v,
                        panel.values_format.as_ref()
                    )))
                );
            }
        }
    }
}

/// `values` on bars and points: each value above its bar (below a negative one, in the
/// middle of a stacked segment) or above its point and error bar, with a halo so it reads
/// over lines. Heatmap cells write their own.
pub(super) fn draw_values(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    plot: Rect,
    style: &Style,
) {
    let font = style.font;
    let tfs = style.tick_font_size * 0.9;
    let cap = text::cap_height(font, tfs);
    let gap = 3.0;
    let fmt = panel.values_format.as_ref();
    for s in &panel.series {
        if s.grid.is_some()
            || s.is_fit
            || !matches!(
                s.kind,
                PlotKind::Bar | PlotKind::Line | PlotKind::Scatter | PlotKind::Step
            )
        {
            continue;
        }
        let sy = match (sy2, s.right) {
            (Some(r), true) => r,
            _ => sy,
        };
        let dx = bar_offset(panel, s);
        let mut labels = String::new();
        for (i, (&x, &y)) in s.x.iter().zip(&s.y).enumerate() {
            let base = s.base.as_ref().map_or(0.0, |b| b[i]);
            let px = sx.map(x + dx);
            let py = sy.map(y);
            if !(px.is_finite() && py.is_finite()) || px < plot.x - 1.0 || px > plot.right() + 1.0 {
                continue;
            }
            // (down, up)
            let err = s
                .yerr_at(i)
                .filter(|(d, u)| d.is_finite() && u.is_finite() && !s.band);
            let value = |v: f64| format_value_err(v, err, fmt);
            let (text, ty) = if s.kind == PlotKind::Bar {
                let v = y - base;
                if panel.stacked && s.base.is_some() {
                    let mid = sy.map(base + v / 2.0);
                    (format_value(v, fmt), mid + cap / 2.0)
                } else {
                    let end = |(d, u): (f64, f64)| if v >= 0.0 { y + u } else { y - d };
                    let top = err.map_or(py, |e| sy.map(end(e)));
                    if v >= 0.0 {
                        (value(v), top - gap)
                    } else {
                        (value(v), top + gap + cap)
                    }
                }
            } else {
                let above =
                    err.map_or(py - style.marker_size * 0.5, |(_, u)| sy.map(y + u).min(py));
                let ty = above - gap;
                // No room above the plot: under the point instead.
                let ty = if ty - cap < plot.y {
                    err.map_or(py + style.marker_size * 0.5, |(d, _)| sy.map(y - d).max(py))
                        + gap
                        + cap
                } else {
                    ty
                };
                (value(y), ty)
            };
            let _ = write!(
                labels,
                "\n<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\">{}</text>",
                n(px),
                n(ty),
                n(tfs),
                escape(&mathtext::plain(&text))
            );
        }
        if !labels.is_empty() {
            let _ = write!(
                out,
                "\n<g fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" paint-order=\"stroke\">{labels}\n</g>",
                style.foreground.hex(),
                style.background.hex(),
                n(tfs * 0.22)
            );
        }
    }
}

/// A value written on the plot: three significant figures, or the decimals of
/// `values=%.1f` with the text around it; a value that would round to 0 shows as `<1`
/// (`<0.1`…, or `>−1` when negative), as in tables.
pub(super) fn format_value(v: f64, fmt: Option<&NumberFormat>) -> String {
    let Some(f) = fmt else {
        return short_number(v);
    };
    format!("{}{}{}", f.before, value_body(v, f), f.after)
}

/// `values=%.1f+-`: the value with its error bar, `41 ± 3`, or `41 (36–47)` when the bar
/// is longer one way (quartiles round a median). `err` is (down, up).
pub(super) fn format_value_err(
    v: f64,
    err: Option<(f64, f64)>,
    fmt: Option<&NumberFormat>,
) -> String {
    let (Some(f), Some((down, up))) = (fmt.filter(|f| f.error), err) else {
        return format_value(v, fmt);
    };
    let body = value_body(v, f);
    let same = (down - up).abs() <= 1e-9 * down.abs().max(up.abs());
    let with = if same {
        format!("{body} ± {}", value_body(up, f))
    } else {
        format!(
            "{body} ({}–{})",
            value_body(v - down, f),
            value_body(v + up, f)
        )
    };
    format!("{}{with}{}", f.before, f.after)
}

/// The number itself in a `values=` format, without the text around it.
pub(super) fn value_body(v: f64, f: &NumberFormat) -> String {
    match f.decimals {
        None => short_number(v),
        Some(d) => {
            let unit = 10f64.powi(-(d as i32));
            // Decided on the written digits: 0.5 with no decimals rounds to an even "0".
            let written = format!("{v:.d$}");
            let zero = written
                .trim_start_matches('-')
                .chars()
                .all(|c| c == '0' || c == '.');
            if v != 0.0 && zero {
                let u = format!("{unit:.d$}");
                if v > 0.0 {
                    format!("<{u}")
                } else {
                    format!(">\u{2212}{u}")
                }
            } else {
                written.replace('-', "\u{2212}")
            }
        }
    }
}

/// A number for a heatmap cell: at most three significant figures.
pub(super) fn short_number(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let mag = v.abs().log10().floor() as i32;
    let decimals = (2 - mag).clamp(0, 3) as usize;
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    };
    s.replace('-', "\u{2212}")
}

/// The colour of the filled band holding `v`.
pub(super) fn band_color(panel: &Panel, v: f64) -> Color {
    let (lo, hi) = panel.zrange;
    let mut edges = vec![lo];
    edges.extend(panel.levels.iter().copied());
    edges.push(hi);
    let k = edges
        .windows(2)
        .position(|w| v <= w[1])
        .unwrap_or(edges.len() - 2);
    panel.cmap.at(zfrac(panel, (edges[k] + edges[k + 1]) / 2.0))
}

/// The part of a polygon where the value is above (`keep_above`) or below `t`.
pub(super) fn clip(poly: &[Corner], t: f64, keep_above: bool) -> Vec<Corner> {
    let inside = |p: &Corner| if keep_above { p.2 >= t } else { p.2 <= t };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let a = poly[k];
        let b = poly[(k + 1) % poly.len()];
        let (ia, ib) = (inside(&a), inside(&b));
        if ia {
            out.push(a);
        }
        if ia != ib {
            let f = (t - a.2) / (b.2 - a.2);
            out.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, t));
        }
    }
    out
}

/// Contours: each grid cell is cut into two triangles, over which the value changes
/// linearly. Lines of equal value cross them straight; filled bands are the triangles
/// clipped between two levels.
pub(super) fn draw_contour(
    out: &mut String,
    s: &Series,
    g: &Grid,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    style: &Style,
) {
    let (nx, ny) = (g.xs.len(), g.ys.len());
    let mut triangles: Vec<[Corner; 3]> = Vec::with_capacity(nx * ny * 2);
    for j in 0..ny.saturating_sub(1) {
        for i in 0..nx.saturating_sub(1) {
            let c = |a: usize, b: usize| (sx.map(g.xs[a]), sy.map(g.ys[b]), g.at(a, b));
            let (p00, p10, p11, p01) = (c(i, j), c(i + 1, j), c(i + 1, j + 1), c(i, j + 1));
            for t in [[p00, p10, p11], [p00, p11, p01]] {
                if t.iter()
                    .all(|p| p.0.is_finite() && p.1.is_finite() && p.2.is_finite())
                {
                    triangles.push(t);
                }
            }
        }
    }
    if panel.filled {
        let (lo, hi) = panel.zrange;
        let mut edges = vec![f64::NEG_INFINITY];
        edges.extend(panel.levels.iter().copied());
        edges.push(f64::INFINITY);
        for w in edges.windows(2) {
            let mid = (w[0].max(lo) + w[1].min(hi)) / 2.0;
            let mut d = String::new();
            for t in &triangles {
                let mut poly: Vec<Corner> = t.to_vec();
                if w[0].is_finite() {
                    poly = clip(&poly, w[0], true);
                }
                if w[1].is_finite() && poly.len() >= 3 {
                    poly = clip(&poly, w[1], false);
                }
                if poly.len() >= 3 {
                    for (k, p) in poly.iter().enumerate() {
                        let _ =
                            write!(d, "{}{} {}", if k == 0 { 'M' } else { 'L' }, n(p.0), n(p.1));
                    }
                    d.push('Z');
                }
            }
            if !d.is_empty() {
                let c = panel.cmap.at(zfrac(panel, mid)).hex();
                // A thin stroke of the same colour closes hairline gaps between pieces.
                let _ = write!(
                    out,
                    "\n<path d=\"{d}\" fill=\"{c}\" stroke=\"{c}\" stroke-width=\"0.3\" stroke-linejoin=\"round\"/>"
                );
            }
        }
    }
    for level in &panel.levels {
        let mut d = String::new();
        for t in &triangles {
            let mut pts = Vec::new();
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if (a.2 - level) * (b.2 - level) < 0.0 || (a.2 == *level && b.2 != *level) {
                    let f = (level - a.2) / (b.2 - a.2);
                    pts.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
                }
            }
            if pts.len() >= 2 {
                let _ = write!(
                    d,
                    "M{} {}L{} {}",
                    n(pts[0].0),
                    n(pts[0].1),
                    n(pts[1].0),
                    n(pts[1].1)
                );
            }
        }
        if d.is_empty() {
            continue;
        }
        let (color, width, opacity) = if panel.filled {
            (style.foreground, 0.4, 0.5)
        } else {
            (panel.cmap.at(zfrac(panel, *level)), s.width, s.alpha)
        };
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"{}/>",
            color.hex(),
            n(width),
            if opacity < 1.0 {
                format!(" stroke-opacity=\"{}\"", n(opacity))
            } else {
                String::new()
            }
        );
    }
}

/// A polar plot's area: a square in the middle of the cell, with room around it for the
/// angle labels.
pub(super) fn measure_polar(
    panel: &Panel,
    cell: Rect,
    style: &Style,
    shared: &Shared,
    legend_out: Option<(f64, f64)>,
) -> Layout {
    let tfs = style.tick_font_size;
    let around = tfs * 2.2;
    let title_room = if panel.title.is_some() || shared.letter_room {
        style.title_font_size * 1.5
    } else {
        0.0
    };
    let legend_room = legend_out.map_or(0.0, |(w, _)| w + LEGEND_OUT_GAP);
    let top = cell.y + style.pad + title_room + around;
    let avail_w = cell.w - 2.0 * style.pad - 2.0 * around - legend_room;
    let avail_h = cell.bottom() - style.pad - around - top;
    let side = avail_w.min(avail_h).max(10.0);
    let x = cell.x + style.pad + around + (avail_w - side) / 2.0;
    let y = top + (avail_h - side) / 2.0;
    let none = ticks::linear(0.0, 1.0, 3);
    Layout {
        x: (0.0, 1.0),
        y: (0.0, 1.0),
        y2: None,
        plot: Rect::new(x, y, side, side),
        xt: none,
        legend_out,
        label_gap: TICK_GAP,
        hide_x: false,
        hide_y: false,
    }
}

/// A round plot: angle 0 to the right, counter-clockwise; radius from the centre.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_polar(
    out: &mut String,
    panel: &Panel,
    cell: Rect,
    plot: Rect,
    style: &Style,
    id: usize,
    letter: Option<&str>,
    legend_out: Option<(f64, f64)>,
) {
    let (cx, cy, radius) = (plot.x + plot.w / 2.0, plot.y + plot.h / 2.0, plot.w / 2.0);
    let tfs = style.tick_font_size;
    let font = style.font;
    let fg = style.foreground.hex();
    let rmax_data = panel
        .series
        .iter()
        .flat_map(|s| s.y.iter().copied())
        .filter(|r| r.is_finite())
        .fold(0.0, f64::max);
    let (rlo, rhi) = (
        panel.ylim.0.unwrap_or(0.0),
        panel
            .ylim
            .1
            .unwrap_or(if rmax_data > 0.0 { rmax_data } else { 1.0 }),
    );
    // The outer ring is the first tick at or above the largest radius.
    let mut rt = ticks::linear(rlo, rhi, 4);
    let step = match rt.major.as_slice() {
        [a, b, ..] => b - a,
        _ => (rhi - rlo).max(1e-9),
    };
    let rmax = if panel.ylim.1.is_some() {
        rhi
    } else {
        let top = rlo + ((rhi - rlo) / step - 1e-9).ceil() * step;
        rt = ticks::linear(rlo, top, 4);
        top
    };
    let to_angle = |a: f64| if panel.radians { a } else { a.to_radians() };
    let at = |a: f64, r: f64| -> (f64, f64) {
        let f = (r - rlo) / (rmax - rlo) * radius;
        (cx + f * a.cos(), cy - f * a.sin())
    };
    // Rings and spokes.
    let mut grid = String::new();
    for r in rt.major.iter().filter(|r| **r > rlo && **r < rmax) {
        let f = (r - rlo) / (rmax - rlo) * radius;
        let _ = write!(
            grid,
            "M{} {}a{f} {f} 0 1 0 {} 0a{f} {f} 0 1 0 {} 0",
            n(cx - f),
            n(cy),
            n(2.0 * f),
            n(-2.0 * f),
            f = n(f)
        );
    }
    for k in 0..8 {
        let a = std::f64::consts::FRAC_PI_4 * k as f64;
        let (x, y) = (cx + radius * a.cos(), cy - radius * a.sin());
        let _ = write!(grid, "M{} {}L{} {}", n(cx), n(cy), n(x), n(y));
    }
    let _ = write!(
        out,
        "\n<path d=\"{grid}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
        style.grid_color.hex(),
        n(style.grid_width.max(0.5))
    );
    let _ = write!(
        out,
        "\n<clipPath id=\"plot{id}\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\"/></clipPath>\n<g clip-path=\"url(#plot{id})\">",
        n(cx),
        n(cy),
        n(radius)
    );
    for s in &panel.series {
        let pts: Vec<Option<(f64, f64)>> =
            s.x.iter()
                .zip(&s.y)
                .map(|(a, r)| {
                    (a.is_finite() && r.is_finite() && *r >= rlo).then(|| at(to_angle(*a), *r))
                })
                .collect();
        let mut d = String::new();
        let mut pen = false;
        for p in &pts {
            match p {
                Some((x, y)) => {
                    let _ = write!(d, "{}{} {}", if pen { 'L' } else { 'M' }, n(*x), n(*y));
                    pen = true;
                }
                None => pen = false,
            }
        }
        if !d.is_empty() {
            let dash = match s.style {
                LineStyle::Solid => String::new(),
                LineStyle::Dashed => format!(
                    " stroke-dasharray=\"{} {}\"",
                    n(3.7 * s.width),
                    n(1.6 * s.width)
                ),
                LineStyle::Dotted => format!(" stroke-dasharray=\"0.01 {}\"", n(2.0 * s.width)),
                LineStyle::DashDot => format!(
                    " stroke-dasharray=\"{} {} {} {}\"",
                    n(6.4 * s.width),
                    n(1.6 * s.width),
                    n(s.width),
                    n(1.6 * s.width)
                ),
            };
            let _ = write!(
                out,
                "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"{dash}{}/>",
                s.color.hex(),
                n(s.width),
                opacity("stroke", s.alpha)
            );
        }
        if s.marker != Marker::None {
            let mut m = String::new();
            for (x, y) in pts.iter().flatten() {
                marker_path(&mut m, s.marker, *x, *y, style.marker_size);
            }
            write_marker_path(out, &m, s.marker, s.color, s.alpha, style.marker_size);
        }
    }
    out.push_str("\n</g>");
    let _ = write!(
        out,
        "\n<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
        n(cx),
        n(cy),
        n(radius),
        n(style.axis_width)
    );
    // Angle labels around the edge, radius labels along the 22.5° line.
    let cap = text::cap_height(font, tfs);
    const PI_LABELS: [&str; 8] = ["0", "π/4", "π/2", "3π/4", "π", "5π/4", "3π/2", "7π/4"];
    for (k, pi_label) in PI_LABELS.iter().enumerate() {
        let a = std::f64::consts::FRAC_PI_4 * k as f64;
        let label = if panel.radians {
            (*pi_label).to_owned()
        } else {
            format!("{}°", k * 45)
        };
        let gap = tfs * 0.9;
        let (x, y) = (cx + (radius + gap) * a.cos(), cy - (radius + gap) * a.sin());
        let anchor = match k {
            0 | 1 | 7 => "start",
            3..=5 => "end",
            _ => "middle",
        };
        let x = match anchor {
            "start" => x - gap * 0.3,
            "end" => x + gap * 0.3,
            _ => x,
        };
        text_at(out, font, x, y + cap / 2.0, tfs, anchor, &label);
    }
    let a = std::f64::consts::PI / 8.0;
    for (r, label) in rt.major.iter().zip(&rt.labels) {
        if *r <= rlo || *r > rmax {
            continue;
        }
        let (x, y) = at(a, *r);
        text_at(out, font, x + 2.0, y - 2.0, tfs, "start", label);
    }
    let title_y = cell.y + style.pad + text::cap_height(font, style.title_font_size);
    if let Some(title) = &panel.title {
        text_at(
            out,
            font,
            cx,
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
    if panel.legend != LegendPos::Off && has_legend(panel) {
        let unit = Scale::linear(0.0, 1.0, plot.x, plot.right());
        let flat = Scale::linear(0.0, 1.0, plot.bottom(), plot.y);
        let scales = Scales {
            x: &unit,
            y: &flat,
            y2: None,
        };
        let mut p = panel.clone();
        if p.legend != LegendPos::Out {
            // The corners of the square are free of data.
            p.legend = LegendPos::TopRight;
        }
        // Outside: beyond the angle labels.
        let corner = if p.legend == LegendPos::Out {
            Rect::new(plot.x, plot.y, plot.w + tfs * 2.2, plot.h)
        } else {
            Rect::new(plot.x, plot.y, plot.w + radius * 0.25, plot.h)
        };
        draw_legend(out, &p, corner, &scales, style, legend_out, None);
    }
}
