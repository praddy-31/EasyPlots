//! Everything drawn on top of the data that isn't data: shaded spans, shapes, reference
//! lines, marks from analysis (peaks, onsets, crossings) and notes.

use super::*;

/// `span=` / `yspan=`: light shaded regions behind everything.
pub(super) fn draw_spans(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    plot: Rect,
    style: &Style,
) {
    let mut d = String::new();
    for (a, b) in &panel.spans {
        let (x0, x1) = (sx.map(*a), sx.map(*b));
        if x0.is_finite() && x1.is_finite() {
            let _ = write!(
                d,
                "M{} {}H{}V{}H{}Z",
                n(x0.min(x1)),
                n(plot.y),
                n(x0.max(x1)),
                n(plot.bottom()),
                n(x0.min(x1))
            );
        }
    }
    // yspan2= on the right axis's scale.
    let right = panel.yspans2.iter().filter_map(|r| sy2.map(|s| (s, r)));
    for (s, (a, b)) in panel.yspans.iter().map(|r| (sy, r)).chain(right) {
        let (y0, y1) = (s.map(*a), s.map(*b));
        if y0.is_finite() && y1.is_finite() {
            let _ = write!(
                d,
                "M{} {}V{}H{}V{}Z",
                n(plot.x),
                n(y0.min(y1)),
                n(y0.max(y1)),
                n(plot.right()),
                n(y0.min(y1))
            );
        }
    }
    if !d.is_empty() {
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"{}\" fill-opacity=\"0.1\" stroke=\"none\"/>",
            style.foreground.hex()
        );
    }
}

/// The corners or ends of a shape, so the axes include it.
pub(super) fn shape_points(s: &Shape) -> [(f64, f64); 2] {
    match s {
        Shape::Rect { x, y, .. } => [(x.0, y.0), (x.1, y.1)],
        Shape::Circle { x, y, .. } => [(*x, *y), (*x, *y)],
        Shape::Segment { from, to } => [*from, *to],
    }
}

/// `rect=`, `circle=`, `segment=`: outlines in the text colour (inside the plot's clip;
/// also in an inset).
pub(super) fn draw_shapes(out: &mut String, panel: &Panel, sx: &Scale, sy: &Scale, style: &Style) {
    let radius = shape_radius(style);
    let mut d = String::new();
    for shape in &panel.shapes {
        match shape {
            Shape::Rect { x, y, .. } => {
                let (x0, x1) = (sx.map(x.0), sx.map(x.1));
                let (y0, y1) = (sy.map(y.1), sy.map(y.0));
                if [x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
                    let (left, top) = (x0.min(x1), y0.min(y1));
                    let _ = write!(
                        d,
                        "M{} {}H{}V{}H{}Z",
                        n(left),
                        n(top),
                        n(x0.max(x1)),
                        n(y0.max(y1)),
                        n(left)
                    );
                }
            }
            Shape::Circle { x, y, .. } => {
                let (px, py) = (sx.map(*x), sy.map(*y));
                if px.is_finite() && py.is_finite() {
                    let _ = write!(
                        d,
                        "M{} {}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 {} 0",
                        n(px - radius),
                        n(py),
                        n(2.0 * radius),
                        n(-2.0 * radius),
                        r = n(radius)
                    );
                }
            }
            Shape::Segment { from, to } => {
                let (ax, ay, bx, by) = (sx.map(from.0), sy.map(from.1), sx.map(to.0), sy.map(to.1));
                if [ax, ay, bx, by].iter().all(|v| v.is_finite()) {
                    let _ = write!(d, "M{} {}L{} {}", n(ax), n(ay), n(bx), n(by));
                }
            }
        }
    }
    if !d.is_empty() {
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
            style.foreground.hex(),
            n(style.line_width * 0.7)
        );
    }
}

/// The y scale a mark is measured on.
pub(super) fn mark_scale<'a>(sy: &'a Scale, sy2: Option<&'a Scale>, right: bool) -> &'a Scale {
    match (sy2, right) {
        (Some(r), true) => r,
        _ => sy,
    }
}

/// `area=`: the measured part of each curve, shaded lightly in its colour down to y = 0
/// (or the bottom of the plot on a log axis), under everything else.
pub(super) fn draw_mark_fills(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    plot: Rect,
) {
    for m in &panel.marks {
        let Mark::Fill {
            outline,
            color,
            right,
        } = m
        else {
            continue;
        };
        let sy = mark_scale(sy, sy2, *right);
        let zero = sy.map(0.0);
        let base = if zero.is_finite() {
            zero.clamp(plot.y, plot.bottom())
        } else {
            plot.bottom()
        };
        let pts: Vec<(f64, f64)> = outline
            .iter()
            .map(|(x, y)| (sx.map(*x), sy.map(*y)))
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect();
        let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
            continue;
        };
        let mut d = format!("M{} {}", n(first.0), n(base));
        for (x, y) in &pts {
            let _ = write!(d, "L{} {}", n(*x), n(*y));
        }
        let _ = write!(d, "L{} {}Z", n(last.0), n(base));
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"{}\" fill-opacity=\"0.22\" stroke=\"none\"/>",
            color.hex()
        );
    }
}

/// `onset`: the baseline and the tangent, thin and dashed in the curve's colour; and
/// `showbaseline`'s fitted baselines.
pub(super) fn draw_mark_lines(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    style: &Style,
) {
    let w = style.line_width * 0.6;
    for m in &panel.marks {
        let (pts, color, right) = match m {
            Mark::Line {
                from,
                to,
                color,
                right,
            } => (vec![*from, *to], color, right),
            Mark::Path {
                points,
                color,
                right,
            } => (points.clone(), color, right),
            _ => continue,
        };
        let sy = mark_scale(sy, sy2, *right);
        let mut d = String::new();
        for (x, y) in &pts {
            let (px, py) = (sx.map(*x), sy.map(*y));
            if px.is_finite() && py.is_finite() {
                let _ = write!(
                    d,
                    "{}{} {}",
                    if d.is_empty() { "M" } else { "L" },
                    n(px),
                    n(py)
                );
            }
        }
        if d.matches(['M', 'L']).count() >= 2 {
            let _ = write!(
                out,
                "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-dasharray=\"{} {}\"/>",
                color.hex(),
                n(w),
                n(4.0 * w),
                n(2.5 * w)
            );
        }
    }
}

/// Whether the straight line from `a` to `b` passes through box `r` (left, top, right,
/// bottom).
fn segment_hits(a: (f64, f64), b: (f64, f64), r: (f64, f64, f64, f64)) -> bool {
    // Liang–Barsky: narrow the part of the segment (t0..t1) that lies inside each edge.
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.0 - r.0),
        (dx, r.2 - a.0),
        (-dy, a.1 - r.1),
        (dy, r.3 - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Points on the page, joined in order.
type Polyline = Vec<(f64, f64)>;

/// The panel's drawn lines (as points on the page) and its markers, for labels to keep
/// clear of.
fn drawn_curves(
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
) -> (Vec<Polyline>, Polyline) {
    let mut lines = Vec::new();
    let mut dots = Vec::new();
    for s in &panel.series {
        if s.grid.is_some() || !s.boxes.is_empty() {
            continue;
        }
        let sy = mark_scale(sy, sy2, s.right);
        let page = |s: &Series| -> Vec<(f64, f64)> {
            s.x.iter()
                .zip(&s.y)
                .map(|(x, y)| (sx.map(*x), sy.map(*y)))
                .collect()
        };
        match s.kind {
            PlotKind::Line | PlotKind::Fft | PlotKind::Area | PlotKind::Kde => lines.push(page(s)),
            PlotKind::Step => lines.push(page(&stepped(s))),
            _ => {}
        }
        if s.marker != Marker::None && s.kind != PlotKind::Strip {
            dots.extend(page(s));
        }
    }
    for m in &panel.marks {
        if let Mark::Line {
            from, to, right, ..
        } = m
        {
            let sy = mark_scale(sy, sy2, *right);
            lines.push(vec![
                (sx.map(from.0), sy.map(from.1)),
                (sx.map(to.0), sy.map(to.1)),
            ]);
        }
    }
    (lines, dots)
}

/// `peaks`, `dips`, `onset`, `cross`: a ring in the curve's colour with a value beside it;
/// not clipped. Each label tries above, below, right and left of its ring, then the four
/// diagonals, and takes the first place that stays inside the plot, clear of the labels
/// already placed and off the drawn curves and markers (failing that, the place that
/// covers the fewest of them).
pub(super) fn draw_mark_points(
    out: &mut String,
    panel: &Panel,
    scales: &Scales,
    plot: Rect,
    style: &Style,
    covered: &[Rect],
) {
    let (sx, sy, sy2) = (scales.x, scales.y, scales.y2);
    let font = style.font;
    let tfs = style.tick_font_size;
    let cap = text::cap_height(font, tfs);
    let r = style.marker_size * 1.4 + 1.5;
    let gap = 2.0;
    // Placed labels as (left, top, right, bottom), after the legend and the inset.
    let mut taken: Vec<(f64, f64, f64, f64)> = covered
        .iter()
        .map(|c| (c.x, c.y, c.right(), c.bottom()))
        .collect();
    let clear = |b: (f64, f64, f64, f64), taken: &[(f64, f64, f64, f64)]| {
        b.0 >= plot.x
            && b.2 <= plot.right()
            && b.1 >= plot.y
            && b.3 <= plot.bottom()
            && taken
                .iter()
                .all(|t| b.2 <= t.0 || b.0 >= t.2 || b.3 <= t.1 || b.1 >= t.3)
    };
    // The rings first, so that no label covers any ring.
    let mut rings: Vec<(f64, f64, &str, Color)> = Vec::new();
    for m in &panel.marks {
        let Mark::Point {
            x,
            y,
            label,
            color,
            right,
        } = m
        else {
            continue;
        };
        let sy = mark_scale(sy, sy2, *right);
        let (px, py) = (sx.map(*x), sy.map(*y));
        if px.is_finite() && py.is_finite() && plot.contains(px, py) {
            let _ = write!(
                out,
                "\n<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
                n(px),
                n(py),
                n(r),
                color.hex(),
                n(style.line_width * 0.8)
            );
            taken.push((px - r, py - r, px + r, py + r));
            rings.push((px, py, label, *color));
        }
    }
    let (lines, dots) = if rings.iter().any(|r| !r.2.is_empty()) {
        drawn_curves(panel, sx, sy, sy2)
    } else {
        (Vec::new(), Vec::new())
    };
    let pad = style.line_width + 1.0;
    let dot = style.marker_size / 2.0 + 1.0;
    // How much drawn data a label at `b` would cover: curves passing through, then markers.
    let covered = |b: (f64, f64, f64, f64)| -> usize {
        let wide = (b.0 - pad, b.1 - pad, b.2 + pad, b.3 + pad);
        let crossing = lines
            .iter()
            .filter(|l| {
                l.windows(2).any(|w| {
                    w[0].0.is_finite()
                        && w[0].1.is_finite()
                        && w[1].0.is_finite()
                        && w[1].1.is_finite()
                        && segment_hits(w[0], w[1], wide)
                })
            })
            .count();
        let under = dots
            .iter()
            .filter(|d| d.0 > b.0 - dot && d.0 < b.2 + dot && d.1 > b.1 - dot && d.1 < b.3 + dot)
            .count();
        crossing * 1000 + under
    };
    // Then each label, in its curve's colour.
    for (px, py, label, color) in rings {
        if label.is_empty() {
            continue;
        }
        let w = text::width(font, label, tfs);
        // Diagonal places start at the corners of the ring's square.
        let d = r;
        // Boxes (left, top, right, bottom) above, below, right and left of the ring.
        let places = [
            (px - w / 2.0, py - r - gap - cap, px + w / 2.0, py - r - gap),
            (px - w / 2.0, py + r + gap, px + w / 2.0, py + r + gap + cap),
            (
                px + r + gap,
                py - cap / 2.0,
                px + r + gap + w,
                py + cap / 2.0,
            ),
            (
                px - r - gap - w,
                py - cap / 2.0,
                px - r - gap,
                py + cap / 2.0,
            ),
            // The diagonals: beside the ring's corners.
            (px + d, py - d - cap, px + d + w, py - d),
            (px - d - w, py - d - cap, px - d, py - d),
            (px + d, py + d, px + d + w, py + d + cap),
            (px - d - w, py + d, px - d, py + d + cap),
        ];
        let free: Vec<(f64, f64, f64, f64)> = places
            .iter()
            .copied()
            .filter(|b| clear(*b, &taken))
            .collect();
        // `min_by_key` keeps the first of equals, so above still wins when nothing is in
        // the way.
        let b = free
            .iter()
            .copied()
            .min_by_key(|b| covered(*b))
            .unwrap_or(places[0]);
        taken.push(b);
        // A thin halo in the background colour keeps the label readable over a line.
        let _ = write!(
            out,
            "\n<g fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" paint-order=\"stroke\">",
            color.hex(),
            style.background.hex(),
            n(tfs * 0.22)
        );
        text_at(out, font, b.0, b.3, tfs, "start", label);
        out.push_str("</g>");
    }
}

/// The ring of `circle=`: a fixed size on the page.
pub(super) fn shape_radius(style: &Style) -> f64 {
    (style.marker_size * 2.2).max(6.0)
}

/// Labels of `rect=` (inside its top left) and `circle=` (above right, or below right near
/// the top of the plot); not clipped, so they may reach past the plot's edge.
pub(super) fn draw_shape_labels(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    plot: Rect,
    style: &Style,
) {
    let font = style.font;
    let tfs = style.tick_font_size;
    let cap = text::cap_height(font, tfs);
    let r = shape_radius(style);
    for shape in &panel.shapes {
        match shape {
            Shape::Rect {
                x,
                y,
                label: Some(l),
            } => {
                let (left, top) = (sx.map(x.0).min(sx.map(x.1)), sy.map(y.1).min(sy.map(y.0)));
                if left.is_finite() && top.is_finite() && plot.contains(left, top) {
                    text_at(out, font, left + 3.0, top + cap + 3.0, tfs, "start", l);
                }
            }
            Shape::Circle {
                x,
                y,
                label: Some(l),
            } => {
                let (px, py) = (sx.map(*x), sy.map(*y));
                if px.is_finite() && py.is_finite() && plot.contains(px, py) {
                    let above = py - r * 0.8 - cap > plot.y;
                    let ty = if above {
                        py - r * 0.8
                    } else {
                        py + r * 0.8 + cap
                    };
                    text_at(out, font, px + r * 0.8 + 2.0, ty, tfs, "start", l);
                }
            }
            _ => {}
        }
    }
}

/// `hline=` / `vline=`: thin dashed lines across the plot, under the data lines.
pub(super) fn draw_ref_lines(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    plot: Rect,
    style: &Style,
) {
    let mut d = String::new();
    for v in &panel.vlines {
        let x = sx.map(*v);
        if x.is_finite() {
            let _ = write!(d, "M{} {}V{}", n(x), n(plot.y), n(plot.bottom()));
        }
    }
    let right = panel.hlines2.iter().filter_map(|v| sy2.map(|s| (s, v)));
    for (s, v) in panel.hlines.iter().map(|v| (sy, v)).chain(right) {
        let y = s.map(*v);
        if y.is_finite() {
            let _ = write!(d, "M{} {}H{}", n(plot.x), n(y), n(plot.right()));
        }
    }
    if !d.is_empty() {
        let w = style.line_width * 0.6;
        let _ = write!(
            out,
            "\n<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"0.6\" stroke-width=\"{}\" stroke-dasharray=\"{} {}\"/>",
            style.foreground.hex(),
            n(w),
            n(4.0 * w),
            n(2.5 * w)
        );
    }
}

/// Labels on reference lines (`hline=20:"limit"`) and regions (`span=5:8:"heating"`), in
/// the tick font, just inside the plot.
pub(super) fn draw_ref_labels(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    plot: Rect,
    style: &Style,
) {
    let font = style.font;
    let tfs = style.tick_font_size;
    let cap = text::cap_height(font, tfs);
    let inside =
        |v: f64, lo: f64, hi: f64| v.is_finite() && v >= lo.min(hi) - 0.5 && v <= lo.max(hi) + 0.5;
    let mut text = String::new();
    for l in &panel.ref_labels {
        // hline2= / yspan2= labels on the right axis's scale.
        let sy = match (l.second, sy2) {
            (true, Some(s)) => s,
            (true, None) => continue,
            _ => sy,
        };
        let placed = match l.kind {
            RefKind::HLine => {
                let y = sy.map(l.at.0);
                inside(y, plot.y, plot.bottom()).then(|| (plot.right() - 3.0, y - 3.0, "end"))
            }
            RefKind::VLine => {
                let x = sx.map(l.at.0);
                inside(x, plot.x, plot.right()).then_some((x + 3.0, plot.y + cap + 3.0, "start"))
            }
            RefKind::Span => {
                let (a, b) = (sx.map(l.at.0), sx.map(l.at.1));
                let mid = ((a + b) / 2.0).clamp(plot.x, plot.right());
                (a.is_finite() && b.is_finite()).then_some((mid, plot.y + cap + 3.0, "middle"))
            }
            RefKind::YSpan => {
                let (a, b) = (sy.map(l.at.0), sy.map(l.at.1));
                let mid = ((a + b) / 2.0).clamp(plot.y + cap, plot.bottom());
                (a.is_finite() && b.is_finite()).then(|| (plot.x + 4.0, mid + cap / 2.0, "start"))
            }
        };
        if let Some((x, y, anchor)) = placed {
            text_at(&mut text, font, x, y, tfs, anchor, &l.text);
        }
    }
    if !text.is_empty() {
        let _ = write!(
            out,
            "\n<g fill=\"{}\" fill-opacity=\"0.8\">{text}\n</g>",
            style.foreground.hex()
        );
    }
}

/// `note="peak"@12,30` (text at a point) and `arrow="peak"@12,30` (text a little way off,
/// towards the middle of the plot, with an arrow to the point).
pub(super) fn draw_notes(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    sy2: Option<&Scale>,
    plot: Rect,
    style: &Style,
) {
    let scales = Scales {
        x: sx,
        y: sy,
        y2: sy2,
    };
    let font = style.font;
    let tfs = style.tick_font_size;
    let cap = text::cap_height(font, tfs);
    let fg = style.foreground.hex();
    for (_, x, y, anchor, text) in corner_notes(panel, plot, style, &scales) {
        text_at(out, font, x, y, tfs, anchor, text);
    }
    for note in panel.notes.iter().filter(|n| n.corner.is_none()) {
        let (px, py) = (sx.map(note.x), sy.map(note.y));
        if !(px.is_finite() && py.is_finite()) {
            continue;
        }
        if !note.arrow {
            text_at(out, font, px + 3.0, py - 3.0, tfs, "start", &note.text);
            continue;
        }
        let w = mathtext::width(font, &note.text, tfs);
        // Text goes towards the middle sideways, and above the point unless that leaves
        // the plot.
        let right = px < plot.x + plot.w / 2.0;
        let above = py - 28.0 - cap > plot.y;
        let mut tx = if right { px + 30.0 } else { px - 30.0 };
        tx = if right {
            tx.min(plot.right() - w - 2.0).max(plot.x + 2.0)
        } else {
            tx.max(plot.x + w + 2.0).min(plot.right() - 2.0)
        };
        let ty = if above { py - 28.0 } else { py + 28.0 + cap };
        text_at(
            out,
            font,
            tx,
            ty,
            tfs,
            if right { "start" } else { "end" },
            &note.text,
        );
        // The arrow starts under (or over) the near end of the text.
        let sx0 = if right {
            tx + w.min(8.0)
        } else {
            tx - w.min(8.0)
        };
        let sy0 = if above { ty + 3.0 } else { ty - cap - 3.0 };
        let (dx, dy) = (px - sx0, py - sy0);
        let len = dx.hypot(dy);
        if len < 6.0 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let lw = style.line_width * 0.6;
        let (tipx, tipy) = (px - ux * 1.5, py - uy * 1.5);
        let (bx, by) = (tipx - ux * 5.0, tipy - uy * 5.0);
        let _ = write!(
            out,
            "\n<path d=\"M{} {}L{} {}\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{}\"/>\
             \n<path d=\"M{} {}L{} {}L{} {}Z\" fill=\"{fg}\" stroke=\"none\"/>",
            n(sx0),
            n(sy0),
            n(bx),
            n(by),
            n(lw),
            n(tipx),
            n(tipy),
            n(bx - uy * 2.2),
            n(by + ux * 2.2),
            n(bx + uy * 2.2),
            n(by - ux * 2.2)
        );
    }
}

/// Notes in the plot's corners (`note="…"@tr`, `fitlabel=tr`, `corr`): each corner's
/// notes stacked from the corner inwards, and past the legend when it sits in that
/// corner. A note placed automatically (`corr`, corner `Best`) takes the corner with the
/// least data under it. Returns each note's box, its text position and anchor, its text.
pub(super) fn corner_notes<'a>(
    panel: &'a Panel,
    plot: Rect,
    style: &Style,
    scales: &Scales,
) -> Vec<(Rect, f64, f64, &'static str, &'a str)> {
    let font = style.font;
    let tfs = style.tick_font_size;
    let cap = text::cap_height(font, tfs);
    let line = tfs * 1.35;
    let margin = 6.0;
    let corners = [
        LegendPos::BottomRight,
        LegendPos::TopLeft,
        LegendPos::TopRight,
        LegendPos::BottomLeft,
    ];
    // Where the automatic notes go: the corner whose box covers the fewest data points.
    let auto: Vec<&str> = panel
        .notes
        .iter()
        .filter(|n| n.corner == Some(LegendPos::Best))
        .map(|n| n.text.as_str())
        .collect();
    let auto_corner = (!auto.is_empty()).then(|| {
        let w = auto
            .iter()
            .map(|t| mathtext::width(font, t, tfs))
            .fold(0.0, f64::max);
        let h = line * auto.len() as f64;
        let obstacles = obstacles(panel, scales);
        corners
            .into_iter()
            .filter(|c| !(has_legend(panel) && panel.legend == *c))
            .min_by_key(|c| {
                let x = if matches!(c, LegendPos::TopRight | LegendPos::BottomRight) {
                    plot.right() - margin - w
                } else {
                    plot.x + margin
                };
                let y = if matches!(c, LegendPos::TopLeft | LegendPos::TopRight) {
                    plot.y + margin
                } else {
                    plot.bottom() - margin - h
                };
                let r = Rect::new(x, y, w, h);
                obstacles.iter().filter(|o| o.overlaps(&r)).count()
            })
            .unwrap_or(LegendPos::BottomRight)
    });
    let corner_of = |n: &crate::spec::Note| match n.corner {
        Some(LegendPos::Best) => auto_corner,
        c => c,
    };
    let mut out = Vec::new();
    for corner in [
        LegendPos::TopLeft,
        LegendPos::TopRight,
        LegendPos::BottomLeft,
        LegendPos::BottomRight,
    ] {
        let notes: Vec<&str> = panel
            .notes
            .iter()
            .filter(|n| corner_of(n) == Some(corner))
            .map(|n| n.text.as_str())
            .collect();
        if notes.is_empty() {
            continue;
        }
        let top = matches!(corner, LegendPos::TopLeft | LegendPos::TopRight);
        let right = matches!(corner, LegendPos::TopRight | LegendPos::BottomRight);
        // A legend fixed in the same corner comes first; the notes go past it.
        let legend_here = has_legend(panel) && panel.legend == corner;
        let skip = if legend_here {
            legend_size(panel, style).1 + 4.0
        } else {
            0.0
        };
        let count = notes.len();
        for (k, text) in notes.into_iter().enumerate() {
            let w = mathtext::width(font, text, tfs);
            // Read top to bottom in every corner: at the bottom the first line is the
            // furthest from the edge.
            let row = if top { k } else { count - 1 - k };
            let step = skip + line * row as f64;
            let baseline = if top {
                plot.y + margin + cap + step
            } else {
                plot.bottom() - margin - step
            };
            let (x, anchor) = if right {
                (plot.right() - margin, "end")
            } else {
                (plot.x + margin, "start")
            };
            let left = if right { x - w } else { x };
            out.push((
                Rect::new(left - 2.0, baseline - cap - 2.0, w + 4.0, cap + 4.0),
                x,
                baseline,
                anchor,
                text,
            ));
        }
    }
    out
}

/// The size of the notes under group names (`stats`).
pub(super) fn note_size(style: &Style) -> f64 {
    style.tick_font_size * 0.82
}

/// How many lines the notes under group names take (0 without `stats`).
pub(super) fn note_lines(panel: &Panel) -> usize {
    panel
        .cat_notes
        .iter()
        .map(|n| n.lines().count())
        .max()
        .unwrap_or(0)
}
