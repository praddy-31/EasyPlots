//! Row filters: `where="temp > 20 and site == 'north'"` keeps only the matching rows.
//!
//! ```text
//! condition := part (or part)*
//! part      := test (and test)*
//! test      := not test | ( condition ) | column op value
//! op        := > < >= <= == = !=
//! ```
//!
//! Columns are named as in the file (any case); a name with spaces goes in backticks:
//! `` `Voltage (V)` > 1.2 ``. Values are numbers, dates (`2026-03-01`) or text in single
//! quotes. Number columns compare as numbers, date columns as dates, text columns only with
//! `==` / `!=`. Missing values never match.

use std::cmp::Ordering;

use crate::data::{self, ColumnData, Table};
use crate::error::suggestion;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Quoted(String),
    Op(Op),
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Or(Vec<Expr>),
    And(Vec<Expr>),
    Not(Box<Expr>),
    Test {
        column: String,
        op: Op,
        value: Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Bare(String),
    Quoted(String),
}

/// Tokens with their char positions in the condition (for reading calculations back out).
fn tokenize(src: &str) -> Result<Vec<(Tok, std::ops::Range<usize>)>, String> {
    let mut out: Vec<(Tok, std::ops::Range<usize>)> = Vec::new();
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let start = i;
        let before = out.len();
        match c {
            c if c.is_whitespace() => i += 1,
            '(' => {
                out.push((Tok::Open, 0..0));
                i += 1;
            }
            ')' => {
                out.push((Tok::Close, 0..0));
                i += 1;
            }
            '\'' | '`' | '"' => {
                let end = cs[i + 1..]
                    .iter()
                    .position(|x| *x == c)
                    .ok_or_else(|| format!("missing closing {c}"))?;
                let text: String = cs[i + 1..i + 1 + end].iter().collect();
                // Backticks name a column; quotes are text values.
                out.push((
                    if c == '`' {
                        Tok::Word(text)
                    } else {
                        Tok::Quoted(text)
                    },
                    0..0,
                ));
                i += end + 2;
            }
            '>' | '<' | '=' | '!' => {
                let two = cs.get(i + 1) == Some(&'=');
                let op = match (c, two) {
                    ('>', true) => Op::Ge,
                    ('>', false) => Op::Gt,
                    ('<', true) => Op::Le,
                    ('<', false) => Op::Lt,
                    ('=', _) => Op::Eq,
                    ('!', true) => Op::Ne,
                    _ => return Err("use != for \"not equal\"".into()),
                };
                out.push((Tok::Op(op), 0..0));
                i += if two { 2 } else { 1 };
            }
            _ => {
                while i < cs.len() && !cs[i].is_whitespace() && !"()<>=!'`\"".contains(cs[i]) {
                    i += 1;
                }
                out.push((Tok::Word(cs[start..i].iter().collect()), 0..0));
            }
        }
        if out.len() > before {
            out.last_mut().unwrap().1 = start..i;
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    spans: Vec<std::ops::Range<usize>>,
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    /// A calculation before the comparison (`power/area > 3`, `(a + b)/2 < 1`): the tokens
    /// up to the first comparison, if they are more than one word and their brackets match.
    /// Returns its text and where the comparison is.
    fn calculation_ahead(&self) -> Option<(String, usize)> {
        let k = (self.at..self.toks.len()).find(|&i| matches!(self.toks[i], Tok::Op(_)))?;
        if k <= self.at + 1 {
            return None;
        }
        let mut depth = 0i32;
        for t in &self.toks[self.at..k] {
            match t {
                Tok::Open => depth += 1,
                Tok::Close => {
                    depth -= 1;
                    if depth < 0 {
                        return None;
                    }
                }
                Tok::Word(w)
                    if ["and", "or", "not"]
                        .iter()
                        .any(|x| w.eq_ignore_ascii_case(x)) =>
                {
                    return None;
                }
                Tok::Word(_) => {}
                _ => return None,
            }
        }
        if depth != 0 {
            return None;
        }
        let text: String = self.chars[self.spans[self.at].start..self.spans[k - 1].end]
            .iter()
            .collect();
        Some((text, k))
    }

    fn peek_word(&self, w: &str) -> bool {
        matches!(self.toks.get(self.at), Some(Tok::Word(x)) if x.eq_ignore_ascii_case(w))
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut parts = vec![self.and()?];
        while self.peek_word("or") {
            self.at += 1;
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Expr::Or(parts)
        })
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut parts = vec![self.test()?];
        while self.peek_word("and") {
            self.at += 1;
            parts.push(self.test()?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Expr::And(parts)
        })
    }

    fn test(&mut self) -> Result<Expr, String> {
        if self.peek_word("not") {
            self.at += 1;
            return Ok(Expr::Not(Box::new(self.test()?)));
        }
        if let Some((text, k)) = self.calculation_ahead() {
            self.at = k;
            let column = format!("({text})");
            return self.comparison(column);
        }
        match self.toks.get(self.at).cloned() {
            Some(Tok::Open) => {
                self.at += 1;
                let e = self.or()?;
                if self.toks.get(self.at) != Some(&Tok::Close) {
                    return Err("missing )".into());
                }
                self.at += 1;
                Ok(e)
            }
            Some(Tok::Word(column)) => {
                self.at += 1;
                self.comparison(column)
            }
            _ => Err("expected a column name, e.g. temp > 20".into()),
        }
    }

    /// `op value` after a column (or a bracketed calculation).
    fn comparison(&mut self, column: String) -> Result<Expr, String> {
        let Some(Tok::Op(op)) = self.toks.get(self.at).cloned() else {
            return Err(format!(
                "expected a comparison after {column}, e.g. {column} > 20"
            ));
        };
        self.at += 1;
        let value = match self.toks.get(self.at).cloned() {
            Some(Tok::Word(w)) => Value::Bare(w),
            Some(Tok::Quoted(q)) => Value::Quoted(q),
            _ => return Err(format!("expected a value after the comparison on {column}")),
        };
        self.at += 1;
        Ok(Expr::Test { column, op, value })
    }
}

/// A checked filter, ready to apply to a table.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter(Expr);

pub fn parse(src: &str) -> Result<Filter, String> {
    let (toks, spans) = tokenize(src)?.into_iter().unzip();
    let mut p = Parser {
        toks,
        spans,
        chars: src.chars().collect(),
        at: 0,
    };
    let e = p.or()?;
    if p.at != p.toks.len() {
        return Err("unexpected text after the condition — join conditions with and / or".into());
    }
    Ok(Filter(e))
}

impl Filter {
    /// Which rows of `table` match.
    pub fn mask(&self, table: &Table) -> Result<Vec<bool>, String> {
        eval(&self.0, table)
    }
}

fn eval(e: &Expr, t: &Table) -> Result<Vec<bool>, String> {
    Ok(match e {
        Expr::Or(parts) => combine(parts, t, |a, b| a || b)?,
        Expr::And(parts) => combine(parts, t, |a, b| a && b)?,
        Expr::Not(inner) => eval(inner, t)?.into_iter().map(|b| !b).collect(),
        Expr::Test { column, op, value } => test(t, column, *op, value)?,
    })
}

fn combine(parts: &[Expr], t: &Table, f: impl Fn(bool, bool) -> bool) -> Result<Vec<bool>, String> {
    let mut acc = eval(&parts[0], t)?;
    for p in &parts[1..] {
        for (a, b) in acc.iter_mut().zip(eval(p, t)?) {
            *a = f(*a, b);
        }
    }
    Ok(acc)
}

fn ordering_matches(op: Op, ord: Ordering) -> bool {
    match op {
        Op::Gt => ord == Ordering::Greater,
        Op::Lt => ord == Ordering::Less,
        Op::Ge => ord != Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Eq => ord == Ordering::Equal,
        Op::Ne => ord != Ordering::Equal,
    }
}

fn test(t: &Table, column: &str, op: Op, value: &Value) -> Result<Vec<bool>, String> {
    let found = t
        .columns
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(column));
    if found.is_none() && crate::expr::looks_like_expr(column) {
        let values = crate::expr::parse(column)?.eval(t)?;
        let raw = match value {
            Value::Bare(s) | Value::Quoted(s) => s.as_str(),
        };
        let target = data::parse_number(raw, t.decimal_comma)
            .ok_or_else(|| format!("compare the calculation with a number, not {raw}"))?;
        return Ok(values
            .iter()
            .map(|x| {
                x.partial_cmp(&target)
                    .is_some_and(|o| ordering_matches(op, o))
            })
            .collect());
    }
    let col = found.ok_or_else(|| {
        let hint = suggestion(column, t.column_names());
        format!(
            "no column \"{column}\" in {}{hint} (use `backticks` for names with spaces)",
            t.file_name()
        )
    })?;
    let raw = match value {
        Value::Bare(s) | Value::Quoted(s) => s.as_str(),
    };
    let numbers = |v: &[f64], target: f64| -> Vec<bool> {
        v.iter()
            .map(|x| {
                x.partial_cmp(&target)
                    .is_some_and(|o| ordering_matches(op, o))
            })
            .collect()
    };
    match &col.data {
        ColumnData::Number(v) => {
            let target = data::parse_number(raw, t.decimal_comma).ok_or_else(|| {
                format!(
                    "\"{}\" holds numbers, so compare it with a number, not {raw}",
                    col.name
                )
            })?;
            Ok(numbers(v, target))
        }
        ColumnData::Date(v) => {
            let target = data::parse_date(raw).ok_or_else(|| {
                format!(
                    "\"{}\" holds dates, so compare it with a date like 2026-03-01, not {raw}",
                    col.name
                )
            })?;
            Ok(numbers(v, target))
        }
        ColumnData::Text(v) => {
            if !matches!(op, Op::Eq | Op::Ne) {
                return Err(format!(
                    "\"{}\" holds text, which can only be compared with == or !=",
                    col.name
                ));
            }
            Ok(v.iter()
                .map(|cell| !data::is_missing(cell) && ((cell == raw) == (op == Op::Eq)))
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn table() -> Table {
        data::parse(
            "t,temp,site,Voltage (V),day\n1,18,north,1.1,2026-01-01\n2,22,south,1.3,2026-02-01\n3,25,north,,2026-03-01\n",
            Path::new("t.csv"),
        )
        .unwrap()
    }

    fn rows(src: &str) -> Vec<bool> {
        parse(src).unwrap().mask(&table()).unwrap()
    }

    #[test]
    fn comparisons() {
        assert_eq!(rows("temp > 20"), [false, true, true]);
        assert_eq!(rows("temp >= 22 and t < 3"), [false, true, false]);
        assert_eq!(rows("site == 'north'"), [true, false, true]);
        assert_eq!(rows("site != 'north' or temp < 20"), [true, true, false]);
        assert_eq!(rows("not (temp > 20)"), [true, false, false]);
        assert_eq!(rows("`Voltage (V)` > 1.2"), [false, true, false]); // missing never matches
        assert_eq!(rows("day >= 2026-02-01"), [false, true, true]);
        assert_eq!(rows("TEMP = 18"), [true, false, false]);
        assert_eq!(rows("temp/t > 10"), [true, true, false]);
        assert_eq!(rows("(temp - 20) * 2 > 0 and t > 2"), [false, false, true]);
        assert_eq!(rows("(temp + t > 24)"), [false, false, true]);
        assert_eq!(rows("abs(temp - 22) <= 3"), [false, true, true]);
    }

    #[test]
    fn helpful_errors() {
        let err = |s: &str| match parse(s) {
            Err(e) => e,
            Ok(f) => f.mask(&table()).unwrap_err(),
        };
        assert!(err("tmep > 3").contains("did you mean \"temp\""));
        assert!(err("site > 'a'").contains("only be compared with =="));
        assert!(err("temp > hot").contains("compare it with a number"));
        assert!(err("temp > 3 temp").contains("and / or"));
        assert!(err("temp").contains("expected a comparison"));
        assert!(err("(temp > 3").contains("missing )"));
    }
}
