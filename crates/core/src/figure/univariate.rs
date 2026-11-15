//! Plots of one variable's spread: histogram bins, kernel density curves (kde), and the
//! shapes behind box and violin plots.

use super::*;

/// Histogram series: count their values into shared bins (so they can be compared), and
/// stack them if asked. Afterwards `x` holds the bin edges and `y` the heights.
pub(super) fn histograms(
    spec: &PanelSpec,
    density: bool,
    series: &mut [Series],
) -> Result<(), Error> {
    let idx: Vec<usize> = (0..series.len())
        .filter(|&i| series[i].kind == PlotKind::Hist)
        .collect();
    if idx.is_empty() {
        return Ok(());
    }
    let ok = |v: &f64| v.is_finite() && (!spec.logx || *v > 0.0);
    let all: Vec<f64> = idx
        .iter()
        .flat_map(|&i| series[i].y.iter().copied().filter(ok))
        .collect();
    if all.is_empty() {
        return Err(Error::new("nothing to count in the histogram"));
    }
    let t = |v: f64| if spec.logx { v.log10() } else { v };
    let lo = all.iter().copied().map(t).fold(f64::INFINITY, f64::min);
    let hi = all.iter().copied().map(t).fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi) = if hi > lo {
        (lo, hi)
    } else {
        (lo - 0.5, hi + 0.5)
    };
    let bins = spec.bins.unwrap_or_else(|| {
        // Freedman–Diaconis: bin width from the spread of the middle half of the values.
        let mut sorted: Vec<f64> = all.iter().copied().map(t).collect();
        sorted.sort_by(f64::total_cmp);
        let q = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
        let iqr = q(0.75) - q(0.25);
        let n = sorted.len() as f64;
        let by_spread = if iqr > 0.0 {
            ((hi - lo) / (2.0 * iqr / n.cbrt())).ceil()
        } else {
            0.0
        };
        let sturges = n.log2().ceil() + 1.0;
        (by_spread.max(sturges) as usize).clamp(1, 100)
    });
    let width = (hi - lo) / bins as f64;
    let edges: Vec<f64> = (0..=bins)
        .map(|k| {
            let e = lo + width * k as f64;
            if spec.logx { 10f64.powf(e) } else { e }
        })
        .collect();
    let mut below = vec![0.0; bins];
    for &i in &idx {
        let s = &mut series[i];
        let mut counts = vec![0.0; bins];
        let mut n = 0.0;
        for v in s.y.iter().copied().filter(ok) {
            let k = (((t(v) - lo) / width).floor() as usize).min(bins - 1);
            counts[k] += 1.0;
            n += 1.0;
        }
        if density {
            for (k, c) in counts.iter_mut().enumerate() {
                *c /= n * (edges[k + 1] - edges[k]);
            }
        }
        if spec.stacked {
            let base = below.clone();
            for (b, c) in below.iter_mut().zip(&mut counts) {
                *c += *b;
                *b = *c;
            }
            s.base = Some(base);
        }
        s.x = edges.clone();
        s.y = counts;
    }
    Ok(())
}

/// Scott's rule: a smoothing width from the spread and the number of values.
pub(super) fn bandwidth(values: &[f64], bw: Option<f64>) -> Option<f64> {
    if let Some(b) = bw {
        return Some(b);
    }
    let n = values.len() as f64;
    if n < 2.0 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / n;
    let sd = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    (sd > 0.0).then(|| sd * n.powf(-0.2))
}

/// Gaussian density estimate at the points `at`.
pub(super) fn density(values: &[f64], h: f64, at: &[f64]) -> Vec<f64> {
    let k = 1.0 / (values.len() as f64 * h * (2.0 * std::f64::consts::PI).sqrt());
    at.iter()
        .map(|x| {
            k * values
                .iter()
                .map(|v| (-0.5 * ((x - v) / h).powi(2)).exp())
                .sum::<f64>()
        })
        .collect()
}

/// A kde curve: the density from three widths below the lowest value to three above the
/// highest. On a log x axis the curve is worked out in powers of ten.
pub(super) fn kde_curve(
    values: &[f64],
    bw: Option<f64>,
    log: bool,
) -> Option<(Vec<f64>, Vec<f64>)> {
    let t: Vec<f64> = values
        .iter()
        .copied()
        .filter(|v| v.is_finite() && (!log || *v > 0.0))
        .map(|v| if log { v.log10() } else { v })
        .collect();
    let h = bandwidth(&t, bw.filter(|_| !log))?;
    let lo = t.iter().copied().fold(f64::INFINITY, f64::min) - 3.0 * h;
    let hi = t.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 3.0 * h;
    let at: Vec<f64> = (0..=255)
        .map(|k| lo + (hi - lo) * k as f64 / 255.0)
        .collect();
    let d = density(&t, h, &at);
    if log {
        // Density per unit of x, not per decade.
        let xs: Vec<f64> = at.iter().map(|a| 10f64.powf(*a)).collect();
        let ys = d
            .iter()
            .zip(&xs)
            .map(|(d, x)| d / (x * std::f64::consts::LN_10))
            .collect();
        return Some((xs, ys));
    }
    Some((at, d))
}

/// Violins get the shape of their values' density, from the lowest to the highest value.
pub(super) fn with_shape(
    mut b: BoxStats,
    values: &[f64],
    kind: PlotKind,
    bw: Option<f64>,
) -> BoxStats {
    if kind != PlotKind::Violin {
        return b;
    }
    let v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if let Some(h) = bandwidth(&v, bw)
        && hi > lo
    {
        let at: Vec<f64> = (0..=80).map(|k| lo + (hi - lo) * k as f64 / 80.0).collect();
        b.shape = at.iter().copied().zip(density(&v, h, &at)).collect();
    }
    b
}
