//! Drawing grouped data: strips (every value as a dot, spread as a beeswarm or at
//! random), the mean lines across them, boxes in their own colours (`highlight=`), and
//! the sideways view for `horizontal`.
//!
//! **Horizontal plots** keep their data upright (groups along x, values up y) and are
//! drawn through a mirror across the diagonal: SVG's `matrix(0 1 1 0 0 0)` swaps every
//! x and y, so a vertical bar comes out horizontal, with its error bar, and boxes and
//! violins the same, without a second drawing routine for each. Only what must not be
//! mirrored is drawn after the turn: text, and markers (a triangle would point sideways).
//! The axes are those of a second panel, [`Sideways::axes`]: numbers along the bottom,
//! group names down the side, the first group at the top.

use std::fmt::Write;

use super::{Rect, Scale, Style, bar_offset, escape, mathtext, n, text, x_range, y_range};
use crate::figure::{AxisKind, Mark, Panel, Series};
use crate::spec::{Color, Marker, PlotKind};

/// A series once per colour of its boxes (`highlight=` greys some of them).
pub(super) fn each_colour(s: &Series) -> Vec<Series> {
    if s.colors.is_empty() {
        return vec![s.clone()];
    }
    let mut out: Vec<Series> = Vec::new();
    for (i, b) in s.boxes.iter().enumerate() {
        let c = s.color_at(i);
        match out.iter_mut().find(|o| o.color == c) {
            Some(o) => o.boxes.push(b.clone()),
            None => out.push(Series {
                boxes: vec![b.clone()],
                color: c,
                colors: Vec::new(),
                ..s.clone()
            }),
        }
    }
    out
}

/// `mean` / `median`: a short line across each group at its mean (solid) or median
/// (dashed).
pub(super) fn draw_levels(out: &mut String, panel: &Panel, sx: &Scale, sy: &Scale, style: &Style) {
    for m in &panel.marks {
        let Mark::Level {
            from,
            to,
            color,
            dashed,
            ..
        } = m
        else {
            continue;
        };
        let (x0, x1, y) = (sx.map(from.0), sx.map(to.0), sy.map(from.1));
        if ![x0, x1, y].iter().all(|v| v.is_finite()) {
            continue;
        }
        let _ = write!(
            out,
            "\n<path d=\"M{} {}H{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{}/>",
            n(x0),
            n(y),
            n(x1),
            color.hex(),
            n(style.line_width * 1.8),
            if *dashed {
                format!(
                    " stroke-dasharray=\"{} {}\"",
                    n(style.line_width * 3.0),
                    n(style.line_width * 2.0)
                )
            } else {
                String::new()
            }
        );
    }
}

/// `mean=label`: each mean's value beside its line (right of it upright, above it on its
/// side). `sx`/`sy` are the upright data scales; `swap` says they're turned.
pub(super) fn draw_level_labels(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    style: &Style,
    swap: bool,
) {
    let tfs = style.tick_font_size * 0.9;
    let cap = text::cap_height(style.font, tfs);
    let mut labels = String::new();
    for m in &panel.marks {
        let Mark::Level {
            from,
            to,
            label: Some(label),
            ..
        } = m
        else {
            continue;
        };
        let (a, b, v) = (sx.map(from.0), sx.map(to.0), sy.map(from.1));
        if ![a, b, v].iter().all(|v| v.is_finite()) {
            continue;
        }
        let (x, y, anchor) = if swap {
            (v, a.min(b) - 3.0, "middle")
        } else {
            (a.max(b) + 3.0, v + cap / 2.0, "start")
        };
        let _ = write!(
            labels,
            "\n<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"{anchor}\">{}</text>",
            n(x),
            n(y),
            n(tfs),
            escape(&mathtext::plain(label))
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

/// More dots than this in one group: a random spread instead of the beeswarm (whose
/// placing takes time growing with the square of the count).
const SWARM_LIMIT: usize = 1500;

/// Where each of a strip's dots goes, in data units: its own value, and its group's
/// position moved sideways so dots don't cover each other (a beeswarm), or at random with
/// a fixed seed (`jitter`). Dots never leave their group's slot (`spread`); if a group is
/// too crowded for that, they overlap at its edges.
fn spread_points(panel: &Panel, s: &Series, sx: &Scale, sy: &Scale, size: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = s.x.iter().copied().zip(s.y.iter().copied()).collect();
    // The groups: points sharing an x position.
    let mut xs: Vec<f64> = s.x.iter().copied().filter(|x| x.is_finite()).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    for (gi, &x) in xs.iter().enumerate() {
        let idx: Vec<usize> = (0..s.x.len())
            .filter(|&i| s.x[i] == x && s.y[i].is_finite())
            .collect();
        // Points per data unit across the group (and so how far `spread` reaches).
        let per_unit = sx.map(x + 1.0) - sx.map(x);
        if !per_unit.is_finite() || per_unit == 0.0 {
            continue;
        }
        let reach = s.spread * per_unit.abs();
        if panel.jitter || idx.len() > SWARM_LIMIT {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (gi as u64 + 1).wrapping_mul(0xBF58_476D));
            for &i in &idx {
                out[i].0 = x + (rng.next() * 2.0 - 1.0) * s.spread * 0.85;
            }
            continue;
        }
        // Beeswarm: from the lowest value up, each dot goes as near the middle as it can
        // without touching one already placed.
        let d = size * 1.08;
        let mut order: Vec<(usize, f64)> = idx.iter().map(|&i| (i, sy.map(s.y[i]))).collect();
        order.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut placed: Vec<(f64, f64)> = Vec::new(); // (sideways offset, value), in points
        for (i, v) in order {
            let near: Vec<(f64, f64)> = placed
                .iter()
                .copied()
                .filter(|(_, pv)| (v - pv).abs() < d)
                .collect();
            let mut candidates = vec![0.0];
            for (pu, pv) in &near {
                let dx = (d * d - (v - pv) * (v - pv)).max(0.0).sqrt();
                candidates.push(pu + dx);
                candidates.push(pu - dx);
            }
            let free = |u: f64| {
                near.iter()
                    .all(|(pu, pv)| (u - pu).powi(2) + (v - pv).powi(2) >= d * d * 0.999)
            };
            candidates.sort_by(|a: &f64, b: &f64| a.abs().total_cmp(&b.abs()));
            let u = candidates
                .iter()
                .copied()
                .find(|u| u.abs() <= reach && free(*u))
                .unwrap_or_else(|| candidates[0].clamp(-reach, reach));
            // Not quite free: the least crowded spot inside the slot.
            let u = if free(u) || near.is_empty() {
                u
            } else {
                let tries = (-8..=8).map(|k| reach * f64::from(k) / 8.0);
                tries
                    .max_by(|a, b| {
                        let room = |u: f64| {
                            near.iter()
                                .map(|(pu, pv)| (u - pu).powi(2) + (v - pv).powi(2))
                                .fold(f64::INFINITY, f64::min)
                        };
                        room(*a).total_cmp(&room(*b))
                    })
                    .unwrap_or(u)
            };
            placed.push((u, v));
            out[i].0 = x + u / per_unit;
        }
    }
    out
}

/// A small fixed-seed random number source (xorshift), so `jitter` looks the same on
/// every redraw and in every format.
struct Rng(u64);

impl Rng {
    /// A number from 0 up to (not including) 1.
    fn next(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Strip plots: each value as a dot with a thin edge in the background colour, so
/// touching dots stay countable. `swap`: the scales are the upright ones of a sideways
/// plot, and the dots go where the turn puts them.
pub(super) fn draw_strips(
    out: &mut String,
    panel: &Panel,
    sx: &Scale,
    sy: &Scale,
    style: &Style,
    swap: bool,
) {
    let size = style.marker_size * 1.15;
    for s in panel.series.iter().filter(|s| s.kind == PlotKind::Strip) {
        let points = spread_points(panel, s, sx, sy, size);
        let mut paths: Vec<(Color, String)> = Vec::new();
        for (i, (x, y)) in points.into_iter().enumerate() {
            let (px, py) = if swap {
                (sy.map(y), sx.map(x))
            } else {
                (sx.map(x), sy.map(y))
            };
            if !(px.is_finite() && py.is_finite()) {
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
            let marker = if s.marker == Marker::None {
                Marker::Circle
            } else {
                s.marker
            };
            super::marker_path(d, marker, px, py, size);
        }
        for (c, d) in paths {
            if s.marker.is_outline() {
                super::write_marker_path(out, &d, s.marker, c, s.alpha, size);
            } else {
                let _ = write!(
                    out,
                    "\n<path d=\"{d}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}/>",
                    c.hex(),
                    style.background.hex(),
                    n(size * 0.12),
                    super::opacity("fill", s.alpha)
                );
            }
        }
    }
}

/// A horizontal panel taken apart for drawing: `axes` is what the page shows (numbers
/// along x, group names down y, the first at the top), `data` the upright panel drawn
/// through the mirror (its reference lines moved to match).
pub(super) struct Sideways {
    pub axes: Panel,
    pub data: Panel,
    /// The axes' ranges: values along x, group positions down y.
    pub x: (f64, f64),
    pub y: (f64, f64),
}

pub(super) fn sideways(p: &Panel) -> Sideways {
    let groups = p.categories.len() as f64;
    // Group k sits at height (groups - 1 - k) on the page, so the first is at the top.
    let flip = |v: f64| groups - 1.0 - v;
    // Still marked horizontal: its strips are drawn after the turn, not through it.
    let mut data = p.clone();
    data.ylim = p.xlim;
    data.ystep = p.xstep;
    data.logy = p.logx;
    data.xlim = (None, None);
    data.xstep = None;
    data.logx = false;
    data.hlines = p.vlines.clone();
    data.yspans = p.spans.clone();
    data.vlines = p.hlines.iter().map(|v| flip(*v)).collect();
    data.spans = p.yspans.iter().map(|(a, b)| (flip(*b), flip(*a))).collect();
    data.ref_labels.clear();
    data.notes.clear();
    data.shapes.clear();
    let x = y_range(&data, false);
    let (clo, chi) = x_range(&data);
    let y = (flip(chi), flip(clo));

    let mut axes = p.clone();
    axes.xaxis = AxisKind::Number;
    axes.categories = Vec::new();
    axes.ycategories = p.categories.iter().rev().cloned().collect();
    axes.bold_categories = p
        .bold_categories
        .iter()
        .map(|c| p.categories.len() - 1 - c)
        .collect();
    axes.cat_notes = p.cat_notes.iter().rev().cloned().collect();
    axes.marks.clear();
    axes.values = false;
    // For the legend: the series as seen on the page.
    axes.series = p
        .series
        .iter()
        .map(|s| Series {
            x: s.y.clone(),
            y: s.x.iter().map(|v| flip(*v)).collect(),
            xerr: s.yerr.clone(),
            yerr: s.xerr.clone(),
            xerr_lo: s.yerr_lo.clone(),
            yerr_lo: s.xerr_lo.clone(),
            boxes: Vec::new(),
            kind: if s.kind == PlotKind::Bar {
                PlotKind::Bar
            } else {
                PlotKind::Scatter
            },
            ..s.clone()
        })
        .collect();
    Sideways { axes, data, x, y }
}

/// The upright scales of a sideways plot: group positions down the page (`cat`) and
/// values across it (`val`), matching the axes' scales `sx` (values) and `sy` (groups).
pub(super) fn upright_scales(side: &Sideways, sx: &Scale, sy: &Scale) -> (Scale, Scale) {
    let groups = side.data.categories.len() as f64;
    let cat = Scale::linear(groups - 1.0 - sy.lo, groups - 1.0 - sy.hi, sy.start, sy.end);
    (cat, *sx)
}

/// `values` on a sideways plot: each bar's number just past its end.
pub(super) fn draw_values_sideways(
    out: &mut String,
    data: &Panel,
    cat: &Scale,
    val: &Scale,
    plot: Rect,
    style: &Style,
) {
    let tfs = style.tick_font_size * 0.9;
    let cap = text::cap_height(style.font, tfs);
    let gap = 3.0;
    let fmt = data.values_format.as_ref();
    let mut labels = String::new();
    for s in data.series.iter().filter(|s| s.kind == PlotKind::Bar) {
        let dy = bar_offset(data, s);
        for (i, (&x, &y)) in s.x.iter().zip(&s.y).enumerate() {
            let base = s.base.as_ref().map_or(0.0, |b| b[i]);
            let py = cat.map(x + dy);
            let v = y - base;
            let err = s.yerr_at(i).filter(|(d, u)| d.is_finite() && u.is_finite());
            let end = val.map(err.map_or(y, |(d, u)| if v >= 0.0 { y + u } else { y - d }));
            if !(py.is_finite() && end.is_finite()) || py < plot.y - 1.0 || py > plot.bottom() + 1.0
            {
                continue;
            }
            let (px, anchor) = if data.stacked && s.base.is_some() {
                (val.map(base + v / 2.0), "middle")
            } else if v >= 0.0 {
                (end + gap, "start")
            } else {
                (end - gap, "end")
            };
            let _ = write!(
                labels,
                "\n<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"{anchor}\">{}</text>",
                n(px),
                n(py + cap / 2.0),
                n(tfs),
                escape(&mathtext::plain(&super::format_value_err(v, err, fmt)))
            );
        }
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
