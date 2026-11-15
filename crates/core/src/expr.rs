//! Calculated columns: `y=power/area`, `x=t/60`, `y=log10(counts)`.
//!
//! ```text
//! sum     := product (('+' | '-') product)*
//! product := unary (('*' | '/') unary)*
//! unary   := '-' unary | power
//! power   := atom ('^' unary)?
//! atom    := number | column | function '(' sum ')' | '(' sum ')'
//! ```
//!
//! Columns are named as in the file (any case); a name with spaces or symbols goes in
//! backticks: `` `Voltage (V)` * 1000 ``. `pi` and `e` are constants unless a column has that
//! name. Functions work row by row, except `cumsum` (running total), `diff` (change from
//! the row before), `norm` (scaled to 0–1) and `smooth(col, n)` (average of `n` rows
//! around each row; `n` defaults to 5). A missing value or an impossible result (`log` of a negative number)
//! gives a gap, like a missing cell.

use crate::data::{ColumnData, Table};
use crate::error::suggestion;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Column(String),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Call(&'static str, Box<Expr>),
    /// `smooth(col, n)`: the average of the `n` rows around each row.
    Smooth(Box<Expr>, usize),
}

/// Functions, with what they do (for errors and the docs).
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("ln", "natural logarithm"),
    ("log", "natural logarithm (as in Python and numpy)"),
    ("log10", "logarithm base 10"),
    ("log2", "logarithm base 2"),
    ("exp", "e to the power"),
    ("sqrt", "square root"),
    ("abs", "absolute value"),
    ("sin", "sine (radians)"),
    ("cos", "cosine (radians)"),
    ("tan", "tangent (radians)"),
    ("asin", "arc sine"),
    ("acos", "arc cosine"),
    ("atan", "arc tangent"),
    ("sinh", "hyperbolic sine"),
    ("cosh", "hyperbolic cosine"),
    ("tanh", "hyperbolic tangent"),
    ("floor", "round down"),
    ("ceil", "round up"),
    ("round", "round to whole numbers"),
    ("cumsum", "running total"),
    ("diff", "change from the row before"),
    ("norm", "scaled to 0–1 (smallest 0, largest 1)"),
    (
        "smooth",
        "moving average: smooth(col, 5) averages 5 rows around each row",
    ),
];

/// Whether a column name as typed should be read as a calculation (it is only tried when no
/// column has exactly that name).
pub fn looks_like_expr(s: &str) -> bool {
    s.chars()
        .any(|c| "+-*/^()` ".contains(c) || c.is_whitespace())
}

pub fn parse(src: &str) -> Result<Expr, String> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, at: 0 };
    let e = p.sum()?;
    match p.toks.get(p.at) {
        None => Ok(e),
        Some(t) => Err(format!("unexpected {} in the calculation", t.describe())),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Quoted(String),
    Sym(char),
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Tok::Num(v) => format!("number {v}"),
            Tok::Name(n) | Tok::Quoted(n) => format!("\"{n}\""),
            Tok::Sym(c) => format!("\"{c}\""),
        }
    }
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if "+-*/^(),".contains(c) {
            out.push(Tok::Sym(c));
            i += 1;
        } else if c == '`' {
            let end = cs[i + 1..]
                .iter()
                .position(|x| *x == '`')
                .ok_or("missing closing `")?;
            out.push(Tok::Quoted(cs[i + 1..i + 1 + end].iter().collect()));
            i += end + 2;
        } else if c.is_ascii_digit()
            || (c == '.' && cs.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            // Exponent: 1e3, 2.5E-4.
            if i < cs.len()
                && (cs[i] == 'e' || cs[i] == 'E')
                && (cs.get(i + 1).is_some_and(char::is_ascii_digit)
                    || (matches!(cs.get(i + 1), Some('+' | '-'))
                        && cs.get(i + 2).is_some_and(char::is_ascii_digit)))
            {
                i += 2;
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text: String = cs[start..i].iter().collect();
            let v = text
                .parse::<f64>()
                .map_err(|_| format!("\"{text}\" is not a number"))?;
            out.push(Tok::Num(v));
        } else {
            let start = i;
            while i < cs.len() && !cs[i].is_whitespace() && !"+-*/^(),`".contains(cs[i]) {
                i += 1;
            }
            out.push(Tok::Name(cs[start..i].iter().collect()));
        }
    }
    if out.is_empty() {
        return Err("empty calculation".into());
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn eat(&mut self, c: char) -> bool {
        if self.toks.get(self.at) == Some(&Tok::Sym(c)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Result<Expr, String> {
        let mut e = self.product()?;
        loop {
            let op = if self.eat('+') {
                '+'
            } else if self.eat('-') {
                '-'
            } else {
                return Ok(e);
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.product()?));
        }
    }

    fn product(&mut self) -> Result<Expr, String> {
        let mut e = self.unary()?;
        loop {
            let op = if self.eat('*') {
                '*'
            } else if self.eat('/') {
                '/'
            } else {
                return Ok(e);
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat('-') {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        let base = self.atom()?;
        if self.eat('^') {
            return Ok(Expr::Bin('^', Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        let tok = self.toks.get(self.at).cloned();
        self.at += 1;
        match tok {
            Some(Tok::Num(v)) => Ok(Expr::Num(v)),
            Some(Tok::Quoted(name)) => Ok(Expr::Column(name)),
            Some(Tok::Name(name)) => {
                if self.toks.get(self.at) == Some(&Tok::Sym('(')) {
                    let f = FUNCTIONS
                        .iter()
                        .find(|(f, _)| f.eq_ignore_ascii_case(&name))
                        .map(|(f, _)| *f)
                        .ok_or_else(|| {
                            format!(
                                "unknown function {name}(){}",
                                suggestion(&name, FUNCTIONS.iter().map(|(f, _)| *f))
                            )
                        })?;
                    self.at += 1;
                    let inner = self.sum()?;
                    // smooth(col, n): how many rows to average (5 if left out).
                    let window = if f == "smooth" && self.eat(',') {
                        match self.toks.get(self.at) {
                            Some(Tok::Num(n)) if n.fract() == 0.0 && *n >= 1.0 => {
                                self.at += 1;
                                Some(*n as usize)
                            }
                            _ => {
                                return Err(
                                    "smooth(col, n): n is how many rows to average, a whole number like 5"
                                        .into(),
                                );
                            }
                        }
                    } else {
                        None
                    };
                    if !self.eat(')') {
                        return Err(format!("missing ) after {f}("));
                    }
                    if f == "smooth" {
                        return Ok(Expr::Smooth(Box::new(inner), window.unwrap_or(5)));
                    }
                    return Ok(Expr::Call(f, Box::new(inner)));
                }
                Ok(Expr::Column(name))
            }
            Some(Tok::Sym('(')) => {
                let e = self.sum()?;
                if !self.eat(')') {
                    return Err("missing )".into());
                }
                Ok(e)
            }
            Some(t) => Err(format!("expected a number or column, not {}", t.describe())),
            None => Err("the calculation ends too early".into()),
        }
    }
}

impl Expr {
    /// Evaluate for every row of `table`.
    pub fn eval(&self, t: &Table) -> Result<Vec<f64>, String> {
        Ok(match self {
            Expr::Smooth(e, n) => moving_average(&e.eval(t)?, *n),
            Expr::Num(v) => vec![*v; t.rows],
            Expr::Column(name) => column(t, name)?,
            Expr::Neg(e) => e.eval(t)?.into_iter().map(|v| -v).collect(),
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(t)?, b.eval(t)?);
                a.iter()
                    .zip(&b)
                    .map(|(x, y)| match op {
                        '+' => x + y,
                        '-' => x - y,
                        '*' => x * y,
                        '/' => x / y,
                        _ => x.powf(*y),
                    })
                    .map(finite)
                    .collect()
            }
            Expr::Call(f, e) => {
                let v = e.eval(t)?;
                match *f {
                    "cumsum" => {
                        let mut total = 0.0;
                        v.iter()
                            .map(|x| {
                                if x.is_finite() {
                                    total += x;
                                    total
                                } else {
                                    f64::NAN
                                }
                            })
                            .collect()
                    }
                    "diff" => std::iter::once(f64::NAN)
                        .chain(v.windows(2).map(|w| w[1] - w[0]))
                        .take(v.len())
                        .collect(),
                    "norm" => {
                        let finite_vals = v.iter().filter(|x| x.is_finite());
                        let lo = finite_vals.clone().fold(f64::INFINITY, |a, b| a.min(*b));
                        let hi = finite_vals.fold(f64::NEG_INFINITY, |a, b| a.max(*b));
                        let span = hi - lo;
                        v.iter()
                            .map(|x| if span > 0.0 { (x - lo) / span } else { 0.0 })
                            .map(finite)
                            .collect()
                    }
                    _ => v.into_iter().map(|x| finite(apply(f, x))).collect(),
                }
            }
        })
    }

    /// Column names used, for messages.
    pub fn columns(&self) -> Vec<&str> {
        match self {
            Expr::Num(_) => vec![],
            Expr::Column(n) => vec![n.as_str()],
            Expr::Neg(e) | Expr::Call(_, e) | Expr::Smooth(e, _) => e.columns(),
            Expr::Bin(_, a, b) => {
                let mut v = a.columns();
                v.extend(b.columns());
                v
            }
        }
    }
}

/// The average of the `n` rows centred on each row (fewer at the ends; gaps are skipped,
/// and a row that is itself a gap stays one).
pub fn moving_average(v: &[f64], n: usize) -> Vec<f64> {
    let before = (n.max(1) - 1) / 2;
    let after = n.max(1) - 1 - before;
    (0..v.len())
        .map(|i| {
            if !v[i].is_finite() {
                return f64::NAN;
            }
            let window = &v[i.saturating_sub(before)..(i + after + 1).min(v.len())];
            let (sum, count) = window
                .iter()
                .filter(|x| x.is_finite())
                .fold((0.0, 0.0), |(s, c), x| (s + x, c + 1.0));
            sum / count
        })
        .collect()
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { f64::NAN }
}

pub(crate) fn apply(f: &str, x: f64) -> f64 {
    match f {
        "ln" | "log" => x.ln(),
        "log10" => x.log10(),
        "log2" => x.log2(),
        "exp" => x.exp(),
        "sqrt" => x.sqrt(),
        "abs" => x.abs(),
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        _ => x.round(),
    }
}

fn column(t: &Table, name: &str) -> Result<Vec<f64>, String> {
    let found = t
        .columns
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name.trim()));
    let Some(col) = found else {
        match name.to_ascii_lowercase().as_str() {
            "pi" => return Ok(vec![std::f64::consts::PI; t.rows]),
            "e" => return Ok(vec![std::f64::consts::E; t.rows]),
            _ => {}
        }
        return Err(format!(
            "no column \"{name}\" in {}{} (use `backticks` for names with spaces or symbols)",
            t.file_name(),
            suggestion(name, t.column_names())
        ));
    };
    match &col.data {
        ColumnData::Number(v) => Ok(v.clone()),
        ColumnData::Date(_) => Err(format!(
            "\"{}\" holds dates, which can't be calculated with",
            col.name
        )),
        ColumnData::Text(_) => Err(format!(
            "\"{}\" holds text, which can't be calculated with",
            col.name
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn table() -> Table {
        crate::data::parse(
            "t,power,area,Voltage (V),site\n1,10,2,1.5,a\n2,20,4,,b\n3,-30,0,3,c\n",
            Path::new("t.csv"),
        )
        .unwrap()
    }

    fn calc(src: &str) -> Vec<f64> {
        parse(src).unwrap().eval(&table()).unwrap()
    }

    fn same(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| (x.is_nan() && y.is_nan()) || (x - y).abs() < 1e-9)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert!(same(&calc("power/area"), &[5.0, 5.0, f64::NAN]));
        assert!(same(&calc("t/60*60"), &[1.0, 2.0, 3.0]));
        assert!(same(&calc("1 + 2*t^2"), &[3.0, 9.0, 19.0]));
        assert!(same(&calc("-t^2"), &[-1.0, -4.0, -9.0]));
        assert!(same(&calc("(t+1)*2"), &[4.0, 6.0, 8.0]));
        assert!(same(&calc("2e1 + t"), &[21.0, 22.0, 23.0]));
        assert!(same(
            &calc("`Voltage (V)` * 1000"),
            &[1500.0, f64::NAN, 3000.0]
        ));
    }

    #[test]
    fn functions() {
        assert!(same(&calc("log10(power)"), &[1.0, 20f64.log10(), f64::NAN]));
        assert!(same(&calc("abs(power)"), &[10.0, 20.0, 30.0]));
        assert!(same(&calc("cumsum(t)"), &[1.0, 3.0, 6.0]));
        assert!(same(&calc("diff(power)"), &[f64::NAN, 10.0, -50.0]));
        assert!(same(&calc("cos(pi*0)"), &[1.0, 1.0, 1.0]));
    }

    #[test]
    fn smoothing_and_scaling() {
        assert!(same(
            &moving_average(&[1.0, 2.0, 6.0, 4.0, 5.0], 3),
            &[1.5, 3.0, 4.0, 5.0, 4.5]
        ));
        // Gaps are skipped in the average and stay gaps.
        assert!(same(
            &moving_average(&[1.0, f64::NAN, 3.0], 3),
            &[1.0, f64::NAN, 3.0]
        ));
        let n = calc("norm(abs(power))");
        assert!(same(&n, &[0.0, 0.5, 1.0]), "{n:?}");
        let s = calc("smooth(abs(power), 3)");
        assert!(same(&s, &[15.0, 20.0, 25.0]), "{s:?}");
        assert!(
            parse("smooth(power, 2.5)")
                .unwrap_err()
                .contains("whole number")
        );
    }

    #[test]
    fn helpful_errors() {
        let err = |s: &str| match parse(s) {
            Err(e) => e,
            Ok(e) => e.eval(&table()).unwrap_err(),
        };
        assert!(err("powr/area").contains("did you mean \"power\""));
        assert!(err("site*2").contains("holds text"));
        assert!(err("lg(t)").contains("unknown function"));
        assert!(err("(t+1").contains("missing )"));
        assert!(err("t +").contains("ends too early"));
        assert!(looks_like_expr("power/area") && !looks_like_expr("temp"));
    }
}
