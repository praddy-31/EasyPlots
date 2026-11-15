//! Reading numbers off a curve: its slope and running area (`slope`, `integral`), the area
//! between two x values (`area=2:8`), peaks and dips (`peaks`, `dips=3`) and the onset of a
//! step by the two-tangent construction (`onset`, `onset=20:40`), where curves cross
//! (`cross`, `cross=50`), baselines fitted over chosen ranges (`baseline=`), and normal
//! quantiles for Q–Q plots.
//!
//! Everything here works on plain x and y lists and knows nothing about drawing; `figure.rs`
//! turns the results into marks on the plot and lines for the Terminal.

/// The slope dy/dx at each row, from its neighbours in row order (central differences;
/// one-sided at the ends). Rows are taken in file order, so a heating-then-cooling scan
/// keeps its two halves apart. A gap, or two rows at the same x, gives a gap.
pub fn slope(xs: &[f64], ys: &[f64]) -> Vec<f64> {
    let n = xs.len().min(ys.len());
    let d = |a: usize, b: usize| {
        let dx = xs[b] - xs[a];
        let v = (ys[b] - ys[a]) / dx;
        if dx != 0.0 && v.is_finite() {
            v
        } else {
            f64::NAN
        }
    };
    (0..n)
        .map(|i| match (i, n) {
            (_, 0 | 1) => f64::NAN,
            (0, _) => d(0, 1),
            (i, n) if i == n - 1 => d(i - 1, i),
            (i, _) => d(i - 1, i + 1),
        })
        .collect()
}

/// The running area under the curve from the first row (trapezoids, in row order). Gaps
/// are stepped over: the total carries on from the last good row, and the gap row itself
/// stays a gap.
pub fn integral(xs: &[f64], ys: &[f64]) -> Vec<f64> {
    let mut total = 0.0;
    let mut last: Option<(f64, f64)> = None;
    xs.iter()
        .zip(ys)
        .map(|(&x, &y)| {
            if !(x.is_finite() && y.is_finite()) {
                return f64::NAN;
            }
            if let Some((px, py)) = last {
                total += (x - px) * (y + py) / 2.0;
            }
            last = Some((x, y));
            total
        })
        .collect()
}

/// The points with both values present, in order of x.
pub fn sorted(xs: &[f64], ys: &[f64]) -> Vec<(f64, f64)> {
    let mut p: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y))
        .collect();
    p.sort_by(|a, b| a.0.total_cmp(&b.0));
    p
}

/// y at `x` on the straight line between the points either side (`p` sorted by x).
fn at(p: &[(f64, f64)], x: f64) -> Option<f64> {
    let k = p.partition_point(|q| q.0 < x);
    match (k.checked_sub(1).and_then(|j| p.get(j)), p.get(k)) {
        (_, Some(b)) if b.0 == x => Some(b.1),
        (Some(a), Some(b)) => Some(a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0)),
        _ => None,
    }
}

/// The area under the curve (down to y = 0) from x = `a` to `b` by trapezoids, with the
/// ends read off the line between neighbouring points. Also returns the outline (the
/// curve inside the range) for shading. Err when the range misses the data.
pub fn area(xs: &[f64], ys: &[f64], range: (f64, f64)) -> Result<(f64, Vec<(f64, f64)>), String> {
    let p = sorted(xs, ys);
    let (Some(first), Some(last)) = (p.first(), p.last()) else {
        return Err("there are no points".into());
    };
    let (a, b) = (range.0.min(range.1), range.0.max(range.1));
    let (a, b) = (a.max(first.0), b.min(last.0));
    if a >= b {
        return Err(format!(
            "the range is outside the data (x runs from {} to {})",
            crate::data::short(first.0),
            crate::data::short(last.0)
        ));
    }
    let mut outline = vec![(a, at(&p, a).unwrap_or(f64::NAN))];
    outline.extend(p.iter().filter(|q| q.0 > a && q.0 < b));
    outline.push((b, at(&p, b).unwrap_or(f64::NAN)));
    let total = outline
        .windows(2)
        .map(|w| (w[1].0 - w[0].0) * (w[1].1 + w[0].1) / 2.0)
        .sum();
    Ok((total, outline))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Peak {
    pub x: f64,
    pub y: f64,
    /// How far the peak stands above the higher of the two lowest points between it and
    /// the next higher ground on each side (the usual "prominence").
    pub prominence: f64,
}

/// Up to `n` peaks (or dips, the low points, with `dips`), most prominent first. A peak
/// must be inside the data (not an end point) and stand out by at least 5 % of the curve's
/// height range, so noise is not counted; smooth noisy data first (`smooth=`).
pub fn peaks(xs: &[f64], ys: &[f64], n: usize, dips: bool) -> Vec<Peak> {
    let p = sorted(xs, ys);
    let ys: Vec<f64> = p.iter().map(|q| if dips { -q.1 } else { q.1 }).collect();
    if ys.len() < 3 {
        return Vec::new();
    }
    let left = bases(ys.iter().copied());
    let mut right = bases(ys.iter().rev().copied());
    right.reverse();
    let (lo, hi) = ys
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &y| {
            (a.min(y), b.max(y))
        });
    let least = 0.05 * (hi - lo);
    let mut found: Vec<Peak> = (1..ys.len() - 1)
        .filter(|&i| ys[i] > ys[i - 1] && ys[i] >= ys[i + 1])
        // A flat top counts once: its first point, if the ground falls after the flat.
        .filter(|&i| {
            let end = (i..ys.len()).find(|&j| ys[j] != ys[i]);
            end.is_some_and(|j| ys[j] < ys[i])
        })
        .map(|i| Peak {
            x: p[i].0,
            y: p[i].1,
            prominence: ys[i] - left[i].max(right[i]),
        })
        .filter(|pk| pk.prominence > 0.0 && pk.prominence >= least)
        .collect();
    found.sort_by(|a, b| b.prominence.total_cmp(&a.prominence));
    found.truncate(n);
    found
}

/// For each point, the lowest value between it and the nearest higher point before it
/// (or the start). One pass with a stack: each stack entry keeps the lowest value of the
/// stretch it covers, so stretches merge as lower points are popped.
fn bases(ys: impl Iterator<Item = f64>) -> Vec<f64> {
    let mut stack: Vec<(f64, f64)> = Vec::new();
    ys.map(|y| {
        let mut low = y;
        while let Some(&(top, top_low)) = stack.last() {
            if top > y {
                break;
            }
            low = low.min(top_low);
            stack.pop();
        }
        stack.push((y, low));
        low
    })
    .collect()
}

/// A straight line `y = slope * x + at0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub slope: f64,
    pub at0: f64,
}

impl Line {
    pub fn y(&self, x: f64) -> f64 {
        self.slope * x + self.at0
    }

    /// The least-squares line through the points.
    fn through(p: &[(f64, f64)]) -> Option<Line> {
        let n = p.len() as f64;
        if p.len() < 2 {
            return None;
        }
        let mx = p.iter().map(|q| q.0).sum::<f64>() / n;
        let my = p.iter().map(|q| q.1).sum::<f64>() / n;
        let sxx: f64 = p.iter().map(|q| (q.0 - mx).powi(2)).sum();
        let sxy: f64 = p.iter().map(|q| (q.0 - mx) * (q.1 - my)).sum();
        (sxx > 0.0).then(|| {
            let slope = sxy / sxx;
            Line {
                slope,
                at0: my - slope * mx,
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Onset {
    pub x: f64,
    pub y: f64,
    /// The flat stretch before the step, extended to the onset.
    pub baseline: Line,
    /// The x range the baseline was fitted on.
    pub base_range: (f64, f64),
    /// The tangent at the steepest point of the step.
    pub tangent: Line,
    /// Where the curve is steepest.
    pub steepest: (f64, f64),
}

/// The onset of a step (a glass transition, a melting peak's start, a modulus drop): where
/// the baseline before the step crosses the tangent at its steepest point. The baseline is
/// a straight line fitted to `base` (an x range) or, without one, to the first third of
/// the way from the first point to the steepest point.
pub fn onset(xs: &[f64], ys: &[f64], base: Option<(f64, f64)>) -> Result<Onset, String> {
    let p = sorted(xs, ys);
    if p.len() < 8 {
        return Err("needs at least 8 points".into());
    }
    // The steepest point, on a lightly smoothed slope so one noisy pair can't win.
    let px: Vec<f64> = p.iter().map(|q| q.0).collect();
    let py: Vec<f64> = p.iter().map(|q| q.1).collect();
    let w = (p.len() / 50).max(2);
    let raw = slope(&px, &py);
    let smooth = crate::expr::moving_average(&raw, 2 * w + 1);
    // Only up to the point farthest from where the curve starts: for a step that is the
    // whole curve, for a peak its rising side (the onset is where the peak begins).
    let far = py
        .iter()
        .enumerate()
        .max_by(|a, b| (a.1 - py[0]).abs().total_cmp(&(b.1 - py[0]).abs()))
        .map_or(p.len() - 1, |(i, _)| i);
    let (coarse, _) = smooth
        .iter()
        .enumerate()
        .take(far + 1)
        .filter(|(_, s)| s.is_finite())
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .ok_or("the curve has no slope to measure")?;
    // The wide smoothing finds the right stretch but blurs a narrow peak's flank (a 3 °C
    // peak in a 300 °C scan), so look again near it on a barely smoothed slope.
    let fine = crate::expr::moving_average(&raw, 5);
    let lo = coarse.saturating_sub(w);
    let hi = (coarse + w).min(far).max(lo);
    let k = (lo..=hi)
        .filter(|&i| fine[i].is_finite())
        .max_by(|&a, &b| fine[a].abs().total_cmp(&fine[b].abs()))
        .unwrap_or(coarse);
    // The tangent: a straight line through the steepest stretch only, where the slope is
    // within 3 % of its steepest (at least 2 points each side, at most `w`).
    let top = fine[k].abs();
    let steep = |i: usize| fine[i].is_finite() && fine[i].abs() >= 0.97 * top;
    let mut a = k;
    while a > 0 && k - a < w && (k - a < 2 || steep(a - 1)) {
        a -= 1;
    }
    let mut b = k;
    while b + 1 < p.len() && b - k < w && (b - k < 2 || steep(b + 1)) {
        b += 1;
    }
    let around = &p[a..=b];
    let tangent = Line::through(around).ok_or("the steepest part is a single x value")?;
    let base_range = match base {
        Some((a, b)) => (a.min(b), a.max(b)),
        None => (p[0].0, p[0].0 + (p[k].0 - p[0].0) / 3.0),
    };
    let flat: Vec<(f64, f64)> = p
        .iter()
        .copied()
        .filter(|q| q.0 >= base_range.0 && q.0 <= base_range.1)
        .collect();
    if flat.len() < 3 {
        return Err(if base.is_some() {
            "fewer than 3 points in the baseline range".into()
        } else {
            "the step starts too close to the first point to find a baseline; give its x range, \
             e.g. onset=20:40"
                .into()
        });
    }
    let baseline = Line::through(&flat).ok_or("the baseline range is a single x value")?;
    let turn = tangent.slope - baseline.slope;
    if turn.abs() < 1e-12 * (tangent.slope.abs() + baseline.slope.abs()).max(1e-300) {
        return Err("the baseline and the tangent are parallel".into());
    }
    let x = (baseline.at0 - tangent.at0) / turn;
    if !x.is_finite() {
        return Err("the baseline and the tangent don't cross".into());
    }
    Ok(Onset {
        x,
        y: baseline.y(x),
        baseline,
        base_range,
        tangent,
        steepest: p[k],
    })
}

/// Where a curve crosses the level `y = level`, in row order (lines are drawn in row
/// order, so a heating-then-cooling scan gives both crossings). Between two rows the
/// crossing is read off the straight line joining them; a row exactly on the level counts
/// once.
pub fn level_crossings(xs: &[f64], ys: &[f64], level: f64) -> Vec<(f64, f64)> {
    let pts: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y - level))
        .collect();
    let mut out = Vec::new();
    // The last row off the level, and where a run of rows exactly on it began.
    let mut last: Option<(f64, f64)> = None;
    let mut on_level: Option<f64> = None;
    for &(x, d) in &pts {
        if d == 0.0 {
            on_level.get_or_insert(x);
            continue;
        }
        if let Some((px, pd)) = last
            && pd * d < 0.0
        {
            out.push((on_level.unwrap_or(px + (x - px) * pd / (pd - d)), level));
        }
        last = Some((x, d));
        on_level = None;
    }
    out
}

/// Where two curves cross, over the x range they share: both are read off their straight
/// lines at every x either has, and a change of sign of their difference is a crossing.
pub fn curve_crossings(ax: &[f64], ay: &[f64], bx: &[f64], by: &[f64]) -> Vec<(f64, f64)> {
    let (a, b) = (sorted(ax, ay), sorted(bx, by));
    let (Some(a0), Some(b0), Some(a1), Some(b1)) = (a.first(), b.first(), a.last(), b.last())
    else {
        return Vec::new();
    };
    let (lo, hi) = (a0.0.max(b0.0), a1.0.min(b1.0));
    if lo >= hi {
        return Vec::new();
    }
    let mut grid: Vec<f64> = a
        .iter()
        .chain(&b)
        .map(|p| p.0)
        .filter(|x| *x >= lo && *x <= hi)
        .collect();
    grid.sort_by(f64::total_cmp);
    grid.dedup();
    let d: Vec<(f64, f64, f64)> = grid
        .iter()
        .filter_map(|&x| Some((x, at(&a, x)?, at(&b, x)?)))
        .collect();
    let mut out = Vec::new();
    let mut last_side = 0.0;
    for (k, &(x, ya, yb)) in d.iter().enumerate() {
        let diff = ya - yb;
        if diff == 0.0 {
            // Touching: a crossing if the sides before and after differ.
            let next = d[k + 1..].iter().map(|p| p.1 - p.2).find(|v| *v != 0.0);
            if last_side != 0.0 && next.is_some_and(|n| n * last_side < 0.0) {
                out.push((x, ya));
            }
            continue;
        }
        if k > 0 {
            let (px, pa, pb) = d[k - 1];
            let pd = pa - pb;
            if pd * diff < 0.0 {
                let t = pd / (pd - diff);
                out.push((px + (x - px) * t, pa + (ya - pa) * t));
            }
        }
        last_side = diff.signum();
    }
    out
}

/// A polynomial of `degree` (0: a constant) fitted by least squares to the points whose x
/// is inside any of `ranges` (none: the first and last tenth of the x range). Returns the
/// coefficients from the constant up, and the ranges used.
#[allow(clippy::type_complexity)]
pub fn baseline(
    xs: &[f64],
    ys: &[f64],
    degree: usize,
    ranges: &[(f64, f64)],
) -> Result<(Vec<f64>, Vec<(f64, f64)>), String> {
    let finite: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys)
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y))
        .collect();
    let (lo, hi) = finite
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p.0), b.max(p.0))
        });
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater) {
        return Err("needs at least two different x values".into());
    }
    let ranges: Vec<(f64, f64)> = if ranges.is_empty() {
        let tenth = (hi - lo) / 10.0;
        vec![(lo, lo + tenth), (hi - tenth, hi)]
    } else {
        ranges.to_vec()
    };
    let (px, py): (Vec<f64>, Vec<f64>) = finite
        .iter()
        .filter(|(x, _)| ranges.iter().any(|(a, b)| x >= a && x <= b))
        .copied()
        .unzip();
    let show = |r: &[(f64, f64)]| {
        r.iter()
            .map(|(a, b)| format!("{}:{}", crate::data::short(*a), crate::data::short(*b)))
            .collect::<Vec<_>>()
            .join(",")
    };
    if px.len() < degree + 1 {
        return Err(format!(
            "only {} point{} inside {} — a {} baseline needs at least {}",
            px.len(),
            if px.len() == 1 { "" } else { "s" },
            show(&ranges),
            ["constant", "straight", "poly2", "poly3"][degree.min(3)],
            degree + 1
        ));
    }
    let coef = if degree == 0 {
        vec![py.iter().sum::<f64>() / py.len() as f64]
    } else {
        crate::fit::polyfit(&px, &py, &vec![1.0; px.len()], degree).ok_or_else(|| {
            format!(
                "the points inside {} don't pin down a {} baseline (spread the ranges out)",
                show(&ranges),
                ["", "straight", "poly2", "poly3"][degree.min(3)]
            )
        })?
    };
    Ok((coef, ranges))
}

/// A polynomial's value (coefficients from the constant up).
pub fn poly(coef: &[f64], x: f64) -> f64 {
    coef.iter().rev().fold(0.0, |acc, c| acc * x + c)
}

/// The standard normal quantile: the z below which a fraction `p` of a normal
/// distribution lies (Acklam's rational approximation, accurate to about 1e-9).
#[allow(clippy::excessive_precision)] // the published constants, kept as printed
pub fn normal_quantile(p: f64) -> f64 {
    if !(p > 0.0 && p < 1.0) {
        return f64::NAN;
    }
    const A: [f64; 6] = [
        -3.969683028665376e1,
        2.209460984245205e2,
        -2.759285104469687e2,
        1.383577518672690e2,
        -3.066479806614716e1,
        2.506628277459239,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e1,
        1.615858368580409e2,
        -1.556989798598866e2,
        6.680131188771972e1,
        -1.328068155288572e1,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-3,
        -3.223964580411365e-1,
        -2.400758277161838,
        -2.549732539343734,
        4.374664141464968,
        2.938163982698783,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-3,
        3.224671290700398e-1,
        2.445134137142996,
        3.754408661907416,
    ];
    let tail = |q: f64| {
        let q = (-2.0 * q.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    if p < 0.02425 {
        tail(p)
    } else if p > 1.0 - 0.02425 {
        -tail(1.0 - p)
    } else {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    }
}

/// A normal Q–Q plot's points: the values sorted, each against the normal quantile of its
/// rank (Blom's positions, (i − 3/8)/(n + 1/4)). Also the reference line through the
/// first and third quartiles of both, as (intercept, slope).
#[allow(clippy::type_complexity)]
pub fn qq(values: &[f64]) -> Option<(Vec<f64>, Vec<f64>, (f64, f64))> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < 3 {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len() as f64;
    let z: Vec<f64> = (0..v.len())
        .map(|i| normal_quantile((i as f64 + 1.0 - 0.375) / (n + 0.25)))
        .collect();
    // Both read at the same rank (a quarter and three quarters of the way along), so
    // values that are exactly normal lie on the line even for a few points.
    let at = |list: &[f64], p: f64| {
        let at = p * (n - 1.0);
        let (i, f) = (at.floor() as usize, at.fract());
        if i + 1 < list.len() {
            list[i] + (list[i + 1] - list[i]) * f
        } else {
            list[i]
        }
    };
    let (z1, z3) = (at(&z, 0.25), at(&z, 0.75));
    let slope = (at(&v, 0.75) - at(&v, 0.25)) / (z3 - z1);
    let intercept = at(&v, 0.25) - slope * z1;
    Some((z, v, (intercept, slope)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn slope_of_a_parabola() {
        let xs: Vec<f64> = (0..=10).map(f64::from).collect();
        let ys: Vec<f64> = xs.iter().map(|x| x * x).collect();
        let s = slope(&xs, &ys);
        // Central differences are exact for a parabola inside; one-sided at the ends.
        assert_eq!(s[5], 10.0);
        assert_eq!(s[0], 1.0);
        assert_eq!(s[10], 19.0);
        assert!(slope(&[1.0], &[2.0])[0].is_nan());
        assert!(slope(&[1.0, 1.0, 2.0], &[0.0, 1.0, 2.0])[0].is_nan());
    }

    #[test]
    fn running_area_and_area_between() {
        let xs: Vec<f64> = (0..=4).map(f64::from).collect();
        let ys = vec![2.0; 5];
        assert_eq!(integral(&xs, &ys), vec![0.0, 2.0, 4.0, 6.0, 8.0]);
        let gap = integral(&xs, &[2.0, f64::NAN, 2.0, 2.0, 2.0]);
        assert!(gap[1].is_nan());
        assert_eq!(gap[2], 4.0);
        // y = x from 1 to 3 (ends between points): (9 - 1) / 2 = 4.
        let (a, outline) = area(&xs, &xs, (1.5, 3.5)).unwrap();
        assert!(close(a, (3.5f64.powi(2) - 1.5f64.powi(2)) / 2.0, 1e-12));
        assert_eq!(outline.first(), Some(&(1.5, 1.5)));
        assert_eq!(outline.last(), Some(&(3.5, 3.5)));
        // Clipped to the data, and out of range is an error.
        assert_eq!(area(&xs, &xs, (-5.0, 9.0)).unwrap().0, 8.0);
        assert!(area(&xs, &xs, (6.0, 9.0)).is_err());
    }

    #[test]
    fn peaks_by_prominence() {
        // Two bumps on a slope plus small noise wiggles that must not count.
        let xs: Vec<f64> = (0..400).map(|i| i as f64 * 0.05).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| {
                3.0 * (-(x - 5.0f64).powi(2)).exp()
                    + 1.5 * (-(x - 14.0f64).powi(2) / 2.0).exp()
                    + 0.05 * x
                    + 0.01 * (x * 40.0).sin()
            })
            .collect();
        let found = peaks(&xs, &ys, 5, false);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(close(found[0].x, 5.0, 0.1));
        assert!(close(found[1].x, 14.0, 0.15));
        assert_eq!(peaks(&xs, &ys, 1, false).len(), 1);
        // The dip between them.
        let d = peaks(&xs, &ys, 1, true);
        assert_eq!(d.len(), 1);
        assert!(d[0].x > 7.0 && d[0].x < 12.0, "{d:?}");
        // A curve that only rises has no peak (its top is an end point).
        assert!(peaks(&xs, &xs, 1, false).is_empty());
    }

    #[test]
    fn flat_top_counts_once() {
        let xs = [0.0, 1.0, 2.0, 3.0, 4.0];
        let ys = [0.0, 2.0, 2.0, 2.0, 0.0];
        let p = peaks(&xs, &ys, 3, false);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].x, 1.0);
    }

    #[test]
    fn onset_of_a_step_down() {
        // A modulus-like curve: flat at 1000 with a slight slope, dropping around x = 60.
        let xs: Vec<f64> = (0..=200).map(|i| i as f64 * 0.5).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 1000.0 - 0.5 * x - 900.0 / (1.0 + (-(x - 60.0) / 3.0).exp()))
            .collect();
        let o = onset(&xs, &ys, None).unwrap();
        // The logistic's tangent at 60 has slope -75 - 0.5; it meets the baseline at about
        // 60 - 450/75 = 54.
        assert!(close(o.steepest.0, 60.0, 0.6), "{o:?}");
        assert!(close(o.x, 54.0, 0.5), "{o:?}");
        assert!(close(o.baseline.slope, -0.5, 0.05), "{o:?}");
        // With the baseline range given.
        let given = onset(&xs, &ys, Some((10.0, 40.0))).unwrap();
        assert!(close(given.x, o.x, 0.5));
        assert!(onset(&xs, &ys, Some((10.0, 10.2))).is_err());
        assert!(onset(&xs[..5], &ys[..5], None).is_err());
    }

    #[test]
    fn onset_of_a_peak_is_on_its_rising_side() {
        // A melting-like peak at 100 (width 8): steepest rise at 100 - 8/sqrt(2) ≈ 94.3.
        let xs: Vec<f64> = (0..=140).map(|i| 20.0 + i as f64).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 10.0 + 260.0 * (-((x - 100.0) / 8.0f64).powi(2)).exp())
            .collect();
        let o = onset(&xs, &ys, None).unwrap();
        assert!(o.steepest.0 < 100.0, "{o:?}");
        // The exact two-tangent onset of a Gaussian: 100 - 8·√2 ≈ 88.7.
        assert!(close(o.x, 100.0 - 8.0 * 2f64.sqrt(), 0.4), "{o:?}");
    }

    #[test]
    fn onset_of_a_narrow_peak_in_a_long_scan() {
        // A DSC-like peak 3 °C wide in a 0–300 °C scan of 3001 points: the old tangent,
        // fitted over 4 % of all points (12 °C), gave 151 instead of 155.8.
        let xs: Vec<f64> = (0..=3000).map(|i| i as f64 * 0.1).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 5.0 * (-((x - 160.0) / 3.0f64).powi(2)).exp())
            .collect();
        let o = onset(&xs, &ys, Some((100.0, 140.0))).unwrap();
        assert!(close(o.x, 160.0 - 3.0 * 2f64.sqrt(), 0.15), "{o:?}");
        // A step with a known onset (100 - 2·4 = 92).
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| 10.0 / (1.0 + (-(x - 100.0) / 4.0).exp()))
            .collect();
        let o = onset(&xs, &ys, Some((0.0, 60.0))).unwrap();
        assert!(close(o.x, 92.0, 0.15), "{o:?}");
    }

    #[test]
    fn onset_survives_noise() {
        // The same peak with noise (a fixed pseudo-random sequence): still near 155.8.
        let mut seed = 7u64;
        let xs: Vec<f64> = (0..=3000).map(|i| i as f64 * 0.1).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let noise = ((seed >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 0.05;
                5.0 * (-((x - 160.0) / 3.0f64).powi(2)).exp() + noise
            })
            .collect();
        let o = onset(&xs, &ys, Some((100.0, 140.0))).unwrap();
        assert!(close(o.x, 160.0 - 3.0 * 2f64.sqrt(), 0.6), "{o:?}");
    }

    #[test]
    fn crossings() {
        // A rise then a fall through 5: crossed twice, in row order.
        let x = [0.0, 1.0, 2.0, 3.0, 4.0];
        let y = [0.0, 10.0, 10.0, 0.0, 0.0];
        let c = level_crossings(&x, &y, 5.0);
        assert_eq!(c.len(), 2);
        assert!(close(c[0].0, 0.5, 1e-12) && close(c[1].0, 2.5, 1e-12));
        // Touching the level without going through isn't a crossing.
        assert!(level_crossings(&x, &[0.0, 5.0, 0.0, 0.0, 0.0], 5.0).is_empty());
        // Resting on it then going through counts once, where it got there.
        let c = level_crossings(&x, &[0.0, 5.0, 5.0, 9.0, 9.0], 5.0);
        assert_eq!(c, vec![(1.0, 5.0)]);
        // y = x and y = 4 − x cross at (2, 2), also when the x values differ.
        let c = curve_crossings(&x, &x, &[0.5, 3.5], &[3.5, 0.5]);
        assert_eq!(c.len(), 1);
        assert!(close(c[0].0, 2.0, 1e-12) && close(c[0].1, 2.0, 1e-12));
        // Curves that don't overlap in x don't cross.
        assert!(curve_crossings(&[0.0, 1.0], &[0.0, 1.0], &[2.0, 3.0], &[1.0, 0.0]).is_empty());
    }

    #[test]
    fn baselines() {
        // y = 3 + 0.5x + 0.01x², with a bump in the middle that the ranges leave out.
        let x: Vec<f64> = (0..=100).map(f64::from).collect();
        let y: Vec<f64> = x
            .iter()
            .map(|x| 3.0 + 0.5 * x + 0.01 * x * x + 50.0 * (-((x - 50.0) / 5.0).powi(2)).exp())
            .collect();
        let (c, used) = baseline(&x, &y, 2, &[(0.0, 20.0), (80.0, 100.0)]).unwrap();
        assert!(close(c[0], 3.0, 1e-6) && close(c[1], 0.5, 1e-6) && close(c[2], 0.01, 1e-8));
        assert_eq!(used.len(), 2);
        // No ranges: the first and last tenth.
        let (_, used) = baseline(&x, &y, 1, &[]).unwrap();
        assert_eq!(used, vec![(0.0, 10.0), (90.0, 100.0)]);
        // A constant is the mean of the points in range.
        let (c, _) = baseline(&x, &x, 0, &[(0.0, 2.0)]).unwrap();
        assert!(close(c[0], 1.0, 1e-12));
        assert!(
            baseline(&x, &y, 1, &[(200.0, 300.0)])
                .unwrap_err()
                .contains("only 0 points")
        );
        assert!(close(poly(&[1.0, 2.0, 3.0], 2.0), 17.0, 1e-12));
    }

    #[test]
    fn normal_quantiles_and_qq() {
        assert!(close(normal_quantile(0.5), 0.0, 1e-9));
        assert!(close(normal_quantile(0.975), 1.959964, 1e-6));
        assert!(close(normal_quantile(0.01), -2.326348, 1e-6));
        assert!(normal_quantile(1.0).is_nan());
        // Values that are exactly normal quantiles lie on the reference line y = 2 + 3z.
        let n = 9.0;
        let v: Vec<f64> = (0..9)
            .map(|i| 2.0 + 3.0 * normal_quantile((f64::from(i) + 1.0 - 0.375) / (n + 0.25)))
            .rev()
            .collect();
        let (z, sorted, (a, b)) = qq(&v).unwrap();
        assert!(sorted.windows(2).all(|w| w[0] <= w[1]));
        for (z, s) in z.iter().zip(&sorted) {
            assert!(close(*s, 2.0 + 3.0 * z, 1e-9));
        }
        assert!(close(a, 2.0, 1e-6) && close(b, 3.0, 1e-6), "{a} {b}");
        assert!(qq(&[1.0, 2.0]).is_none());
    }
}
