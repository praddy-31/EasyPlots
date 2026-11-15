//! Reading CSV files into typed columns.
//!
//! Handles the messy parts of real CSVs: the separator (`,` `;` tab `|`) and whether there is
//! a header row are detected; `#` comment lines and blank lines are skipped; `,` is accepted
//! as the decimal mark when the separator is not a comma; empty cells, `NaN`, `NA` and
//! similar become missing values. Each column ends up as numbers, dates or text.

use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};

use crate::Error;

/// A loaded CSV file.
#[derive(Debug, Clone)]
pub struct Table {
    pub path: PathBuf,
    pub columns: Vec<Column>,
    pub rows: usize,
    pub delimiter: u8,
    pub decimal_comma: bool,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub data: ColumnData,
}

impl Column {
    /// A one-line summary for the picker, so messy columns show before plotting:
    /// `12.1 – 31.4 · mean 22 · 3 blanks`, `2024-01-01 – 2024-03-31`, `3 values: a, b, c`.
    pub fn summary(&self) -> String {
        let blanks = |n: usize| match n {
            0 => String::new(),
            1 => " · 1 blank".to_owned(),
            n => format!(" · {n} blanks"),
        };
        match &self.data {
            ColumnData::Number(v) | ColumnData::Date(v) => {
                let ok: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
                if ok.is_empty() {
                    return "all blank".to_owned();
                }
                let lo = ok.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = ok.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let missing = blanks(v.len() - ok.len());
                if matches!(self.data, ColumnData::Date(_)) {
                    let d = crate::render::ticks::date_label;
                    return format!("{} – {}{missing}", d(lo), d(hi));
                }
                let mean = ok.iter().sum::<f64>() / ok.len() as f64;
                format!(
                    "{} – {} · mean {}{missing}",
                    short(lo),
                    short(hi),
                    short(mean)
                )
            }
            ColumnData::Text(v) => {
                let mut seen: Vec<&str> = Vec::new();
                for t in v.iter().filter(|t| !t.trim().is_empty()) {
                    if !seen.contains(&t.as_str()) {
                        seen.push(t);
                    }
                }
                let missing = blanks(v.iter().filter(|t| t.trim().is_empty()).count());
                let examples = seen.iter().take(3).copied().collect::<Vec<_>>().join(", ");
                let more = if seen.len() > 3 { ", …" } else { "" };
                format!("{} values: {examples}{more}{missing}", seen.len())
            }
        }
    }
}

/// Plain statistics of a column's numbers (blanks left out), for `stats` and `--stats`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub n: usize,
    pub mean: f64,
    /// Sample standard deviation (divides by n − 1); 0 for a single value.
    pub sd: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
}

impl Stats {
    /// None when there are no numbers.
    pub fn of(values: &[f64]) -> Option<Stats> {
        let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
        if v.is_empty() {
            return None;
        }
        v.sort_by(f64::total_cmp);
        let n = v.len();
        let mean = v.iter().sum::<f64>() / n as f64;
        let sd = if n > 1 {
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
        } else {
            0.0
        };
        let median = if n % 2 == 1 {
            v[n / 2]
        } else {
            (v[n / 2 - 1] + v[n / 2]) / 2.0
        };
        Some(Stats {
            n,
            mean,
            sd,
            median,
            min: v[0],
            max: v[n - 1],
        })
    }

    /// A value written to the precision the spread supports: the sd to two significant
    /// figures and the others to the same decimal place (101.04 ± 0.31, not 101 ± 0.31).
    pub fn show(&self, v: f64) -> String {
        if !(self.sd > 0.0 && self.sd.is_finite()) || !(1e-4..1e6).contains(&v.abs().max(self.sd)) {
            return short(v);
        }
        let decimals = (1 - self.sd.log10().floor() as i32).max(0) as usize;
        format!("{v:.decimals$}")
    }

    /// For a legend entry: `22.9 ± 3.5, median 23.5, N = 40`.
    pub fn legend_text(&self) -> String {
        format!(
            "{} ± {}, median {}, N = {}",
            self.show(self.mean),
            self.show(self.sd),
            self.show(self.median),
            self.n
        )
    }
}

/// A number to about three significant figures, for summaries.
pub fn short(v: f64) -> String {
    let a = v.abs();
    if a != 0.0 && !(1e-3..1e6).contains(&a) {
        return format!("{v:.2e}");
    }
    let decimals = if a >= 100.0 {
        0
    } else if a >= 10.0 {
        1
    } else if a >= 1.0 {
        2
    } else {
        3
    };
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}

/// Column values. Missing numbers and dates are stored as `NaN`.
#[derive(Debug, Clone)]
pub enum ColumnData {
    Number(Vec<f64>),
    /// Seconds since 1970-01-01 00:00 on the clock as written; times that carry a zone
    /// are first turned into the Mac's own time zone.
    Date(Vec<f64>),
    Text(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Number,
    Date,
    Text,
}

impl Table {
    /// The first `rows` rows as text, one cell per column, for a quick look before
    /// plotting: numbers shortened, dates as dates, blanks empty.
    pub fn peek(&self, rows: usize) -> Vec<Vec<String>> {
        (0..self.rows.min(rows))
            .map(|r| {
                self.columns
                    .iter()
                    .map(|c| match &c.data {
                        ColumnData::Number(v) => match v.get(r) {
                            Some(x) if x.is_finite() => short(*x),
                            _ => String::new(),
                        },
                        ColumnData::Date(v) => match v.get(r) {
                            Some(x) if x.is_finite() => crate::render::ticks::date_label(*x),
                            _ => String::new(),
                        },
                        ColumnData::Text(v) => v.get(r).cloned().unwrap_or_default(),
                    })
                    .collect()
            })
            .collect()
    }
}

impl ColumnData {
    pub fn kind(&self) -> ColumnKind {
        match self {
            ColumnData::Number(_) => ColumnKind::Number,
            ColumnData::Date(_) => ColumnKind::Date,
            ColumnData::Text(_) => ColumnKind::Text,
        }
    }
}

impl ColumnKind {
    pub fn name(self) -> &'static str {
        match self {
            ColumnKind::Number => "numbers",
            ColumnKind::Date => "dates",
            ColumnKind::Text => "text",
        }
    }
}

impl Table {
    /// The file name without its folder, for messages and legends.
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn file_stem(&self) -> String {
        self.path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.file_name())
    }

    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|c| c.name.as_str())
    }
}

/// How a file is read: lines to skip before the header (`skiplines=`), and the sheet of a
/// workbook (`sheet=`, a name or a number from 1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ReadOptions {
    pub skip: usize,
    pub sheet: Option<String>,
}

/// Read and parse a data file: CSV (or TSV, TXT), an Excel workbook or JSON.
pub fn load(path: &Path, opts: &ReadOptions) -> Result<Table, Error> {
    // The built-in sample files (`demo/peak_onset.csv`) never touch the disk.
    if let Some(text) = crate::demo::content(path) {
        return parse_skipping(text, path, opts.skip);
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "xlsx" | "xlsm" | "xls" | "ods") {
        return crate::sheets::load_workbook(path, opts);
    }
    let bytes = std::fs::read(path)
        .map_err(|e| Error::new(format!("cannot read {}: {e}", path.display())))?;
    let text = String::from_utf8_lossy(&bytes);
    if ext == "json" {
        return crate::sheets::load_json(&text, path);
    }
    parse_skipping(&text, path, opts.skip)
}

/// Parse CSV text. `path` is only used for names in messages.
pub fn parse(text: &str, path: &Path) -> Result<Table, Error> {
    parse_skipping(text, path, 0)
}

/// Parse CSV text, ignoring its first `skip` lines (junk above the header). With `skip`
/// 0, junk lines above the header are found by themselves (see `junk_lines`).
pub fn parse_skipping(text: &str, path: &Path, skip: usize) -> Result<Table, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines: Vec<&str> = text
        .lines()
        .skip(skip)
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect();
    let file = path.display();
    if lines.is_empty() {
        return Err(Error::new(format!(
            "{file} has no data{}",
            if skip > 0 {
                format!(" after skipping {skip} lines (is skiplines= too big?)")
            } else {
                String::new()
            }
        )));
    }
    if skip == 0 {
        lines.drain(..junk_lines(&lines));
    }

    let delimiter = detect_delimiter(&lines);
    let cleaned = lines.join("\n");
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .trim(csv::Trim::All)
        .from_reader(cleaned.as_bytes());
    let mut records: Vec<Vec<String>> = Vec::with_capacity(lines.len());
    for record in reader.records() {
        let record = record.map_err(|e| Error::new(format!("{file}: {e}")))?;
        records.push(record.iter().map(str::to_owned).collect());
    }
    from_records(records, path, delimiter, delimiter != b',')
}

/// A table from rows of cells (CSV lines, workbook rows): finds the header, types columns.
pub(crate) fn from_records(
    records: Vec<Vec<String>>,
    path: &Path,
    delimiter: u8,
    decimal_comma: bool,
) -> Result<Table, Error> {
    if records.is_empty() {
        return Err(Error::new(format!("{} has no data", path.display())));
    }
    let width = records.iter().map(Vec::len).max().unwrap_or(0);

    let has_header = !records[0]
        .iter()
        .filter(|f| !is_missing(f))
        .all(|f| parse_number(f, decimal_comma).is_some() || parse_date(f).is_some());
    let (names, body) = if has_header {
        (header_names(&records[0], width), &records[1..])
    } else {
        (
            (1..=width).map(|i| format!("column {i}")).collect(),
            &records[..],
        )
    };

    let columns = names
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let cells: Vec<&str> = body
                .iter()
                .map(|r| r.get(i).map(String::as_str).unwrap_or(""))
                .collect();
            Column {
                name,
                data: type_column(&cells, decimal_comma),
            }
        })
        .collect();

    Ok(Table {
        path: path.to_path_buf(),
        columns,
        rows: body.len(),
        delimiter,
        decimal_comma,
    })
}

/// How many lines at the top are junk above the header (an instrument's name, a date, a
/// title): lines that don't split into the same number of fields as the rest of the file.
/// The first line that does, followed by a few more that do, is the header, but only if it
/// isn't numbers (a header with a different count of fields than the data is kept).
fn junk_lines(lines: &[&str]) -> usize {
    let sample = &lines[..lines.len().min(500)];
    // The separator and field count most lines share.
    let mut best: Option<(u8, usize, usize)> = None;
    for &delim in b"\t;|," {
        let mut tally: Vec<(usize, usize)> = Vec::new();
        for l in sample {
            let c = count_fields(l, delim);
            match tally.iter_mut().find(|t| t.0 == c) {
                Some(t) => t.1 += 1,
                None => tally.push((c, 1)),
            }
        }
        let Some(&(count, freq)) = tally.iter().max_by_key(|t| (t.1, t.0)) else {
            continue;
        };
        if count >= 2 && best.is_none_or(|b| freq > b.2) {
            best = Some((delim, count, freq));
        }
    }
    let Some((delim, count, _)) = best else {
        return 0;
    };
    let fits = |l: &&str| count_fields(l, delim) == count;
    let Some(start) = (0..sample.len()).find(|&i| sample[i..].iter().take(4).all(fits)) else {
        return 0;
    };
    let header_like = sample[start]
        .split(delim as char)
        .map(|f| f.trim().trim_matches('"'))
        .filter(|f| !is_missing(f))
        .any(|f| parse_number(f, delim != b',').is_none() && parse_date(f).is_none());
    if start > 0 && header_like { start } else { 0 }
}

/// The same for rows of cells (a workbook sheet): rows above the header that are filled to
/// a different width than most rows.
pub(crate) fn junk_records(records: &[Vec<String>]) -> usize {
    let sample = &records[..records.len().min(500)];
    let width = |r: &Vec<String>| {
        r.iter()
            .rposition(|c| !c.trim().is_empty())
            .map_or(0, |i| i + 1)
    };
    let mut tally: Vec<(usize, usize)> = Vec::new();
    for r in sample {
        let w = width(r);
        match tally.iter_mut().find(|t| t.0 == w) {
            Some(t) => t.1 += 1,
            None => tally.push((w, 1)),
        }
    }
    let Some(&(count, _)) = tally.iter().max_by_key(|t| (t.1, t.0)) else {
        return 0;
    };
    if count < 2 {
        return 0;
    }
    let Some(start) =
        (0..sample.len()).find(|&i| sample[i..].iter().take(4).all(|r| width(r) == count))
    else {
        return 0;
    };
    let header_like = sample[start]
        .iter()
        .filter(|f| !is_missing(f))
        .any(|f| parse_number(f, false).is_none() && parse_date(f).is_none());
    if start > 0 && header_like { start } else { 0 }
}

/// Pick the separator that splits the first lines into the same number (>1) of fields.
/// Tab and `;` win over `,` when both work, because those files often use `,` for decimals.
fn detect_delimiter(lines: &[&str]) -> u8 {
    let sample = &lines[..lines.len().min(30)];
    let mut best = (b',', 0usize);
    for &delim in b"\t;|," {
        let counts: Vec<usize> = sample.iter().map(|l| count_fields(l, delim)).collect();
        let first = counts[0];
        if first < 2 {
            continue;
        }
        let consistent = counts.iter().filter(|&&c| c == first).count();
        // Most lines must agree with the first (header) line.
        if consistent * 10 >= sample.len() * 9 && consistent > best.1 {
            best = (delim, consistent);
        }
    }
    best.0
}

fn count_fields(line: &str, delim: u8) -> usize {
    let mut in_quotes = false;
    let mut n = 1;
    for b in line.bytes() {
        if b == b'"' {
            in_quotes = !in_quotes;
        } else if b == delim && !in_quotes {
            n += 1;
        }
    }
    n
}

fn header_names(row: &[String], width: usize) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(width);
    for i in 0..width {
        let raw = row.get(i).map(|s| s.trim()).unwrap_or("");
        let base = if raw.is_empty() {
            format!("column {}", i + 1)
        } else {
            raw.to_owned()
        };
        let mut name = base.clone();
        let mut n = 2;
        while names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&name))
        {
            name = format!("{base} ({n})");
            n += 1;
        }
        names.push(name);
    }
    names
}

fn type_column(cells: &[&str], decimal_comma: bool) -> ColumnData {
    let present = || cells.iter().filter(|c| !is_missing(c));
    if present().next().is_none() {
        return ColumnData::Number(vec![f64::NAN; cells.len()]);
    }
    if present().all(|c| parse_number(c, decimal_comma).is_some()) {
        return ColumnData::Number(
            cells
                .iter()
                .map(|c| parse_number(c, decimal_comma).unwrap_or(f64::NAN))
                .collect(),
        );
    }
    if present().all(|c| parse_date(c).is_some()) {
        return ColumnData::Date(
            cells
                .iter()
                .map(|c| parse_date(c).unwrap_or(f64::NAN))
                .collect(),
        );
    }
    ColumnData::Text(cells.iter().map(|c| (*c).to_owned()).collect())
}

const MISSING: &[&str] = &[
    "", "nan", "na", "n/a", "null", "none", "-", "--", "?", "#n/a",
];

pub fn is_missing(cell: &str) -> bool {
    let t = cell.trim();
    MISSING.iter().any(|m| t.eq_ignore_ascii_case(m))
}

/// Parse a number, accepting `,` as the decimal mark when `decimal_comma` is set.
pub fn parse_number(cell: &str, decimal_comma: bool) -> Option<f64> {
    let t = cell.trim();
    if t.is_empty() {
        return None;
    }
    // Reject things Rust accepts but people don't mean as numbers.
    if t.eq_ignore_ascii_case("infinity") || t.eq_ignore_ascii_case("nan") {
        return None;
    }
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    if decimal_comma && t.matches(',').count() == 1 && !t.contains('.') {
        return t.replace(',', ".").parse::<f64>().ok();
    }
    // Thousands marks: 1,234.5 and 12,345 (or 1.234,5 and 1.234.567 in a decimal-comma
    // file). Only strict groups of three count, so 1,23 or 12.34.5 stay text.
    let (group, decimal) = if decimal_comma {
        ('.', ',')
    } else {
        (',', '.')
    };
    if !t.contains(group) {
        return None;
    }
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (whole, frac) = match digits.split_once(decimal) {
        Some((w, f)) => (w, Some(f)),
        None => (digits, None),
    };
    let all_digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    let mut groups = whole.split(group);
    let first = groups.next()?;
    let rest: Vec<&str> = groups.collect();
    let grouped = all_digits(first)
        && first.len() <= 3
        && !rest.is_empty()
        && rest.iter().all(|g| g.len() == 3 && all_digits(g))
        && frac.is_none_or(all_digits)
        // 1.234 in a decimal-comma file already read as 1.234 above; with a comma after
        // it, or two groups, the dots can only be thousands marks.
        && (!decimal_comma || frac.is_some() || rest.len() > 1);
    if !grouped {
        return None;
    }
    let plain: String = t
        .chars()
        .filter(|c| *c != group)
        .map(|c| if c == decimal { '.' } else { c })
        .collect();
    plain.parse::<f64>().ok()
}

/// A time and its zone apart: `10:00:00+02:00` → (`10:00:00`, 7200 s east of UTC),
/// `…Z` → (`…`, 0), no zone → (as it is, None). Zones are `Z`, `±HH`, `±HHMM`, `±HH:MM`.
fn split_zone(t: &str) -> (&str, Option<i32>) {
    if let Some(rest) = t.strip_suffix('Z')
        && rest.contains(':')
    {
        return (rest.trim_end(), Some(0));
    }
    // Only after a time of day: the date's own dashes must not count.
    let Some(colon) = t.find(':') else {
        return (t, None);
    };
    let Some(at) = t[colon..].rfind(['+', '-']).map(|i| colon + i) else {
        return (t, None);
    };
    let zone = &t[at + 1..];
    let digits: String = zone.chars().filter(|c| *c != ':').collect();
    let shape_ok = match zone.len() {
        2 | 4 => zone.bytes().all(|b| b.is_ascii_digit()),
        5 => zone.as_bytes()[2] == b':' && digits.len() == 4,
        _ => false,
    } && digits.bytes().all(|b| b.is_ascii_digit());
    if !shape_ok {
        return (t, None);
    }
    let hours: i32 = digits[..2].parse().unwrap_or(0);
    let minutes: i32 = digits.get(2..).map_or(0, |m| m.parse().unwrap_or(0));
    let sign = if t.as_bytes()[at] == b'-' { -1 } else { 1 };
    (
        t[..at].trim_end(),
        Some(sign * (hours * 3600 + minutes * 60)),
    )
}

/// The Mac's own clock time for a moment given in UTC (summer time by date, from the
/// system's time zone; `TZ` overrides it, as for any program).
fn mac_clock(utc: NaiveDateTime) -> NaiveDateTime {
    use chrono::TimeZone;
    chrono::Local.from_utc_datetime(&utc).naive_local()
}

const DATETIME_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d %H:%M",
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%d.%m.%Y %H:%M:%S",
    "%d.%m.%Y %H:%M",
];
const DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%Y/%m/%d", "%d.%m.%Y"];

/// Parse an ISO-style date or date-time into seconds since 1970. A time with a zone at
/// the end (`Z`, `+02:00`, `-0500`) is shown in the Mac's own time zone (owner,
/// 2026-11-15), so files from different zones line up; a time without one counts as
/// written.
pub fn parse_date(cell: &str) -> Option<f64> {
    parse_date_in(cell, mac_clock)
}

/// `parse_date`, turning zoned times into local ones with `local` (tests use a fixed zone).
fn parse_date_in(cell: &str, local: impl Fn(NaiveDateTime) -> NaiveDateTime) -> Option<f64> {
    let (t, zone) = split_zone(cell.trim());
    // Cheap guard: every accepted format starts with a digit and has a separator.
    if !t.starts_with(|c: char| c.is_ascii_digit()) || t.len() < 8 {
        return None;
    }
    if let Some(east) = zone {
        let dt = DATETIME_FORMATS
            .iter()
            .find_map(|f| NaiveDateTime::parse_from_str(t, f).ok())?;
        let utc = dt - chrono::TimeDelta::seconds(i64::from(east));
        return Some(seconds(local(utc)));
    }
    for f in DATETIME_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(t, f) {
            return Some(seconds(dt));
        }
    }
    for f in DATE_FORMATS {
        if let Ok(d) = NaiveDate::parse_from_str(t, f) {
            return Some(seconds(d.and_hms_opt(0, 0, 0)?));
        }
    }
    None
}

fn seconds(dt: NaiveDateTime) -> f64 {
    let utc = dt.and_utc();
    utc.timestamp() as f64 + f64::from(utc.timestamp_subsec_nanos()) * 1e-9
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_summaries() {
        let t = parse(
            "t,temp,site,when\n1,12.13,north,2024-01-01\n2,,south,2024-03-31\n3,31.4,north,\n",
            Path::new("t.csv"),
        )
        .unwrap();
        let s: Vec<String> = t.columns.iter().map(Column::summary).collect();
        assert_eq!(s[0], "1 – 3 · mean 2");
        assert_eq!(s[1], "12.1 – 31.4 · mean 21.8 · 1 blank");
        assert_eq!(s[2], "2 values: north, south");
        assert_eq!(s[3], "2024-01-01 – 2024-03-31 · 1 blank");
    }

    #[test]
    fn statistics() {
        let s = Stats::of(&[1.0, 2.0, f64::NAN, 3.0, 4.0]).unwrap();
        assert_eq!(
            (s.n, s.mean, s.median, s.min, s.max),
            (4, 2.5, 2.5, 1.0, 4.0)
        );
        assert!((s.sd - 1.290_994).abs() < 1e-6);
        assert_eq!(s.legend_text(), "2.5 ± 1.3, median 2.5, N = 4");
        let p = Stats::of(&[101.0, 101.3, 100.8, 101.1]).unwrap();
        assert_eq!(p.show(p.mean), "101.05");
        assert!(Stats::of(&[f64::NAN]).is_none());
    }

    #[test]
    fn junk_above_the_header_is_found() {
        let text = "Instrument: Logger 850\nDate: 2024-05-01\nOperator,Jo,Smith,x\n\
                    T,storage,loss\n20,1.5,0.1\n30,1.4,0.2\n40,1.2,0.3\n50,1.0,0.3\n";
        let t = parse(text, Path::new("heating.csv")).unwrap();
        assert_eq!(
            t.column_names().collect::<Vec<_>>(),
            ["T", "storage", "loss"]
        );
        assert_eq!(t.rows, 4);
        // A header with fewer fields than the data (trailing commas) stays the header.
        let t = parse("a,b\n1,2,\n3,4,\n5,6,\n7,8,\n", Path::new("t.csv")).unwrap();
        assert_eq!(t.columns[0].name, "a");
        assert_eq!(t.rows, 4);
        // No header at all: nothing is left out.
        let t = parse("x\n1,2\n3,4\n5,6\n7,8\n", Path::new("t.csv")).unwrap();
        assert_eq!(t.rows, 4);
        // Too big a skip says so.
        let e = parse_skipping("a,b\n1,2\n", Path::new("t.csv"), 5).unwrap_err();
        assert!(e.to_string().contains("skiplines="), "{e}");
    }

    fn fixture(name: &str) -> Table {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name);
        load(&path, &ReadOptions::default()).unwrap()
    }

    fn numbers(t: &Table, i: usize) -> &[f64] {
        match &t.columns[i].data {
            ColumnData::Number(v) => v,
            other => panic!("column {i} is {:?}", other.kind()),
        }
    }

    #[test]
    fn simple_comma_file() {
        let t = fixture("simple.csv");
        assert_eq!(t.delimiter, b',');
        assert_eq!(
            t.column_names().collect::<Vec<_>>(),
            ["time", "temp", "pressure"]
        );
        assert_eq!(t.rows, 40);
        assert_eq!(numbers(&t, 0)[3], 1.5);
    }

    #[test]
    fn thousands_marks_and_time_zones() {
        assert_eq!(parse_number("1,234.5", false), Some(1234.5));
        assert_eq!(parse_number("12,345", false), Some(12345.0));
        assert_eq!(parse_number("-1,234,567.89", false), Some(-1234567.89));
        assert_eq!(parse_number("1,23", false), None);
        assert_eq!(parse_number("1,2345", false), None);
        assert_eq!(parse_number("a,123", false), None);
        assert_eq!(parse_number("1.234,5", true), Some(1234.5));
        assert_eq!(parse_number("1.234.567", true), Some(1234567.0));
        // Ambiguous, so it stays what it was: one point two three four.
        assert_eq!(parse_number("1.234", true), Some(1.234));
        assert_eq!(parse_number("3,5", true), Some(3.5));
        // Zoned times move to the Mac's clock: here a Mac one hour east of UTC.
        let east1 = |utc: NaiveDateTime| utc + chrono::TimeDelta::hours(1);
        let at = |day: &str, hm: &str| parse_date(&format!("2024-03-{day}T{hm}:00")).unwrap();
        for (zoned, day, local) in [
            ("2024-03-05T10:00:00Z", "05", "11:00"),
            ("2024-03-05T10:00:00+02:00", "05", "09:00"),
            ("2024-03-05T10:00:00+01", "05", "10:00"),
            ("2024-03-05T10:00:00-0500", "05", "16:00"),
            ("2024-03-05 10:00:00 +0530", "05", "05:30"),
            ("2024-03-05T23:30Z", "06", "00:30"),
        ] {
            assert_eq!(parse_date_in(zoned, east1), Some(at(day, local)), "{zoned}");
        }
        // Without a zone: as written, whatever the Mac's zone.
        assert_eq!(
            parse_date_in("2024-03-05T10:00:00", east1),
            Some(at("05", "10:00"))
        );
        // The real Mac clock: summer time comes from the system's zone database.
        let utc10 = NaiveDate::from_ymd_opt(2024, 3, 5)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap();
        assert_eq!(
            parse_date("2024-03-05T10:00:00Z"),
            Some(seconds(mac_clock(utc10)))
        );
        // The fixture: four zones, one hour apart once on one clock; amounts with commas.
        let t = load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/formats.csv"),
            &ReadOptions::default(),
        )
        .unwrap();
        let (ColumnData::Date(when), ColumnData::Number(amount)) =
            (&t.columns[0].data, &t.columns[1].data)
        else {
            panic!(
                "formats.csv: {:?}",
                t.columns.iter().map(|c| &c.name).collect::<Vec<_>>()
            );
        };
        assert!(
            when.windows(2).all(|w| (w[1] - w[0] - 3600.0).abs() < 1.0),
            "{when:?}"
        );
        assert_eq!(amount[0], 1234.5);
        // A plain date's dashes are not a zone.
        assert!(parse_date("2024-03-05").is_some());
        assert_eq!(parse_date("2024-03-05T10:00:00+2"), None);
    }

    #[test]
    fn semicolon_with_decimal_comma() {
        let t = fixture("semicolon_decimal_comma.csv");
        assert_eq!(t.delimiter, b';');
        assert_eq!(t.columns[1].name, "Temperatur");
        assert!((numbers(&t, 0)[1] - 0.5).abs() < 1e-12);
        assert!(numbers(&t, 1)[0] > 19.0);
    }

    #[test]
    fn tab_separated() {
        let t = fixture("tabs.csv");
        assert_eq!(t.delimiter, b'\t');
        assert_eq!(t.columns.len(), 3);
    }

    #[test]
    fn dates_and_datetimes() {
        let t = fixture("dates.csv");
        assert_eq!(t.columns[0].data.kind(), ColumnKind::Date);
        let t = fixture("datetimes.csv");
        let ColumnData::Date(d) = &t.columns[0].data else {
            panic!()
        };
        assert_eq!(d[1] - d[0], 15.0 * 60.0);
    }

    #[test]
    fn comments_missing_and_quoted_header() {
        let t = fixture("missing_and_comments.csv");
        assert_eq!(t.columns[1].name, "Voltage (V)");
        assert_eq!(t.rows, 6);
        let v = numbers(&t, 1);
        assert!(v[2].is_nan());
        let c = numbers(&t, 2);
        assert!(c[1].is_nan() && c[3].is_nan());
        assert_eq!(c[5], 17.0);
    }

    #[test]
    fn text_column() {
        let t = fixture("text_categories.csv");
        assert_eq!(t.columns[0].data.kind(), ColumnKind::Text);
        assert_eq!(t.columns[1].data.kind(), ColumnKind::Number);
    }

    #[test]
    fn no_header_gets_numbered_names() {
        let t = parse("1,2\n3,4\n", Path::new("x.csv")).unwrap();
        assert_eq!(t.columns[0].name, "column 1");
        assert_eq!(t.rows, 2);
    }

    #[test]
    fn duplicate_and_empty_header_names() {
        let t = parse("a,A,\n1,2,3\n", Path::new("x.csv")).unwrap();
        let names: Vec<_> = t.column_names().collect();
        assert_eq!(names, ["a", "A (2)", "column 3"]);
    }

    #[test]
    fn single_column() {
        let t = parse("value\n1\n2\n", Path::new("x.csv")).unwrap();
        assert_eq!(t.columns.len(), 1);
    }

    #[test]
    fn empty_file_is_an_error() {
        assert!(parse("# only a comment\n\n", Path::new("x.csv")).is_err());
    }
}
