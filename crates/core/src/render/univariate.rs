//! Drawing one variable's spread: histograms, boxes and violins (strips are in `groups`).

use super::*;

/// A histogram: touching bars between the bin edges. Several unstacked histograms are
/// see-through so they can overlap.
pub(super) fn draw_hist(
    out: &mut String,
    s: &Series,
    sx: &Scale,
    sy: &Scale,
    overlap: bool,
    style: &Style,
) {
    let mut d = String::new();
    for (k, h) in s.y.iter().enumerate() {
        let (x0, x1) = (sx.map(s.x[k]), sx.map(s.x[k + 1]));
        let y0 = s
            .base
            .as_ref()
            .map_or_else(|| sy.baseline(), |b| sy.map_or_floor(b[k]));
        let y1 = sy.map(*h);
        if *h == 0.0 || !(x0.is_finite() && x1.is_finite() && y0.is_finite() && y1.is_finite()) {
            continue;
        }
        let _ = write!(d, "M{} {}H{}V{}H{}Z", n(x0), n(y0), n(x1), n(y1), n(x0));
    }
    let alpha = if overlap { fill_alpha(s, 0.5) } else { s.alpha };
    // A thin line in the background colour keeps neighbouring bars apart.
    let _ = write!(
        out,
        "\n<path d=\"{d}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.4\"{}/>",
        s.color.hex(),
        if overlap {
            s.color.hex()
        } else {
            style.background.hex()
        },
        opacity("fill", alpha)
    );
}

/// Box plots: the box from the lower to the upper quartile with the median across it,
/// whiskers, and outliers as open circles.
pub(super) fn draw_boxes(
    out: &mut String,
    s: &Series,
    sx: &Scale,
    sy: &Scale,
    width: f64,
    shift: f64,
    style: &Style,
) {
    let c = s.color.hex();
    let mut boxes = String::new();
    let mut lines = String::new();
    let mut medians = String::new();
    let mut dots = String::new();
    for b in &s.boxes {
        let mid = b.pos + shift;
        let (x0, xm, x1) = (
            sx.map(mid - width / 2.0),
            sx.map(mid),
            sx.map(mid + width / 2.0),
        );
        let (q1, q3, med) = (sy.map_or_floor(b.q1), sy.map(b.q3), sy.map(b.median));
        let (lo, hi) = (sy.map_or_floor(b.low), sy.map(b.high));
        if ![x0, x1, q1, q3, med, lo, hi].iter().all(|v| v.is_finite()) {
            continue;
        }
        let _ = write!(boxes, "M{} {}H{}V{}H{}Z", n(x0), n(q1), n(x1), n(q3), n(x0));
        let cap = (x1 - x0) * 0.25;
        let _ = write!(
            lines,
            "M{xm} {q3}V{hi}M{} {hi}H{}M{xm} {q1}V{lo}M{} {lo}H{}",
            n(xm - cap),
            n(xm + cap),
            n(xm - cap),
            n(xm + cap),
            xm = n(xm),
            q3 = n(q3),
            hi = n(hi),
            q1 = n(q1),
            lo = n(lo)
        );
        let _ = write!(medians, "M{} {}H{}", n(x0), n(med), n(x1));
        let r = style.marker_size * 0.35;
        for o in &b.outliers {
            let py = sy.map(*o);
            if py.is_finite() {
                let _ = write!(
                    dots,
                    "M{} {}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 {} 0Z",
                    n(xm - r),
                    n(py),
                    n(2.0 * r),
                    n(-2.0 * r),
                    r = n(r)
                );
            }
        }
    }
    let w = (s.width * 0.8).max(0.6);
    let _ = write!(
        out,
        "\n<path d=\"{boxes}\" fill=\"{c}\" fill-opacity=\"{}\" stroke=\"{c}\" stroke-width=\"{}\"/>",
        n(fill_alpha(s, 0.25)),
        n(w)
    );
    let _ = write!(
        out,
        "\n<path d=\"{lines}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"{}\"/>",
        n(w)
    );
    let _ = write!(
        out,
        "\n<path d=\"{medians}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>",
        // On a (nearly) solid box a median in the box's own colour would vanish.
        if fill_alpha(s, 0.25) > 0.6 {
            style.background.hex()
        } else {
            c.clone()
        },
        n(w * 2.0)
    );
    if !dots.is_empty() {
        let _ = write!(
            out,
            "\n<path d=\"{dots}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"{}\"/>",
            n(w * 0.8)
        );
    }
}

/// Violins: the density's shape mirrored around the group's position, with the middle
/// half of the values as a thick bar and the median as a white dot.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_violins(
    out: &mut String,
    s: &Series,
    sx: &Scale,
    sy: &Scale,
    width: f64,
    shift: f64,
    widest: f64,
    style: &Style,
) {
    let c = s.color.hex();
    let mut shapes = String::new();
    let mut bars = String::new();
    let mut dots = String::new();
    for b in &s.boxes {
        let mid = b.pos + shift;
        let half = |d: f64| {
            if widest > 0.0 {
                d / widest * width / 2.0
            } else {
                0.0
            }
        };
        if b.shape.len() >= 2 {
            for (k, (v, d)) in b.shape.iter().enumerate() {
                let _ = write!(
                    shapes,
                    "{}{} {}",
                    if k == 0 { 'M' } else { 'L' },
                    n(sx.map(mid + half(*d))),
                    n(sy.map(*v))
                );
            }
            for (v, d) in b.shape.iter().rev() {
                let _ = write!(shapes, "L{} {}", n(sx.map(mid - half(*d))), n(sy.map(*v)));
            }
            shapes.push('Z');
        }
        let xm = sx.map(mid);
        let _ = write!(
            bars,
            "M{xm} {}V{}",
            n(sy.map(b.q1)),
            n(sy.map(b.q3)),
            xm = n(xm)
        );
        let r = style.marker_size * 0.3;
        let py = sy.map(b.median);
        let _ = write!(
            dots,
            "M{} {}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 {} 0Z",
            n(xm - r),
            n(py),
            n(2.0 * r),
            n(-2.0 * r),
            r = n(r)
        );
    }
    let w = (s.width * 0.8).max(0.6);
    let _ = write!(
        out,
        "\n<path d=\"{shapes}\" fill=\"{c}\" fill-opacity=\"{}\" stroke=\"{c}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
        n(fill_alpha(s, 0.3)),
        n(w)
    );
    // On a (nearly) solid violin the inner bar and its dot swap colours to stay visible.
    let solid = fill_alpha(s, 0.3) > 0.6;
    let _ = write!(
        out,
        "\n<path d=\"{bars}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
        if solid {
            style.background.hex()
        } else {
            c.clone()
        },
        n(style.marker_size * 0.9)
    );
    let _ = write!(
        out,
        "\n<path d=\"{dots}\" fill=\"{}\" stroke=\"none\"/>",
        if solid {
            c.clone()
        } else {
            style.background.hex()
        }
    );
}
