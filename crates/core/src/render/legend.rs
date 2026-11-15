//! The legend (size, placement away from the data, drawing) and the colour bar.

use super::*;

/// Width and height of the legend box.
/// The height of a legend entry: one row, and a smaller one for a note (`stats`).
pub(super) fn legend_entry_height(s: &Series, style: &Style) -> f64 {
    style.font_size * LEGEND_ROW + s.note.as_ref().map_or(0.0, |_| style.tick_font_size * 1.25)
}

pub(super) fn legend_size(panel: &Panel, style: &Style) -> (f64, f64) {
    let fs = style.font_size;
    let row = fs * LEGEND_ROW;
    let text_w = legend_series(panel)
        .map(|s| {
            let note = s.note.as_ref().map_or(0.0, |t| {
                mathtext::width(style.font, t, style.tick_font_size)
            });
            mathtext::width(style.font, &s.label, fs).max(note)
        })
        .fold(0.0, f64::max);
    let rows: f64 = legend_series(panel)
        .map(|s| legend_entry_height(s, style))
        .sum();
    (
        LEGEND_INNER * 2.0 + LEGEND_HANDLE + LEGEND_GAP + text_w,
        LEGEND_INNER * 2.0 + rows - (row - fs) * 0.5,
    )
}

pub(super) fn draw_legend(
    out: &mut String,
    panel: &Panel,
    plot: Rect,
    scales: &Scales,
    style: &Style,
    outside: Option<(f64, f64)>,
    avoid: Option<Rect>,
) -> Rect {
    let fs = style.font_size;
    let inner = LEGEND_INNER;
    let margin = LEGEND_MARGIN;
    let (bw, bh) = legend_size(panel, style);
    // Where the box goes, as fractions across and down the plot (0 = left/top).
    let at = |fx: f64, fy: f64| -> Rect {
        let x = plot.x + margin + (plot.w - 2.0 * margin - bw) * fx;
        let y = plot.y + margin + (plot.h - 2.0 * margin - bh) * fy;
        Rect::new(x, y, bw, bh)
    };
    let b = match panel.legend {
        LegendPos::Out => {
            let (w, h) = outside.unwrap_or((bw, bh));
            // Text lines up with the plot's top edge; no box needed outside the data.
            Rect::new(plot.right() + LEGEND_OUT_GAP - inner, plot.y - inner, w, h)
        }
        LegendPos::TopLeft => at(0.0, 0.0),
        LegendPos::BottomLeft => at(0.0, 1.0),
        LegendPos::BottomRight => at(1.0, 1.0),
        LegendPos::Best => {
            let obstacles = obstacles(panel, scales);
            let notes: Vec<Rect> = corner_notes(panel, plot, style, scales)
                .into_iter()
                .map(|n| n.0)
                .collect();
            // Corners first, then the middle of each side, then the centre.
            [
                (1.0, 0.0),
                (0.0, 0.0),
                (0.0, 1.0),
                (1.0, 1.0),
                (1.0, 0.5),
                (0.0, 0.5),
                (0.5, 0.0),
                (0.5, 1.0),
                (0.5, 0.5),
            ]
            .into_iter()
            .map(|(fx, fy)| at(fx, fy))
            .min_by_key(|r| {
                // Never on top of an inset.
                let inset = avoid.is_some_and(|a| a.overlaps(r));
                let note = notes.iter().any(|a| a.overlaps(r));
                obstacles.iter().filter(|o| o.overlaps(r)).count()
                    + usize::from(inset) * 100_000
                    + usize::from(note) * 50_000
            })
            .unwrap_or_else(|| at(1.0, 0.0))
        }
        _ => at(1.0, 0.0),
    };
    if panel.legend != LegendPos::Out {
        let _ = write!(
            out,
            "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" fill-opacity=\"0.85\" stroke=\"{}\" stroke-width=\"0.5\"/>",
            n(b.x),
            n(b.y),
            n(b.w),
            n(b.h),
            style.background.hex(),
            style.legend_frame.hex()
        );
    }
    let mut top = b.y + inner;
    for s in legend_series(panel) {
        let cy = top + fs * 0.5;
        top += legend_entry_height(s, style);
        draw_legend_entry(out, panel, s, b.x + inner, cy, style);
    }
    b
}

/// A legend too wide for its panel (a fit's equation in a narrow panel): first each
/// equation's `(R² = …)` moves to a second, smaller line; if that isn't enough the
/// legend's text shrinks, down to 70 %. None when the legend fits as it is.
pub(super) fn fit_legend(panel: &Panel, width: f64, style: &Style) -> Option<(Panel, Style)> {
    let room = width - 2.0 * LEGEND_MARGIN;
    if panel.legend == LegendPos::Out || legend_size(panel, style).0 <= room {
        return None;
    }
    let mut p = panel.clone();
    for s in &mut p.series {
        if let Some(at) = s.label.rfind(" ($R^2") {
            let r2 = s.label.split_off(at);
            let r2 = r2.trim_start().to_owned();
            s.note = Some(match s.note.take() {
                Some(n) => format!("{r2} · {n}"),
                None => r2,
            });
        }
    }
    let mut st = style.clone();
    let w = legend_size(&p, &st).0;
    if w > room {
        // The sample and the gaps keep their size; only the text shrinks.
        let fixed = LEGEND_INNER * 2.0 + LEGEND_HANDLE + LEGEND_GAP;
        let k = ((room - fixed) / (w - fixed)).clamp(0.7, 1.0);
        st.font_size *= k;
        st.tick_font_size *= k;
    }
    Some((p, st))
}

/// The one legend of a figure (`--legend=`): every panel's entries, each label once.
pub(super) struct FigureLegend<'a> {
    pub side: SharedLegend,
    /// The room it takes from the figure: height for top and bottom, width for right.
    pub room: f64,
    entries: Vec<(&'a Panel, &'a Series, f64)>,
    /// Entries per row (top and bottom) as index ranges.
    rows: Vec<std::ops::Range<usize>>,
}

const SHARED_SPACING: f64 = 18.0;

pub(super) fn figure_legend<'a>(
    fig: &'a Figure,
    style: &Style,
    side: SharedLegend,
    width: f64,
) -> Option<FigureLegend<'a>> {
    let fs = style.font_size;
    let mut entries: Vec<(&Panel, &Series, f64)> = Vec::new();
    for (i, p) in fig.panels.iter().enumerate() {
        if !fig.shared_from.get(i).copied().unwrap_or(true) {
            continue;
        }
        for s in legend_series(p) {
            if s.label.is_empty() || entries.iter().any(|e| e.1.label == s.label) {
                continue;
            }
            let note = s.note.as_ref().map_or(0.0, |t| {
                mathtext::width(style.font, t, style.tick_font_size)
            });
            let w =
                LEGEND_HANDLE + LEGEND_GAP + mathtext::width(style.font, &s.label, fs).max(note);
            entries.push((p, s, w));
        }
    }
    if entries.is_empty() {
        return None;
    }
    let height = |r: &std::ops::Range<usize>| {
        entries[r.clone()]
            .iter()
            .map(|e| legend_entry_height(e.1, style))
            .fold(0.0, f64::max)
    };
    let (rows, room) = if side == SharedLegend::Right {
        let widest = entries.iter().map(|e| e.2).fold(0.0, f64::max);
        (Vec::new(), widest + style.pad + LEGEND_OUT_GAP)
    } else {
        // Fill each row from the left; a new row when the next entry doesn't fit.
        let max = width - 2.0 * style.pad;
        let mut rows = Vec::new();
        let (mut start, mut used) = (0, 0.0);
        for (i, e) in entries.iter().enumerate() {
            let more = if i == start {
                e.2
            } else {
                SHARED_SPACING + e.2
            };
            if i > start && used + more > max {
                rows.push(start..i);
                (start, used) = (i, e.2);
            } else {
                used += more;
            }
        }
        rows.push(start..entries.len());
        let room = rows.iter().map(height).sum::<f64>() + style.pad;
        (rows, room)
    };
    Some(FigureLegend {
        side,
        room,
        entries,
        rows,
    })
}

/// Draw the figure's legend in `band`: the strip of the figure kept free for it.
pub(super) fn draw_figure_legend(
    out: &mut String,
    l: &FigureLegend,
    band: Rect,
    fig_w: f64,
    style: &Style,
) {
    let fs = style.font_size;
    if l.side == SharedLegend::Right {
        let total: f64 = l
            .entries
            .iter()
            .map(|e| legend_entry_height(e.1, style))
            .sum();
        let mut top = band.y + ((band.h - total) / 2.0).max(style.pad);
        for (p, s, _) in &l.entries {
            draw_legend_entry(
                out,
                p,
                s,
                band.x + LEGEND_OUT_GAP * 0.5,
                top + fs * 0.5,
                style,
            );
            top += legend_entry_height(s, style);
        }
        return;
    }
    let mut top = band.y
        + if l.side == SharedLegend::Top {
            style.pad
        } else {
            0.0
        };
    for row in &l.rows {
        let es = &l.entries[row.clone()];
        let w: f64 = es.iter().map(|e| e.2).sum::<f64>() + SHARED_SPACING * (es.len() - 1) as f64;
        // Centred in the band, but never off the figure's edges (a legend wider than the
        // one column it belongs to).
        let mut x = (band.x + (band.w - w) / 2.0)
            .min(fig_w - style.pad - w)
            .max(style.pad);
        for (p, s, ew) in es {
            draw_legend_entry(out, p, s, x, top + fs * 0.5, style);
            x += ew + SHARED_SPACING;
        }
        top += es
            .iter()
            .map(|e| legend_entry_height(e.1, style))
            .fold(0.0, f64::max);
    }
}

/// One legend entry: its sample (line, swatch, marker) starting at `hx`, centred on `cy`,
/// then its label and the note under it.
pub(super) fn draw_legend_entry(
    out: &mut String,
    panel: &Panel,
    s: &Series,
    hx: f64,
    cy: f64,
    style: &Style,
) {
    let fs = style.font_size;
    let font = style.font;
    let handle = LEGEND_HANDLE;
    let gap = LEGEND_GAP;
    let cap = text::cap_height(style.font, fs);
    let swatch = |out: &mut String, fill_opacity: f64, stroke: bool| {
        let _ = write!(
            out,
            "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{}{}/>",
            n(hx + 3.0),
            n(cy - fs * 0.35),
            n(handle - 6.0),
            n(fs * 0.7),
            s.color.hex(),
            opacity("fill", fill_alpha(s, fill_opacity)),
            if stroke {
                format!(" stroke=\"{}\" stroke-width=\"0.8\"", s.color.hex())
            } else {
                String::new()
            }
        );
    };
    let line = |out: &mut String| {
        let line = Series {
            x: vec![0.0, 1.0],
            y: vec![0.0, 0.0],
            ..s.clone()
        };
        let local = Scale::linear(0.0, 1.0, hx, hx + handle);
        let flat = Scale::linear(-1.0, 1.0, cy, cy);
        draw_line(out, &line, &local, &flat);
    };
    match s.kind {
        PlotKind::Bar => swatch(out, 1.0, false),
        PlotKind::Hist => {
            let several = panel
                .series
                .iter()
                .filter(|s| s.kind == PlotKind::Hist)
                .count()
                > 1;
            swatch(
                out,
                if several && !panel.stacked { 0.5 } else { 1.0 },
                false,
            )
        }
        PlotKind::Area | PlotKind::Kde => {
            swatch(
                out,
                if panel.stacked && s.kind == PlotKind::Area {
                    0.85
                } else {
                    0.3
                },
                false,
            );
            line(out);
        }
        PlotKind::Box | PlotKind::Violin => swatch(out, 0.25, true),
        PlotKind::Line | PlotKind::Step | PlotKind::Polar | PlotKind::Fft => line(out),
        PlotKind::Scatter
        | PlotKind::Strip
        | PlotKind::Resid
        | PlotKind::Qq
        | PlotKind::Heatmap
        | PlotKind::Contour => {}
    }
    if s.band {
        // A light block behind the line, like the band.
        let _ = write!(
            out,
            "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" fill-opacity=\"0.22\"/>",
            n(hx),
            n(cy - fs * 0.35),
            n(handle),
            n(fs * 0.7),
            s.color.hex()
        );
    } else if s.yerr.is_some() {
        let e = fs * 0.45;
        let _ = write!(
            out,
            "\n<path d=\"M{x} {}V{}\" stroke=\"{}\" stroke-width=\"{}\"/>",
            n(cy - e),
            n(cy + e),
            s.color.hex(),
            n((s.width * 0.7).max(0.7)),
            x = n(hx + handle / 2.0)
        );
    }
    if s.marker != Marker::None && !matches!(s.kind, PlotKind::Box | PlotKind::Violin) {
        let mut d = String::new();
        marker_path(&mut d, s.marker, hx + handle / 2.0, cy, style.marker_size);
        // Points coloured by a value (`c=`) have no one colour: the map's middle.
        let color = if s.cvalues.is_some() {
            panel.cmap.at(0.5)
        } else {
            s.color
        };
        write_marker_path(out, &d, s.marker, color, s.alpha, style.marker_size);
    }
    text_at(
        out,
        font,
        hx + handle + gap,
        cy + cap / 2.0,
        fs,
        "start",
        &s.label,
    );
    if let Some(note) = &s.note {
        let tfs = style.tick_font_size;
        let _ = write!(out, "\n<g fill-opacity=\"0.75\">");
        text_at(
            out,
            font,
            hx + handle + gap,
            cy + cap / 2.0 + tfs * 1.25,
            tfs,
            "start",
            note,
        );
        out.push_str("</g>");
    }
}

/// Boxes around what's drawn (line pieces, markers, bars, boxes), for placing the legend.
/// Long series are sampled so this stays fast.
pub(super) fn obstacles(panel: &Panel, scales: &Scales) -> Vec<Rect> {
    let mut out = Vec::new();
    let sx = scales.x;
    let slot = bar_slot(panel) * 0.4;
    // Rings from peaks, onset and cross, with a little room for their labels.
    for m in &panel.marks {
        if let Mark::Point { x, y, right, .. } = m {
            let sy = match (scales.y2, *right) {
                (Some(r), true) => r,
                _ => scales.y,
            };
            let (px, py) = (sx.map(*x), sy.map(*y));
            if px.is_finite() && py.is_finite() {
                // Counted several times: a ring matters more than one piece of a line.
                for _ in 0..8 {
                    out.push(Rect::new(px - 12.0, py - 12.0, 24.0, 24.0));
                }
            }
        }
    }
    for s in &panel.series {
        let sy = scales.y_of(s);
        if s.kind == PlotKind::Hist {
            for (k, h) in s.y.iter().enumerate() {
                let (x0, x1, y0, y1) = (
                    sx.map(s.x[k]),
                    sx.map(s.x[k + 1]),
                    sy.baseline(),
                    sy.map(*h),
                );
                if [x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
                    out.push(Rect::new(
                        x0.min(x1),
                        y1.min(y0),
                        (x1 - x0).abs(),
                        (y1 - y0).abs(),
                    ));
                }
            }
            continue;
        }
        for b in &s.boxes {
            let (x0, x1) = (sx.map(b.pos - 0.4), sx.map(b.pos + 0.4));
            let (y0, y1) = (sy.map_or_floor(b.low), sy.map(b.high));
            if [x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
                out.push(Rect::new(x0, y1.min(y0), x1 - x0, (y1 - y0).abs()));
            }
        }
        let step = (s.x.len() / 2000).max(1);
        let pts: Vec<(f64, f64)> =
            s.x.iter()
                .zip(&s.y)
                .step_by(step)
                .map(|(x, y)| (*x, *y))
                .collect();
        for (i, &(x, y)) in pts.iter().enumerate() {
            let (px, py) = (sx.map(x), sy.map(y));
            if !(px.is_finite() && py.is_finite()) {
                continue;
            }
            match s.kind {
                PlotKind::Bar => {
                    let (x0, x1) = (sx.map(x - slot), sx.map(x + slot));
                    let y0 = sy.baseline();
                    out.push(Rect::new(
                        x0.min(x1),
                        py.min(y0),
                        (x1 - x0).abs(),
                        (py - y0).abs(),
                    ));
                }
                PlotKind::Area | PlotKind::Kde => {
                    let y0 = sy.baseline();
                    out.push(Rect::new(px - 2.0, py.min(y0), 4.0, (py - y0).abs()));
                }
                PlotKind::Line | PlotKind::Step | PlotKind::Fft => {
                    out.push(Rect::new(px - 2.0, py - 2.0, 4.0, 4.0));
                    // Small boxes along the segment: one box round a diagonal segment
                    // would block a corner the line only passes by.
                    if let Some(&(nx, ny)) = pts.get(i + 1)
                        && let (qx, qy) = (sx.map(nx), sy.map(ny))
                        && qx.is_finite()
                        && qy.is_finite()
                    {
                        let len = (qx - px).hypot(qy - py);
                        let n = (len / 3.0).ceil().min(200.0) as usize;
                        for j in 1..n {
                            let t = j as f64 / n as f64;
                            let (cx, cy) = (px + (qx - px) * t, py + (qy - py) * t);
                            out.push(Rect::new(cx - 1.5, cy - 1.5, 3.0, 3.0));
                        }
                    }
                }
                _ => out.push(Rect::new(px - 2.0, py - 2.0, 4.0, 4.0)),
            }
        }
        // Error bars (and bands) are drawn too, so they count as well.
        for (k, (&x, &y)) in s.x.iter().zip(&s.y).enumerate().step_by(step) {
            let (px, py) = (sx.map(x), sy.map(y));
            if let Some((lo, hi)) = s.yerr_at(k)
                && lo.is_finite()
                && hi.is_finite()
            {
                let (a, b) = (sy.map(y - lo), sy.map(y + hi));
                if [px, a, b].iter().all(|v| v.is_finite()) {
                    out.push(Rect::new(px - 3.0, a.min(b), 6.0, (a - b).abs()));
                }
            }
            if let Some((lo, hi)) = s.xerr_at(k)
                && lo.is_finite()
                && hi.is_finite()
            {
                let (a, b) = (sx.map(x - lo), sx.map(x + hi));
                if [py, a, b].iter().all(|v| v.is_finite()) {
                    out.push(Rect::new(a.min(b), py - 3.0, (a - b).abs(), 6.0));
                }
            }
        }
    }
    out
}

/// Series that get a legend entry (grids have a colour bar instead).
pub(super) fn legend_series(panel: &Panel) -> impl Iterator<Item = &Series> {
    // A fit whose equation went elsewhere (`fitlabel=`) has no legend entry.
    panel
        .series
        .iter()
        .filter(|s| s.grid.is_none() && !(s.is_fit && s.label.is_empty()))
}

pub(super) fn has_legend(panel: &Panel) -> bool {
    legend_series(panel).next().is_some()
}

pub(super) fn colorbar_ticks(panel: &Panel, target: usize) -> Ticks {
    ticks::linear(panel.zrange.0, panel.zrange.1, target.clamp(3, 8))
}

/// Room on the right for the colour bar, its numbers and its label.
pub(super) fn colorbar_room(panel: &Panel, style: &Style, target: usize) -> f64 {
    // The bar is drawn once the plot's height is known, which may give a tick or two
    // more or fewer than `target`: room for the widest numbers any nearby count gives
    // (0.5 is wider than 1).
    let w = (target.saturating_sub(2).max(3)..=(target + 2).min(8))
        .flat_map(|t| colorbar_ticks(panel, t).labels)
        .map(|l| mathtext::width(style.font, &l, style.tick_font_size))
        .fold(0.0, f64::max);
    let label = if panel.zlabel.is_some() {
        style.font_size * 1.25 + 2.0
    } else {
        0.0
    };
    CB_GAP + CB_WIDTH + TICK_GAP + w + label
}

pub(super) fn zfrac(panel: &Panel, v: f64) -> f64 {
    let (lo, hi) = panel.zrange;
    (v - lo) / (hi - lo)
}

/// The colour bar: the colour scale from low (bottom) to high, with ticks and a label.
pub(super) fn draw_colorbar(
    out: &mut String,
    panel: &Panel,
    plot: Rect,
    cell: Rect,
    style: &Style,
) {
    let x = plot.right() + CB_GAP;
    let steps = 96;
    let h = plot.h / steps as f64;
    for k in 0..steps {
        let t = (k as f64 + 0.5) / steps as f64;
        let c = if panel.filled && !panel.levels.is_empty() {
            // Filled contours: the bar shows the bands.
            let v = panel.zrange.0 + t * (panel.zrange.1 - panel.zrange.0);
            band_color(panel, v)
        } else {
            panel.cmap.at(t)
        };
        let _ = write!(
            out,
            "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            n(x),
            n(plot.bottom() - h * (k + 1) as f64),
            n(CB_WIDTH),
            n(h + 0.3),
            c.hex()
        );
    }
    let fg = style.foreground.hex();
    let _ = write!(
        out,
        "\n<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
        n(x),
        n(plot.y),
        n(CB_WIDTH),
        n(plot.h),
        n(style.axis_width * 0.75)
    );
    let t = colorbar_ticks(panel, (plot.h / 40.0) as usize);
    let tfs = style.tick_font_size;
    let cap = text::cap_height(style.font, tfs);
    let mut d = String::new();
    for (v, label) in t.major.iter().zip(&t.labels) {
        let f = zfrac(panel, *v);
        if !(-1e-9..=1.0 + 1e-9).contains(&f) {
            continue;
        }
        let y = plot.bottom() - f * plot.h;
        let _ = write!(d, "M{} {}h{}", n(x + CB_WIDTH), n(y), n(-style.minor_tick));
        text_at(
            out,
            style.font,
            x + CB_WIDTH + TICK_GAP,
            y + cap / 2.0,
            tfs,
            "start",
            label,
        );
    }
    let _ = write!(
        out,
        "\n<path d=\"{d}\" stroke=\"{fg}\" stroke-width=\"{}\"/>",
        n(style.tick_width)
    );
    if let Some(label) = &panel.zlabel {
        let y = plot.y + plot.h / 2.0;
        let lx = cell.right() - style.pad - text::descent(style.font, style.font_size);
        let _ = write!(
            out,
            "\n<g transform=\"translate({} {}) rotate(-90)\">",
            n(lx),
            n(y)
        );
        text_at(out, style.font, 0.0, 0.0, style.font_size, "middle", label);
        out.push_str("</g>");
    }
}
