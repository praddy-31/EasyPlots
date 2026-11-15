//! Fitted curves: `fit=linear|poly2…|exp|log|power`, or your own formula
//! (`fit="a*exp(-x/b)+c"`). Least squares throughout, with the equation (or the
//! parameters) and R² (how much of the spread the curve explains; 1 = perfect).
//!
//! Polynomials and `log` are linear in their parameters and solved directly. `exp`,
//! `power` and formulas are solved by Levenberg–Marquardt: start from a guess, then
//! repeatedly take the step that best reduces the squared misses, more cautiously when a
//! step doesn't help. `exp` and `power` start from the straight-line fit of the logarithms
//! (the quick method), so the result is the true least-squares curve; on a log y axis
//! they stay fitted to the logarithms, which is what looks straight there. Each parameter gets
//! a standard error from the curve's sensitivity to it (the covariance matrix), which also
//! gives the 95% confidence band (`fitband`). With `weighted`, points count by
//! 1/error² (from `err=`).

use crate::expr::{self, Expr};
use crate::spec::Fit;

/// What is fitted.
#[derive(Debug, Clone, PartialEq)]
pub enum Model {
    Builtin(Fit),
    /// Your own formula of `x`; every other name in it is a parameter to fit, in order of
    /// first appearance (`pi` and `e` are the constants).
    Formula {
        text: String,
        expr: Expr,
        names: Vec<String>,
    },
}

impl Model {
    /// A formula model from `a*exp(-x/b)+c`.
    pub fn formula(text: &str) -> Result<Model, String> {
        let expr = expr::parse(text)?;
        let mut names: Vec<String> = Vec::new();
        let mut has_x = false;
        let mut bad: Option<&'static str> = None;
        walk(&expr, &mut |e| match e {
            Expr::Column(n) if n.eq_ignore_ascii_case("x") => has_x = true,
            Expr::Column(n) if n == "pi" || n == "e" => {}
            Expr::Column(n) => {
                if !names.contains(n) {
                    names.push(n.clone());
                }
            }
            Expr::Call(f @ ("cumsum" | "diff" | "norm"), _) => bad = Some(f),
            Expr::Smooth(..) => bad = Some("smooth"),
            _ => {}
        });
        if let Some(f) = bad {
            return Err(format!(
                "{f}() works on whole columns, not in a fit formula"
            ));
        }
        if !has_x {
            return Err("a fit formula is written with x, e.g. fit=\"a*exp(-x/b)+c\"".into());
        }
        if names.is_empty() {
            return Err(
                "a fit formula needs something to fit: letters besides x, e.g. fit=\"a*x^b\""
                    .into(),
            );
        }
        if names.len() > 10 {
            return Err("a fit formula can have at most 10 parameters".into());
        }
        Ok(Model::Formula {
            text: text.trim().to_owned(),
            expr,
            names,
        })
    }

    fn value(&self, c: &[f64], x: f64) -> f64 {
        match self {
            Model::Builtin(Fit::Poly(_)) => c.iter().rev().fold(0.0, |acc, k| acc * x + k),
            Model::Builtin(Fit::Exp) => c[0] * (c[1] * x).exp(),
            Model::Builtin(Fit::Log) => c[0] + c[1] * x.ln(),
            Model::Builtin(Fit::Power) => c[0] * x.powf(c[1]),
            Model::Formula { expr, names, .. } => point(expr, x, names, c),
        }
    }

    /// Parameter names, for the Terminal report.
    fn names(&self, count: usize) -> Vec<String> {
        match self {
            Model::Builtin(Fit::Poly(1)) => vec!["intercept".into(), "slope".into()],
            Model::Builtin(Fit::Poly(_)) => (0..count)
                .map(|k| match k {
                    0 => "constant".to_owned(),
                    1 => "x".to_owned(),
                    k => format!("x^{k}"),
                })
                .collect(),
            Model::Builtin(_) => vec!["a".into(), "b".into()],
            Model::Formula { names, .. } => names.clone(),
        }
    }
}

/// Visit every part of a formula.
fn walk<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(e);
    match e {
        Expr::Neg(a) | Expr::Call(_, a) | Expr::Smooth(a, _) => walk(a, f),
        Expr::Bin(_, a, b) => {
            walk(a, f);
            walk(b, f);
        }
        Expr::Num(_) | Expr::Column(_) => {}
    }
}

/// A formula's value at one x with these parameter values.
fn point(e: &Expr, x: f64, names: &[String], p: &[f64]) -> f64 {
    match e {
        Expr::Num(v) => *v,
        Expr::Column(n) if n.eq_ignore_ascii_case("x") => x,
        Expr::Column(n) => match names.iter().position(|m| m == n) {
            Some(i) => p[i],
            None if n == "pi" => std::f64::consts::PI,
            None if n == "e" => std::f64::consts::E,
            None => f64::NAN,
        },
        Expr::Neg(a) => -point(a, x, names, p),
        Expr::Bin(op, a, b) => {
            let (a, b) = (point(a, x, names, p), point(b, x, names, p));
            match op {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                '/' => a / b,
                _ => a.powf(b),
            }
        }
        Expr::Call(f, a) => expr::apply(f, point(a, x, names, p)),
        Expr::Smooth(..) => f64::NAN,
    }
}

/// How to fit: through the origin, weights, starting guesses, an x range.
#[derive(Debug, Clone, Default)]
pub struct FitOptions<'a> {
    /// Polynomials: no constant term (the curve passes through 0, 0).
    pub through_zero: bool,
    /// Each point's error (`weighted`): points count by 1/error².
    pub sigma: Option<&'a [f64]>,
    /// Starting guesses for formula parameters (`start=a:1,b:10`).
    pub start: &'a [(String, f64)],
    /// Only points with x in this range (`fitrange=2:8`).
    pub range: Option<(f64, f64)>,
    /// The y axis is logarithmic: `exp` and `power` are fitted to the logarithms (what looks
    /// straight on that plot) instead of the values.
    pub log_y: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fitted {
    pub model: Model,
    /// Polynomial: c0 + c1·x + c2·x² …; exp: [a, b] for a·e^(bx); log: [a, b] for
    /// a + b·ln x; power: [a, b] for a·x^b; formula: its parameters in order.
    pub coef: Vec<f64>,
    /// Standard error of each coefficient (NaN when there are no spare points).
    pub se: Vec<f64>,
    /// Covariance of the coefficients (for the confidence band).
    cov: Vec<Vec<f64>>,
    pub r2: f64,
    /// Points used.
    pub n: usize,
    pub through_zero: bool,
}

impl Fitted {
    pub fn eval(&self, x: f64) -> f64 {
        self.model.value(&self.coef, x)
    }

    /// Whether the curve exists at x (log and power need x > 0).
    pub fn defined_at(&self, x: f64) -> bool {
        match self.model {
            Model::Builtin(Fit::Log | Fit::Power) => x > 0.0,
            Model::Formula { .. } => self.eval(x).is_finite(),
            Model::Builtin(_) => true,
        }
    }

    /// Half the width of the 95% confidence band of the curve at x: where the true curve
    /// lies, given the scatter of the points.
    pub fn band(&self, x: f64) -> f64 {
        let k = self.coef.len();
        let free = self.n.saturating_sub(self.free_params());
        if free == 0 {
            return f64::NAN;
        }
        let g: Vec<f64> = (0..k)
            .map(|j| {
                if self.through_zero && j == 0 && self.is_poly() {
                    return 0.0;
                }
                let h = step(self.coef[j]);
                let mut a = self.coef.clone();
                let mut b = self.coef.clone();
                a[j] += h;
                b[j] -= h;
                (self.model.value(&a, x) - self.model.value(&b, x)) / (2.0 * h)
            })
            .collect();
        let var: f64 = (0..k)
            .map(|i| (0..k).map(|j| g[i] * self.cov[i][j] * g[j]).sum::<f64>())
            .sum();
        t95(free) * var.max(0.0).sqrt()
    }

    fn is_poly(&self) -> bool {
        matches!(self.model, Model::Builtin(Fit::Poly(_)))
    }

    fn free_params(&self) -> usize {
        self.coef.len() - usize::from(self.through_zero && self.is_poly())
    }

    /// The equation and R², written with `$…$` maths: `$y = 0.41 + 2.13x$ ($R^2 = 0.987$)`;
    /// a formula shows its parameters: `$a = 2.13$, $b = 4.1$ ($R^2 = 0.998$)`.
    pub fn label(&self) -> String {
        self.label_named("y", "x", 3, false)
    }

    /// The same with the column names in place of y and x when they are short words
    /// (`yield = 85.8 - 1.39·growth`), and `digits` significant figures.
    ///
    /// `labels`: the names are axis labels (`fitnames=labels`), written as text whatever
    /// their length (`Yield = 98 - 18.4·Dose`).
    pub fn label_named(&self, y: &str, x: &str, digits: usize, labels: bool) -> String {
        let digits = digits.clamp(1, 8) as i32;
        let num = |v: f64| num_digits(v, digits);
        let name = |n: &str| {
            if labels { text_name(n) } else { math_name(n) }
        };
        let (yv, xv, mul) = match (name(y), name(x)) {
            (Some(y), Some(x)) => (y, x, "\\cdot "),
            (Some(y), None) => (y, "x".to_owned(), ""),
            _ => ("y".to_owned(), "x".to_owned(), ""),
        };
        let c = &self.coef;
        let eq = match &self.model {
            Model::Builtin(Fit::Poly(_)) => {
                // The constant first, then rising powers: `85.8 - 1.39·growth`, `a + b·x + c·x²`
                // (owner's choice, as in papers).
                let mut s = String::new();
                for (k, v) in c.iter().enumerate() {
                    if *v == 0.0 && c.len() > 1 {
                        continue;
                    }
                    let x = match k {
                        0 => String::new(),
                        1 => format!("{mul}{xv}"),
                        k => format!("{mul}{xv}^{{{k}}}"),
                    };
                    term(&mut s, *v, &x, &num);
                }
                if s.is_empty() {
                    s.push('0');
                }
                s
            }
            Model::Builtin(Fit::Exp) => format!("{}\\,e^{{{}{xv}}}", num(c[0]), num(c[1])),
            Model::Builtin(Fit::Log) => {
                let mut s = num(c[0]);
                term(&mut s, c[1], &format!("\\,\\mathrm{{ln}}\\,{xv}"), &num);
                s
            }
            Model::Builtin(Fit::Power) => format!("{}\\,{xv}^{{{}}}", num(c[0]), num(c[1])),
            Model::Formula { names, .. } => {
                let parts: Vec<String> = names
                    .iter()
                    .zip(c)
                    .map(|(n, v)| format!("${} = {}$", n, num(*v)))
                    .collect();
                return format!("{} ($R^2 = {}$)", parts.join(", "), num_digits(self.r2, 3));
            }
        };
        format!("${yv} = {eq}$ ($R^2 = {}$)", num_digits(self.r2, 3))
    }

    /// `fitstats`: lines for the table on the plot: each parameter ± its standard error,
    /// then R², RMSE (the root mean square of the residuals) and N.
    pub fn stat_lines(&self, rmse: f64) -> Vec<String> {
        let show = |v: f64, se: f64| num_plain(v, se).replace('-', "\u{2212}");
        let mut out = Vec::new();
        for (k, (name, (v, se))) in self
            .model
            .names(self.coef.len())
            .iter()
            .zip(self.coef.iter().zip(&self.se))
            .enumerate()
        {
            if self.through_zero && self.is_poly() && k == 0 {
                continue;
            }
            let name = match name.strip_prefix("x^") {
                Some(p) => format!("$x^{{{p}}}$ term"),
                None if name == "x" && self.is_poly() => "x term".to_owned(),
                None => name.clone(),
            };
            let mut line = format!("{name} = {}", show(*v, *se));
            if se.is_finite() && *se > 0.0 {
                line.push_str(&format!(" ± {}", show(*se, *se)));
            }
            out.push(line);
        }
        out.push(format!("$R^2$ = {}", num_plain(self.r2, 0.0)));
        out.push(format!("RMSE = {}", show(rmse, 0.0)));
        out.push(format!("N = {}", self.n));
        out
    }

    /// For the Terminal: the curve, each parameter ± its standard error, R² and N.
    pub fn report(&self) -> String {
        let what = match &self.model {
            Model::Builtin(f) => match f {
                Fit::Poly(1) => "linear".to_owned(),
                Fit::Poly(d) => format!("poly{d}"),
                Fit::Exp => "exp: a·e^(b·x)".to_owned(),
                Fit::Log => "log: a + b·ln x".to_owned(),
                Fit::Power => "power: a·x^b".to_owned(),
            },
            Model::Formula { text, .. } => text.clone(),
        };
        let zero = if self.through_zero && self.is_poly() {
            ", through 0"
        } else {
            ""
        };
        let mut out = format!(
            "fit {what}{zero}: R² = {}, N = {}",
            num_plain(self.r2, 0.0),
            self.n
        );
        for (k, (name, (v, se))) in self
            .model
            .names(self.coef.len())
            .iter()
            .zip(self.coef.iter().zip(&self.se))
            .enumerate()
        {
            if self.through_zero && self.is_poly() && k == 0 {
                continue;
            }
            out.push_str(&format!("\n  {name} = {}", num_plain(*v, *se)));
            if se.is_finite() && *se > 0.0 {
                out.push_str(&format!(" ± {}", num_plain(*se, *se)));
            }
        }
        out
    }
}

/// A number for the Terminal, to the precision its error supports (two significant
/// figures of the error), or five significant figures.
fn num_plain(v: f64, se: f64) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    if se.is_finite() && se > 0.0 && (1e-6..1e9).contains(&v.abs().max(se)) {
        let decimals = (1 - se.log10().floor() as i32).max(0) as usize;
        return format!("{v:.decimals$}");
    }
    let s = format!("{v:.5e}");
    match v.abs() {
        a if a != 0.0 && !(1e-4..1e7).contains(&a) => s,
        _ => {
            let digits = if v == 0.0 {
                0
            } else {
                v.abs().log10().floor() as i32
            };
            let decimals = (4 - digits).max(0) as usize;
            trim(format!("{v:.decimals$}"))
        }
    }
}

/// Student's t for a two-sided 95% interval with `df` degrees of freedom.
pub fn t95(df: usize) -> f64 {
    const T: [f64; 30] = [
        12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
        2.052, 2.048, 2.045, 2.042,
    ];
    match df {
        0 => f64::NAN,
        1..=30 => T[df - 1],
        _ => 1.96 + 2.4 / df as f64,
    }
}

/// A small change of a parameter, for slopes by finite differences.
fn step(p: f64) -> f64 {
    (p.abs() * 1e-6).max(1e-9)
}

/// Append `± |v|·x` to an equation (or `v·x` as the first term).
fn term(s: &mut String, v: f64, x: &str, num: &dyn Fn(f64) -> String) {
    let mag = num(v.abs());
    // A coefficient of 1 is left out (`x`, not `1x`), and with it the `·` before a name.
    let (mag, x) = if mag == "1" && !x.is_empty() && !x.starts_with("\\,") {
        (String::new(), x.strip_prefix("\\cdot ").unwrap_or(x))
    } else {
        (mag, x)
    };
    if s.is_empty() {
        if v < 0.0 {
            s.push('-');
        }
    } else {
        s.push_str(if v < 0.0 { " - " } else { " + " });
    }
    s.push_str(&mag);
    s.push_str(x);
}

/// A column name usable in an equation: a short word (letters and digits, starting with
/// a letter), written upright: `yield` → `\mathrm{yield}`. Longer names (`Weight (%)`) give None,
/// and the equation keeps y and x.
fn math_name(name: &str) -> Option<String> {
    let ok = !name.is_empty()
        && name.len() <= 8
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    if !ok || name == "x" || name == "y" {
        return None;
    }
    // A single letter stays italic, as in maths (T, t); words are upright.
    if name.len() == 1 {
        return Some(name.to_owned());
    }
    Some(format!("\\mathrm{{{name}}}"))
}

/// A `label=` as the y of an equation: upright text, spaces kept. None when it has maths
/// of its own (`$`, `\`, braces), which can't sit inside an equation.
fn text_name(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 40 || name.contains(['$', '\\', '{', '}', '^', '_']) {
        return None;
    }
    math_name(name).or_else(|| Some(format!("\\text{{{name}}}")))
}

/// `digits` significant figures; very large or small values as `1.23\times10^{5}`.
fn num_digits(v: f64, digits: i32) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let exp = v.abs().log10().floor() as i32;
    if !(-3..5).contains(&exp) {
        let m = v / 10f64.powi(exp);
        return format!(
            "{}\\times10^{{{exp}}}",
            trim(format!("{:.*}", (digits - 1).max(0) as usize, m))
        );
    }
    let decimals = (digits - 1 - exp).max(0) as usize;
    trim(format!("{v:.decimals$}"))
}

fn trim(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}

/// Fit `model` to the points (pairs with a missing value are skipped).
pub fn fit(model: Model, xs: &[f64], ys: &[f64], opts: &FitOptions) -> Result<Fitted, String> {
    // (x, y, weight)
    let pts: Vec<(f64, f64, f64)> = xs
        .iter()
        .zip(ys)
        .enumerate()
        .filter(|(_, (x, y))| x.is_finite() && y.is_finite())
        .filter(|(_, (x, _))| {
            opts.range
                .is_none_or(|(a, b)| **x >= a.min(b) && **x <= a.max(b))
        })
        .filter_map(|(i, (x, y))| {
            let w = match opts.sigma {
                None => 1.0,
                Some(s) => {
                    let e = *s.get(i)?;
                    (e.is_finite() && e > 0.0).then(|| 1.0 / (e * e))?
                }
            };
            Some((*x, *y, w))
        })
        .filter(|(x, y, _)| match model {
            Model::Builtin(Fit::Log) => *x > 0.0,
            Model::Builtin(Fit::Power) => *x > 0.0 && *y != 0.0,
            _ => true,
        })
        .collect();
    let through_zero = opts.through_zero && matches!(model, Model::Builtin(Fit::Poly(_)));
    let need = match &model {
        Model::Builtin(Fit::Poly(d)) => d + usize::from(!through_zero),
        Model::Builtin(_) => 2,
        Model::Formula { names, .. } => names.len(),
    };
    if pts.len() < need.max(1) {
        let why = match model {
            Model::Builtin(Fit::Log) => " with x above zero",
            Model::Builtin(Fit::Power) => " with x above zero",
            _ if opts.range.is_some() => " inside fitrange=",
            _ => "",
        };
        return Err(format!(
            "this fit needs at least {need} points{why}, and there are {}",
            pts.len()
        ));
    }
    let px: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let py: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let pw: Vec<f64> = pts.iter().map(|p| p.2).collect();
    let bad = "the points don't allow this fit (all x the same?)";
    let coef: Vec<f64> = match &model {
        Model::Builtin(Fit::Poly(d)) if !through_zero => polyfit(&px, &py, &pw, *d).ok_or(bad)?,
        Model::Builtin(Fit::Poly(d)) => {
            // Through zero: c1·x + c2·x² …, linear, so one solve settles it.
            let f = |p: &[f64], x: f64| p.iter().rev().fold(0.0, |acc, k| acc * x + k) * x;
            let (p, _) = lm(&f, vec![0.0; *d], &pts).ok_or(bad)?;
            std::iter::once(0.0).chain(p).collect()
        }
        Model::Builtin(Fit::Log) => {
            let lx: Vec<f64> = px.iter().map(|x| x.ln()).collect();
            polyfit(&lx, &py, &pw, 1).ok_or(bad)?
        }
        Model::Builtin(kind @ (Fit::Exp | Fit::Power)) => {
            // Start from the straight line through the logarithms (y all one sign).
            let sign = if py.iter().all(|y| *y < 0.0) {
                -1.0
            } else {
                1.0
            };
            let (tx, ty): (Vec<f64>, Vec<f64>) = pts
                .iter()
                .filter(|(_, y, _)| y * sign > 0.0)
                .map(|(x, y, _)| {
                    let lx = if *kind == Fit::Power { x.ln() } else { *x };
                    (lx, (y * sign).ln())
                })
                .unzip();
            if tx.len() < 2 {
                return Err(format!(
                    "this fit needs at least 2 points with y {} zero, and there are {}",
                    if sign > 0.0 { "above" } else { "below" },
                    tx.len()
                ));
            }
            let c = polyfit(&tx, &ty, &vec![1.0; tx.len()], 1).ok_or(bad)?;
            let start = vec![sign * c[0].exp(), c[1]];
            if opts.log_y {
                start
            } else {
                let f = |p: &[f64], x: f64| model.value(p, x);
                lm(&f, start.clone(), &pts).map_or(start, |(p, _)| p)
            }
        }
        Model::Formula { names, .. } => formula_fit(&model, names, &pts, opts.start)?,
    };
    // Standard errors from the slopes at the answer, scaled by the scatter left over.
    let free_idx: Vec<usize> = (0..coef.len())
        .filter(|&j| !(through_zero && j == 0))
        .collect();
    let f = |p: &[f64], x: f64| model.value(p, x);
    let (a, _) = normal(&f, &coef, &pts, &free_idx);
    let chi2: f64 = pts
        .iter()
        .map(|(x, y, w)| w * (y - model.value(&coef, *x)).powi(2))
        .sum();
    let dof = pts.len().saturating_sub(free_idx.len());
    let mut cov = vec![vec![f64::NAN; coef.len()]; coef.len()];
    if dof > 0
        && let Some(inv) = invert(a)
    {
        let s2 = chi2 / dof as f64;
        for (i, &fi) in free_idx.iter().enumerate() {
            for (j, &fj) in free_idx.iter().enumerate() {
                cov[fi][fj] = inv[i][j] * s2;
            }
        }
    }
    if through_zero {
        cov[0] = vec![0.0; coef.len()];
        for row in cov.iter_mut() {
            row[0] = 0.0;
        }
    }
    let se = (0..coef.len()).map(|j| cov[j][j].max(0.0).sqrt()).collect();
    // R² on the original values.
    let mean = py.iter().sum::<f64>() / py.len() as f64;
    let total: f64 = py.iter().map(|y| (y - mean).powi(2)).sum();
    let resid: f64 = px
        .iter()
        .zip(&py)
        .map(|(x, y)| (y - model.value(&coef, *x)).powi(2))
        .sum();
    let r2 = if total > 0.0 {
        1.0 - resid / total
    } else {
        1.0
    };
    if !coef.iter().all(|c| c.is_finite()) || !resid.is_finite() {
        return Err("the fit didn't settle on numbers (try start= with rough guesses)".into());
    }
    Ok(Fitted {
        model,
        coef,
        se,
        cov,
        r2,
        n: pts.len(),
        through_zero,
    })
}

/// A formula: Levenberg–Marquardt from several starting points (the owner's guesses from
/// `start=`, else 1 for each parameter and some guesses from the data's size), keeping the
/// best.
fn formula_fit(
    model: &Model,
    names: &[String],
    pts: &[(f64, f64, f64)],
    start: &[(String, f64)],
) -> Result<Vec<f64>, String> {
    for (n, _) in start {
        if !names.contains(n) {
            return Err(format!(
                "start= names {n}, which isn't in the formula (its letters: {})",
                names.join(", ")
            ));
        }
    }
    let given = |n: &String| start.iter().find(|(m, _)| m == n).map(|(_, v)| *v);
    let base: Vec<f64> = names.iter().map(|n| given(n).unwrap_or(1.0)).collect();
    let span = |v: &mut dyn Iterator<Item = f64>| {
        let (lo, hi) = v.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
            (a.min(x), b.max(x))
        });
        if hi > lo { hi - lo } else { 1.0 }
    };
    let xspan = span(&mut pts.iter().map(|p| p.0));
    let yspan = span(&mut pts.iter().map(|p| p.1));
    let ymean = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
    let mut starts = vec![base.clone()];
    let open: Vec<usize> = (0..names.len())
        .filter(|&j| given(&names[j]).is_none())
        .collect();
    if !open.is_empty() {
        let mut all = base.clone();
        for &j in &open {
            all[j] = yspan;
        }
        starts.push(all);
        for &j in &open {
            for v in [xspan, -1.0, ymean, yspan, 1.0 / xspan] {
                let mut s = base.clone();
                s[j] = v;
                starts.push(s);
            }
        }
    }
    let f = |p: &[f64], x: f64| model.value(p, x);
    starts
        .into_iter()
        .filter_map(|s| lm(&f, s, pts))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p)
        .ok_or_else(|| {
            "the formula can't be fitted from these starting guesses (it gives no numbers \
             there): give rough ones with start=, e.g. start=a:1,b:10"
                .to_owned()
        })
}

/// Levenberg–Marquardt least squares: the parameters and the weighted sum of squared
/// misses. None if the curve gives no numbers at the start.
fn lm(
    f: &dyn Fn(&[f64], f64) -> f64,
    mut p: Vec<f64>,
    pts: &[(f64, f64, f64)],
) -> Option<(Vec<f64>, f64)> {
    let chi2 = |p: &[f64]| -> f64 {
        let s: f64 = pts.iter().map(|(x, y, w)| w * (y - f(p, *x)).powi(2)).sum();
        if s.is_finite() { s } else { f64::INFINITY }
    };
    let mut c = chi2(&p);
    if !c.is_finite() {
        return None;
    }
    let all: Vec<usize> = (0..p.len()).collect();
    let mut lambda = 1e-3;
    for _ in 0..300 {
        let (a, g) = normal(f, &p, pts, &all);
        let mut moved = false;
        for _ in 0..16 {
            let mut m = a.clone();
            for (i, row) in m.iter_mut().enumerate() {
                row[i] += lambda * row[i].abs().max(1e-12);
            }
            let Some(d) = solve(m, g.clone()) else {
                lambda *= 10.0;
                continue;
            };
            let q: Vec<f64> = p.iter().zip(&d).map(|(a, b)| a + b).collect();
            let cq = chi2(&q);
            if cq < c {
                let small = c - cq <= 1e-12 * c.max(1e-300);
                p = q;
                c = cq;
                lambda = (lambda / 10.0).max(1e-15);
                moved = true;
                if small {
                    return Some((p, c));
                }
                break;
            }
            lambda *= 10.0;
        }
        if !moved || c == 0.0 {
            break;
        }
    }
    Some((p, c))
}

/// JᵀWJ and JᵀW·r for the parameters `idx` (J: how the curve changes with each).
fn normal(
    f: &dyn Fn(&[f64], f64) -> f64,
    p: &[f64],
    pts: &[(f64, f64, f64)],
    idx: &[usize],
) -> (Vec<Vec<f64>>, Vec<f64>) {
    let k = idx.len();
    let mut a = vec![vec![0.0; k]; k];
    let mut g = vec![0.0; k];
    let mut jrow = vec![0.0; k];
    let mut q = p.to_vec();
    for (x, y, w) in pts {
        let r = y - f(p, *x);
        for (col, &j) in idx.iter().enumerate() {
            let h = step(p[j]);
            q[j] = p[j] + h;
            let up = f(&q, *x);
            q[j] = p[j] - h;
            let down = f(&q, *x);
            q[j] = p[j];
            jrow[col] = (up - down) / (2.0 * h);
        }
        if !r.is_finite() || jrow.iter().any(|v| !v.is_finite()) {
            continue;
        }
        for i in 0..k {
            g[i] += w * jrow[i] * r;
            for j in 0..k {
                a[i][j] += w * jrow[i] * jrow[j];
            }
        }
    }
    (a, g)
}

/// Solve A·x = b by Gaussian elimination with partial pivoting.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let k = b.len();
    for col in 0..k {
        let pivot = (col..k).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-300 || !a[pivot][col].is_finite() {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pivot_row = a[col].clone();
        let pivot_b = b[col];
        for (r, row) in a.iter_mut().enumerate() {
            if r != col {
                let f = row[col] / pivot_row[col];
                for (cell, p) in row.iter_mut().zip(&pivot_row).skip(col) {
                    *cell -= f * p;
                }
                b[r] -= f * pivot_b;
            }
        }
    }
    let x: Vec<f64> = (0..k).map(|i| b[i] / a[i][i]).collect();
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// The inverse of a small matrix (column by column).
fn invert(a: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let k = a.len();
    let cols: Vec<Vec<f64>> = (0..k)
        .map(|j| {
            let e: Vec<f64> = (0..k).map(|i| f64::from(u8::from(i == j))).collect();
            solve(a.clone(), e)
        })
        .collect::<Option<_>>()?;
    Some(
        (0..k)
            .map(|i| (0..k).map(|j| cols[j][i]).collect())
            .collect(),
    )
}

/// Least-squares polynomial (each point weighted); coefficients from the constant up. x is centred and scaled
/// first so higher powers stay accurate, then the result is expanded back.
pub(crate) fn polyfit(xs: &[f64], ys: &[f64], ws: &[f64], degree: usize) -> Option<Vec<f64>> {
    let n = xs.len() as f64;
    let m = xs.iter().sum::<f64>() / n;
    let s = (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n).sqrt();
    if s == 0.0 || !s.is_finite() {
        return None;
    }
    let k = degree + 1;
    // Normal equations A·c = b in t = (x − m)/s.
    let mut a = vec![vec![0.0; k + 1]; k];
    for ((x, y), w) in xs.iter().zip(ys).zip(ws) {
        let t = (x - m) / s;
        let pow: Vec<f64> = (0..2 * k).map(|i| t.powi(i as i32)).collect();
        for (r, row) in a.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().take(k).enumerate() {
                *cell += w * pow[r + c];
            }
            row[k] += w * pow[r] * y;
        }
    }
    // Gaussian elimination with partial pivoting.
    for col in 0..k {
        let pivot = (col..k).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        let pivot_row = a[col].clone();
        for (r, row) in a.iter_mut().enumerate() {
            if r != col {
                let f = row[col] / pivot_row[col];
                for (cell, p) in row.iter_mut().zip(&pivot_row).skip(col) {
                    *cell -= f * p;
                }
            }
        }
    }
    let ct: Vec<f64> = (0..k).map(|i| a[i][k] / a[i][i]).collect();
    // Expand Σ ct_i ((x − m)/s)^i into powers of x.
    let mut out = vec![0.0; k];
    let mut basis = vec![1.0]; // ((x − m)/s)^i as coefficients in x
    for c in &ct {
        for (j, b) in basis.iter().enumerate() {
            out[j] += c * b;
        }
        let mut next = vec![0.0; basis.len() + 1];
        for (j, b) in basis.iter().enumerate() {
            next[j + 1] += b / s;
            next[j] -= b * m / s;
        }
        basis = next;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_line_and_label() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        let ys = [2.5, 4.5, 6.5, 8.5];
        let f = fit(
            Model::Builtin(Fit::Poly(1)),
            &xs,
            &ys,
            &FitOptions::default(),
        )
        .unwrap();
        assert!((f.coef[1] - 2.0).abs() < 1e-9 && (f.coef[0] - 0.5).abs() < 1e-9);
        assert!((f.r2 - 1.0).abs() < 1e-12);
        assert_eq!(f.label(), "$y = 0.5 + 2x$ ($R^2 = 1$)");
        // Column names when they are short words, and the digits asked for.
        assert_eq!(
            f.label_named("yield", "growth", 3, false),
            "$\\mathrm{yield} = 0.5 + 2\\cdot \\mathrm{growth}$ ($R^2 = 1$)"
        );
        assert_eq!(
            f.label_named("T", "t", 3, false),
            "$T = 0.5 + 2\\cdot t$ ($R^2 = 1$)"
        );
        assert_eq!(f.label_named("Temp (C)", "t", 3, false), f.label());
        // Axis labels (fitnames=labels) are written whatever their length; maths isn't.
        assert!(
            f.label_named("Temp (C)", "t", 3, true)
                .starts_with("$\\text{Temp (C)} = ")
        );
        assert_eq!(f.label_named("$T_g$", "t", 3, true), f.label());
    }

    #[test]
    fn quadratic_far_from_zero() {
        let xs: Vec<f64> = (0..20).map(|i| 1000.0 + i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 3.0 * x * x - 2.0 * x + 1.0).collect();
        let f = fit(
            Model::Builtin(Fit::Poly(2)),
            &xs,
            &ys,
            &FitOptions::default(),
        )
        .unwrap();
        assert!((f.eval(1010.0) - ys[10]).abs() / ys[10] < 1e-9);
        assert!(
            f.label().ends_with(" - 2x + 3x^{2}$ ($R^2 = 1$)"),
            "{}",
            f.label()
        );
    }

    #[test]
    fn exp_log_power() {
        let xs: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
        let e: Vec<f64> = xs.iter().map(|x| 2.0 * (0.5 * x).exp()).collect();
        let f = fit(Model::Builtin(Fit::Exp), &xs, &e, &FitOptions::default()).unwrap();
        assert!((f.coef[0] - 2.0).abs() < 1e-9 && (f.coef[1] - 0.5).abs() < 1e-9);
        assert_eq!(f.label(), "$y = 2\\,e^{0.5x}$ ($R^2 = 1$)");
        let p: Vec<f64> = xs.iter().map(|x| 3.0 * x.powf(1.5)).collect();
        let f = fit(Model::Builtin(Fit::Power), &xs, &p, &FitOptions::default()).unwrap();
        assert!((f.coef[1] - 1.5).abs() < 1e-9);
        let l: Vec<f64> = xs.iter().map(|x| 1.0 - 2.0 * x.ln()).collect();
        let f = fit(Model::Builtin(Fit::Log), &xs, &l, &FitOptions::default()).unwrap();
        assert!(
            f.label().contains("1 - 2\\,\\mathrm{ln}\\,x"),
            "{}",
            f.label()
        );
        assert!(
            fit(
                Model::Builtin(Fit::Power),
                &[-1.0, -2.0],
                &[1.0, 2.0],
                &FitOptions::default()
            )
            .is_err()
        );
    }

    #[test]
    fn formulas_weights_and_errors() {
        let xs: Vec<f64> = (0..30).map(f64::from).collect();
        let ys: Vec<f64> = xs
            .iter()
            .enumerate()
            .map(|(i, x)| 50.0 * (-x / 7.0).exp() + 3.0 + if i % 2 == 0 { 0.2 } else { -0.2 })
            .collect();
        let m = Model::formula("a*exp(-x/tau)+c").unwrap();
        let f = fit(m, &xs, &ys, &FitOptions::default()).unwrap();
        assert!((f.coef[1] - 7.0).abs() < 0.1, "{:?}", f.coef);
        assert!((f.coef[2] - 3.0).abs() < 0.2, "{:?}", f.coef);
        assert!(f.se.iter().all(|s| s.is_finite() && *s > 0.0));
        assert!(f.band(10.0) > 0.0);
        assert!(f.report().contains("tau = 6.99"), "{}", f.report());
        // A straight line's standard errors match the textbook formula.
        let (lx, ly) = ([1.0, 2.0, 3.0, 4.0, 5.0], [1.1, 1.9, 3.2, 3.8, 5.1]);
        let l = fit(
            Model::Builtin(Fit::Poly(1)),
            &lx,
            &ly,
            &FitOptions::default(),
        )
        .unwrap();
        let resid: f64 = lx
            .iter()
            .zip(&ly)
            .map(|(x, y)| (y - l.eval(*x)).powi(2))
            .sum();
        let se_slope = (resid / 3.0 / 10.0).sqrt(); // Σ(x − x̄)² = 10
        assert!(
            (l.se[1] - se_slope).abs() < 1e-6,
            "{} vs {se_slope}",
            l.se[1]
        );
        // Through zero: no constant term.
        let z = FitOptions {
            through_zero: true,
            ..Default::default()
        };
        let l0 = fit(Model::Builtin(Fit::Poly(1)), &lx, &ly, &z).unwrap();
        assert_eq!(l0.coef[0], 0.0);
        assert!((l0.coef[1] - 55.2 / 55.0).abs() < 1e-9, "{:?}", l0.coef); // Σxy / Σx²
        // Weights: a point with a huge error barely counts.
        let mut wy = ly;
        wy[4] = 50.0;
        let sig = [0.1, 0.1, 0.1, 0.1, 1000.0];
        let w = FitOptions {
            sigma: Some(&sig),
            ..Default::default()
        };
        let lw = fit(Model::Builtin(Fit::Poly(1)), &lx, &wy, &w).unwrap();
        assert!((lw.coef[1] - 1.0).abs() < 0.1, "{:?}", lw.coef);
        // fitrange= keeps only those points.
        let r = FitOptions {
            range: Some((1.0, 3.0)),
            ..Default::default()
        };
        assert_eq!(
            fit(Model::Builtin(Fit::Poly(1)), &lx, &ly, &r).unwrap().n,
            3
        );
        // Formula mistakes.
        assert!(Model::formula("a*b").unwrap_err().contains("with x"));
        assert!(
            Model::formula("2*x")
                .unwrap_err()
                .contains("something to fit")
        );
        assert!(Model::formula("cumsum(x)*a").is_err());
    }

    #[test]
    fn number_formatting() {
        assert_eq!(num_digits(0.004123, 3), "0.00412");
        assert_eq!(num_digits(123456.0, 3), "1.23\\times10^{5}");
        assert_eq!(num_digits(-12.345, 3), "-12.3");
    }
}
