//! Choosing where axis ticks go and how their labels read.
//!
//! Numbers use "nice" steps (1, 2, 2.5 or 5 times a power of ten). Very large or very small
//! values get a shared `×10ⁿ` factor instead of long labels. Dates step by calendar units
//! (seconds up to years) and move the shared part (e.g. the year) into an offset label.

use chrono::{Datelike, NaiveDate, NaiveDateTime};

#[derive(Debug, Clone, PartialEq)]
pub struct Ticks {
    pub major: Vec<f64>,
    pub minor: Vec<f64>,
    pub labels: Vec<String>,
    pub offset: Option<Offset>,
}

/// Text shown once near the axis instead of on every label.
#[derive(Debug, Clone, PartialEq)]
pub enum Offset {
    /// Labels are multiplied by 10^n.
    Power(i32),
    /// Shared context, e.g. the year under month labels.
    Text(String),
}

const MINUS: char = '\u{2212}';

/// Ticks for a number axis from `lo` to `hi`, aiming for about `target` labels.
pub fn linear(lo: f64, hi: f64, target: usize) -> Ticks {
    let span = hi - lo;
    let (mut step, mut mantissa) = nice_step(span / target.max(1) as f64);
    let count = |step: f64| ((hi / step + 1e-9).floor() - (lo / step - 1e-9).ceil()) as i64 + 1;
    // The step is rounded up, so a narrow axis can end up with one tick (range 3.7–34 in
    // thirds: step 20, only "20"). Like matplotlib, keep at least two, stepping down one
    // nice value at a time (… 10 → 5 → 2.5 → 2 → 1 …) so a short axis isn't crowded.
    while count(step) < 2 && span > 0.0 && step.is_finite() {
        (step, mantissa) = if mantissa == 1.0 {
            (step / 2.0, 5.0)
        } else if mantissa == 5.0 {
            (step / 2.0, 2.5)
        } else if mantissa == 2.5 {
            (step * 0.8, 2.0)
        } else {
            (step / 2.0, 1.0)
        };
    }
    let first = (lo / step - 1e-9).ceil();
    let last = (hi / step + 1e-9).floor();
    let major: Vec<f64> = (first as i64..=last as i64)
        .map(|i| clean(i as f64 * step, step))
        .collect();

    let subdivisions = if mantissa == 2.0 { 4 } else { 5 };
    let minor_step = step / f64::from(subdivisions);
    let minor = ((lo / minor_step - 1e-9).ceil() as i64..=(hi / minor_step + 1e-9).floor() as i64)
        .filter(|i| i.rem_euclid(i64::from(subdivisions)) != 0)
        .map(|i| i as f64 * minor_step)
        .collect();
    labelled(major, minor, step, lo, hi)
}

/// Ticks every `step` from `from` (`xlim=0:100:20`: 0, 20, 40…), within `lo`..`hi`. A step
/// that would give more than 100 ticks is widened to whole multiples of itself.
pub fn stepped(lo: f64, hi: f64, step: f64, from: f64) -> Ticks {
    let count = ((hi - lo) / step).abs();
    let step = if count > 100.0 {
        step * (count / 100.0).ceil()
    } else {
        step
    };
    let at = |k: i64| clean(from + k as f64 * step, step);
    let first = ((lo - from) / step - 1e-9).ceil() as i64;
    let last = ((hi - from) / step + 1e-9).floor() as i64;
    let major: Vec<f64> = (first..=last).map(at).collect();
    // Minor ticks split each step like the automatic ones (quarters for 2, 20…; else fifths).
    let mantissa = step / 10f64.powf(step.log10().floor());
    let subdivisions: i64 = if (mantissa - 2.0).abs() < 1e-9 { 4 } else { 5 };
    let minor_step = step / subdivisions as f64;
    let minor = (first * subdivisions - subdivisions..=last * subdivisions + subdivisions)
        .filter(|i| i.rem_euclid(subdivisions) != 0)
        .map(|i| from + i as f64 * minor_step)
        .filter(|v| *v >= lo - 1e-9 * step && *v <= hi + 1e-9 * step)
        .collect();
    labelled(major, minor, step, lo, hi)
}

/// Labels for number ticks: as few decimals as needed, and a shared `×10ⁿ` for very large
/// or very small values.
fn labelled(major: Vec<f64>, minor: Vec<f64>, step: f64, lo: f64, hi: f64) -> Ticks {
    let max_abs = lo.abs().max(hi.abs());
    let power = if max_abs > 0.0 {
        let e = max_abs.log10().floor() as i32;
        if e >= 5 || e <= -4 { e } else { 0 }
    } else {
        0
    };
    let scale = 10f64.powi(power);
    let scaled: Vec<f64> = major.iter().map(|v| v / scale).collect();
    let decimals = decimals_for(&scaled, step / scale);
    let labels = scaled.iter().map(|v| format_number(*v, decimals)).collect();

    Ticks {
        major,
        minor,
        labels,
        offset: (power != 0).then_some(Offset::Power(power)),
    }
}

/// Ticks for a log axis. `lo` and `hi` are powers of ten (log10 of the axis ends); the
/// returned tick values are ordinary numbers (100, not 2). Whole decades get labels, written
/// as plain numbers from 0.001 to 1000 and as 10ⁿ beyond; 2–9 × each decade are minor ticks.
/// A range shorter than one decade falls back to ordinary nice steps.
pub fn log(lo: f64, hi: f64, target: usize) -> Ticks {
    if hi - lo < 1.0 {
        let mut t = linear(10f64.powf(lo), 10f64.powf(hi), target);
        t.major.retain(|v| *v > 0.0);
        t.labels.truncate(t.major.len());
        t.minor.clear();
        return t;
    }
    let first = lo.ceil() as i32;
    let last = hi.floor() as i32;
    let decades = (last - first + 1).max(1);
    let every = ((decades as usize).div_ceil(target.max(1))).max(1) as i32;
    let exps: Vec<i32> = (first..=last)
        .filter(|e| (e - first) % every == 0)
        .collect();
    let major: Vec<f64> = exps.iter().map(|e| 10f64.powi(*e)).collect();
    let plain = exps.iter().all(|e| (-3..=3).contains(e));
    let labels = exps
        .iter()
        .map(|&e| {
            if plain {
                format_number(10f64.powi(e), (-e).max(0) as usize)
            } else {
                format!("$10^{{{}}}$", format_number(f64::from(e), 0))
            }
        })
        .collect();
    let minor = if every == 1 {
        (lo.floor() as i32..=hi.ceil() as i32)
            .flat_map(|e| (2..10).map(move |m| f64::from(m) * 10f64.powi(e)))
            .filter(|v| {
                let l = v.log10();
                l >= lo - 1e-9 && l <= hi + 1e-9
            })
            .collect()
    } else {
        Vec::new()
    };
    Ticks {
        major,
        minor,
        labels,
        offset: None,
    }
}

/// Ticks at the given category positions (0, 1, 2, …), thinned out if there are many.
pub fn categories(names: &[String], max_labels: usize) -> Ticks {
    let every = names.len().div_ceil(max_labels.max(1)).max(1);
    let (major, labels) = names
        .iter()
        .enumerate()
        .filter(|(i, _)| i % every == 0)
        .map(|(i, n)| (i as f64, n.clone()))
        .unzip();
    Ticks {
        major,
        minor: Vec::new(),
        labels,
        offset: None,
    }
}

fn nice_step(raw: f64) -> (f64, f64) {
    let raw = if raw > 0.0 && raw.is_finite() {
        raw
    } else {
        1.0
    };
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if raw / mag <= m * (1.0 + 1e-9) {
            return if m == 10.0 {
                (mag * 10.0, 1.0)
            } else {
                (m * mag, m)
            };
        }
    }
    (mag * 10.0, 1.0)
}

/// Snap values like 0.30000000000000004 and -0.0 to clean numbers.
fn clean(v: f64, step: f64) -> f64 {
    if v.abs() < step * 1e-9 { 0.0 } else { v }
}

/// The fewest decimals that show every tick value exactly.
fn decimals_for(values: &[f64], step: f64) -> usize {
    let start = (-step.log10().floor()).max(0.0) as usize;
    (start..12)
        .find(|&d| {
            let f = 10f64.powi(d as i32);
            values
                .iter()
                .all(|v| ((v * f).round() / f - v).abs() <= step * 1e-6)
        })
        .unwrap_or(start)
}

pub fn format_number(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    // "-0" and "-0.00" read as zero.
    let s = if s
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        s.trim_start_matches('-').to_owned()
    } else {
        s
    };
    s.replace('-', &MINUS.to_string())
}

// ---- dates ----

#[derive(Debug, Clone, Copy, PartialEq)]
enum Unit {
    Second,
    Minute,
    Hour,
    Day,
    Month,
    Year,
}

const DATE_STEPS: &[(Unit, i64, f64)] = &[
    (Unit::Second, 1, 1.0),
    (Unit::Second, 2, 2.0),
    (Unit::Second, 5, 5.0),
    (Unit::Second, 10, 10.0),
    (Unit::Second, 15, 15.0),
    (Unit::Second, 30, 30.0),
    (Unit::Minute, 1, 60.0),
    (Unit::Minute, 2, 120.0),
    (Unit::Minute, 5, 300.0),
    (Unit::Minute, 10, 600.0),
    (Unit::Minute, 15, 900.0),
    (Unit::Minute, 30, 1800.0),
    (Unit::Hour, 1, 3600.0),
    (Unit::Hour, 2, 7200.0),
    (Unit::Hour, 3, 10800.0),
    (Unit::Hour, 6, 21600.0),
    (Unit::Hour, 12, 43200.0),
    (Unit::Day, 1, 86400.0),
    (Unit::Day, 2, 172800.0),
    (Unit::Day, 7, 604800.0),
    (Unit::Day, 14, 1209600.0),
    (Unit::Month, 1, 2629746.0),
    (Unit::Month, 2, 5259492.0),
    (Unit::Month, 3, 7889238.0),
    (Unit::Month, 6, 15778476.0),
    (Unit::Year, 1, 31556952.0),
    (Unit::Year, 2, 63113904.0),
    (Unit::Year, 5, 157784760.0),
    (Unit::Year, 10, 315569520.0),
    (Unit::Year, 20, 631139040.0),
    (Unit::Year, 50, 1577847600.0),
    (Unit::Year, 100, 3155695200.0),
];

/// A date (seconds, as stored) as text: `2026-03-01`, with the time if it has one.
pub fn date_label(secs: f64) -> String {
    let dt = to_dt(secs);
    if dt.time() == chrono::NaiveTime::MIN {
        dt.format("%Y-%m-%d").to_string()
    } else {
        dt.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn to_dt(secs: f64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(secs.floor() as i64, 0)
        .unwrap_or_default()
        .naive_utc()
}

fn to_secs(dt: NaiveDateTime) -> f64 {
    dt.and_utc().timestamp() as f64
}

/// Ticks for a date axis (values in seconds since 1970) from `lo` to `hi`.
pub fn dates(lo: f64, hi: f64, target: usize) -> Ticks {
    let span = (hi - lo).max(1.0);
    let &(unit, step, _) = DATE_STEPS
        .iter()
        .find(|(_, _, secs)| span / secs <= target.max(1) as f64)
        .unwrap_or(DATE_STEPS.last().unwrap());

    let mut major = Vec::new();
    match unit {
        Unit::Second | Unit::Minute | Unit::Hour => {
            let s = match unit {
                Unit::Second => step,
                Unit::Minute => step * 60,
                _ => step * 3600,
            } as f64;
            let mut t = (lo / s).ceil() * s;
            while t <= hi {
                major.push(t);
                t += s;
            }
        }
        Unit::Day => {
            let mut d = to_dt(lo).date();
            let end = to_dt(hi).date();
            while d <= end {
                let ok = (i64::from(d.day()) - 1) % step == 0 && (step < 7 || d.day() <= 29);
                let t = to_secs(d.and_hms_opt(0, 0, 0).unwrap());
                if ok && t >= lo && t <= hi {
                    major.push(t);
                }
                let Some(next) = d.succ_opt() else { break };
                d = next;
            }
        }
        Unit::Month => {
            let start = to_dt(lo).date();
            let (mut y, mut m) = (start.year(), start.month());
            loop {
                let d = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
                let t = to_secs(d.and_hms_opt(0, 0, 0).unwrap());
                if t > hi {
                    break;
                }
                if t >= lo && (i64::from(m) - 1) % step == 0 {
                    major.push(t);
                }
                (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            }
        }
        Unit::Year => {
            let mut y = to_dt(lo).year();
            while let Some(d) = NaiveDate::from_ymd_opt(y, 1, 1) {
                let t = to_secs(d.and_hms_opt(0, 0, 0).unwrap());
                if t > hi {
                    break;
                }
                if t >= lo && i64::from(y) % step == 0 {
                    major.push(t);
                }
                y += 1;
            }
        }
    }

    // Judge "same year/day" from the labelled ticks, not the padded axis ends.
    let first = to_dt(major.first().copied().unwrap_or(lo));
    let last = to_dt(major.last().copied().unwrap_or(hi));
    let same_year = first.year() == last.year();
    let same_day = first.date() == last.date();
    let (fmt, offset) = match unit {
        Unit::Year => ("%Y", None),
        Unit::Month if same_year => ("%b", Some(first.format("%Y").to_string())),
        Unit::Month => ("%b %Y", None),
        Unit::Day if same_year => ("%-d %b", Some(first.format("%Y").to_string())),
        Unit::Day => ("%-d %b %Y", None),
        Unit::Hour | Unit::Minute if same_day => {
            ("%H:%M", Some(first.format("%-d %b %Y").to_string()))
        }
        Unit::Hour | Unit::Minute => ("%-d %b %H:%M", None),
        Unit::Second if same_day => ("%H:%M:%S", Some(first.format("%-d %b %Y").to_string())),
        Unit::Second => ("%-d %b %H:%M:%S", None),
    };
    let labels = major
        .iter()
        .map(|t| to_dt(*t).format(fmt).to_string())
        .collect();
    Ticks {
        major,
        minor: Vec::new(),
        labels,
        offset: offset.map(Offset::Text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_narrow_axis_keeps_two_ticks() {
        // 3.7–34 in thirds rounds the step up to 20: only "20" would be left.
        assert_eq!(linear(3.7, 34.0, 3).major, vec![10.0, 20.0, 30.0]);
        assert_eq!(linear(0.2, 0.9, 3).major.len(), 3);
        assert!(linear(4.1, 4.9, 3).major.len() >= 2);
        // A wide one is left alone.
        assert_eq!(
            linear(0.0, 100.0, 5).major,
            vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]
        );
    }

    #[test]
    fn stepped_ticks_start_at_the_lower_limit() {
        let t = stepped(0.0, 100.0, 20.0, 0.0);
        assert_eq!(t.major, vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        assert_eq!(t.labels, vec!["0", "20", "40", "60", "80", "100"]);
        assert_eq!(t.minor.len(), 15); // quarters of each 20, majors left out
        // From a lower limit that isn't a multiple of the step.
        let t = stepped(3.0, 10.0, 2.5, 3.0);
        assert_eq!(t.major, vec![3.0, 5.5, 8.0]);
        assert_eq!(t.labels, vec!["3.0", "5.5", "8.0"]);
        // Too fine a step is widened instead of drawing thousands of ticks.
        assert!(stepped(0.0, 1000.0, 0.01, 0.0).major.len() <= 101);
    }

    #[test]
    fn nice_linear_ticks() {
        let t = linear(0.0, 19.5, 5);
        assert_eq!(t.labels, ["0", "5", "10", "15"]);
        let t = linear(-1.0, 1.0, 5);
        assert_eq!(
            t.labels,
            ["\u{2212}1.0", "\u{2212}0.5", "0.0", "0.5", "1.0"]
        );
        let t = linear(0.0, 1.0, 4);
        assert_eq!(t.labels, ["0.00", "0.25", "0.50", "0.75", "1.00"]);
        assert!(t.offset.is_none());
    }

    #[test]
    fn minor_ticks_between_majors() {
        let t = linear(0.0, 10.0, 5);
        assert_eq!(t.major, [0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(t.minor.len(), 5 * 3);
        assert!(!t.minor.contains(&2.0));
    }

    #[test]
    fn big_and_small_numbers_use_a_power() {
        let t = linear(0.0, 250_000.0, 5);
        assert_eq!(t.offset, Some(Offset::Power(5)));
        assert_eq!(t.labels[1], "0.5");
        let t = linear(0.0, 0.0004, 4);
        assert_eq!(t.offset, Some(Offset::Power(-4)));
        assert_eq!(t.labels.last().unwrap(), "4");
    }

    #[test]
    fn month_ticks_with_year_offset() {
        let lo = crate::data::parse_date("2026-01-01").unwrap();
        let hi = crate::data::parse_date("2026-12-01").unwrap();
        let t = dates(lo, hi, 6);
        assert_eq!(t.labels[0], "Jan");
        assert_eq!(t.offset, Some(Offset::Text("2026".into())));
        assert!(t.major.len() >= 4 && t.major.len() <= 7, "{:?}", t.labels);
    }

    #[test]
    fn padding_before_january_keeps_the_year_offset() {
        // Bars pad the axis into the previous December; labels should still say "Jan".
        let lo = crate::data::parse_date("2025-12-18").unwrap();
        let hi = crate::data::parse_date("2026-12-14").unwrap();
        let t = dates(lo, hi, 5);
        assert_eq!(t.labels[0], "Jan");
        assert_eq!(t.offset, Some(Offset::Text("2026".into())));
    }

    #[test]
    fn hour_ticks_within_a_day() {
        let lo = crate::data::parse_date("2026-10-21T08:00:00").unwrap();
        let hi = crate::data::parse_date("2026-10-21T11:45:00").unwrap();
        let t = dates(lo, hi, 5);
        assert_eq!(t.labels[0], "08:00");
        assert_eq!(t.offset, Some(Offset::Text("21 Oct 2026".into())));
    }

    #[test]
    fn log_ticks() {
        let t = log(0.0, 3.0, 6);
        assert_eq!(t.major, [1.0, 10.0, 100.0, 1000.0]);
        assert_eq!(t.labels, ["1", "10", "100", "1000"]);
        assert_eq!(t.minor.len(), 8 * 3);
        let t = log(-8.0, 2.0, 5);
        assert_eq!(t.labels[0], "$10^{\u{2212}8}$");
        assert!(t.major.len() <= 6);
        let t = log(0.2, 0.8, 4);
        assert!(
            t.major.iter().all(|v| *v > 1.0 && *v < 7.0),
            "{:?}",
            t.major
        );
    }

    #[test]
    fn categories_thin_out() {
        let names: Vec<String> = (0..30).map(|i| i.to_string()).collect();
        assert_eq!(categories(&names, 10).major.len(), 10);
    }
}
