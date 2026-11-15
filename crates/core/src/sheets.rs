//! Excel workbooks (`.xlsx`, `.xls`, `.ods`, read with `calamine`) and JSON files, turned
//! into the same tables as CSV files: the first row is the header unless it holds only
//! numbers or dates, and each column is typed the same way.
//!
//! Workbooks: the first sheet unless `sheet=` names one (by name or number from 1);
//! `skiplines=` ignores rows above the header. Cells are read as values (formulas as their last
//! result); Excel dates become dates.
//!
//! JSON: an array of objects (`[{"t": 1, "temp": 20}, …]`, one row each), an object of
//! arrays (`{"t": [1, 2], "temp": [20, 21]}`, one column each), an array of arrays (rows,
//! the first may be the header), or an object holding one of these (`{"data": […]}`).

use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};
use serde_json::Value;

use crate::Error;
use crate::data::{ReadOptions, Table, from_records};

/// The sheet names of a workbook, for the picker's file list.
pub fn sheet_names(path: &Path) -> Vec<String> {
    open_workbook_auto(path)
        .map(|wb| wb.sheet_names().to_vec())
        .unwrap_or_default()
}

pub fn load_workbook(path: &Path, opts: &ReadOptions) -> Result<Table, Error> {
    let file = path.display();
    let mut wb =
        open_workbook_auto(path).map_err(|e| Error::new(format!("cannot read {file}: {e}")))?;
    let names = wb.sheet_names().to_vec();
    let name = match &opts.sheet {
        None => names
            .first()
            .cloned()
            .ok_or_else(|| Error::new(format!("{file} has no sheets")))?,
        Some(want) => pick_sheet(&names, want).ok_or_else(|| {
            Error::new(format!(
                "no sheet \"{want}\" in {file}{} (sheets: {})",
                crate::error::suggestion(want, names.iter().map(String::as_str)),
                names.join(", ")
            ))
        })?,
    };
    let range = wb
        .worksheet_range(&name)
        .map_err(|e| Error::new(format!("{file}, sheet {name}: {e}")))?;
    let mut records: Vec<Vec<String>> = range
        .rows()
        .skip(opts.skip)
        .map(|row| row.iter().map(cell_text).collect())
        .filter(|row: &Vec<String>| row.iter().any(|c| !c.is_empty()))
        .collect();
    if opts.skip == 0 {
        records.drain(..crate::data::junk_records(&records));
    }
    from_records(records, path, b',', false)
        .map_err(|e| Error::new(format!("{e} (sheet {name})", e = e.message)))
}

/// A sheet by name (any case) or by number from 1.
fn pick_sheet(names: &[String], want: &str) -> Option<String> {
    if let Some(n) = want.parse::<usize>().ok().filter(|n| *n >= 1) {
        return names.get(n - 1).cloned();
    }
    names.iter().find(|n| n.eq_ignore_ascii_case(want)).cloned()
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty | Data::Error(_) => String::new(),
        Data::String(s) => s.trim().to_owned(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => number_text(*f),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d
            .as_datetime()
            .map(|dt| {
                if dt.time() == chrono::NaiveTime::MIN {
                    dt.format("%Y-%m-%d").to_string()
                } else {
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                }
            })
            .unwrap_or_else(|| number_text(d.as_f64())),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
    }
}

fn number_text(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

pub fn load_json(text: &str, path: &Path) -> Result<Table, Error> {
    let file = path.display();
    let value: Value = serde_json::from_str(text)
        .map_err(|e| Error::new(format!("{file} is not valid JSON: {e}")))?;
    let records = records(&value).ok_or_else(|| {
        Error::new(format!(
            "{file}: expected rows ([{{\"t\": 1, …}}, …]), columns ({{\"t\": [1, 2], …}}) or [[…], …]"
        ))
    })?;
    from_records(records, path, b',', false)
}

fn records(value: &Value) -> Option<Vec<Vec<String>>> {
    match value {
        // [{"t": 1, "temp": 20}, …]: one row per object; columns in order of first use.
        Value::Array(rows) if rows.iter().all(Value::is_object) && !rows.is_empty() => {
            let mut names: Vec<String> = Vec::new();
            for row in rows {
                for k in row.as_object()?.keys() {
                    if !names.contains(k) {
                        names.push(k.clone());
                    }
                }
            }
            let mut out = vec![names.clone()];
            for row in rows {
                let o = row.as_object()?;
                out.push(
                    names
                        .iter()
                        .map(|n| o.get(n).map_or_else(String::new, text))
                        .collect(),
                );
            }
            Some(out)
        }
        // [[…], …]: rows as they are.
        Value::Array(rows) if rows.iter().all(Value::is_array) && !rows.is_empty() => Some(
            rows.iter()
                .map(|r| {
                    r.as_array()
                        .map(|r| r.iter().map(text).collect())
                        .unwrap_or_default()
                })
                .collect(),
        ),
        Value::Object(map) if !map.is_empty() => {
            // {"t": [1, 2], "temp": [20, 21]}: one column per key.
            if map.values().all(|v| {
                v.as_array()
                    .is_some_and(|a| a.iter().all(|c| !c.is_array() && !c.is_object()))
            }) {
                let len = map
                    .values()
                    .filter_map(Value::as_array)
                    .map(Vec::len)
                    .max()?;
                let mut out = vec![map.keys().cloned().collect::<Vec<_>>()];
                for i in 0..len {
                    out.push(
                        map.values()
                            .map(|v| {
                                v.as_array()
                                    .and_then(|a| a.get(i))
                                    .map_or_else(String::new, text)
                            })
                            .collect(),
                    );
                }
                return Some(out);
            }
            // {"data": […]}: the one value that holds rows.
            let inner: Vec<&Value> = map
                .values()
                .filter(|v| v.is_array() || v.is_object())
                .collect();
            match inner.as_slice() {
                [one] => records(one),
                _ => None,
            }
        }
        _ => None,
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(json: &str) -> Table {
        load_json(json, Path::new("t.json")).unwrap()
    }

    #[test]
    fn json_shapes() {
        let rows = table(
            r#"[{"t": 1, "temp": 20.5}, {"t": 2, "temp": null}, {"t": 3, "temp": 22, "note": "x"}]"#,
        );
        let names: Vec<&str> = rows.column_names().collect();
        assert_eq!(names, ["t", "temp", "note"]);
        assert_eq!(rows.rows, 3);
        let cols = table(r#"{"t": [1, 2, 3], "temp": [20, 21, 22]}"#);
        assert_eq!(cols.column_names().collect::<Vec<_>>(), ["t", "temp"]);
        assert_eq!(cols.rows, 3);
        let nested = table(r#"{"meta": "run 3", "data": [["t", "temp"], [1, 20], [2, 21]]}"#);
        assert_eq!(nested.column_names().collect::<Vec<_>>(), ["t", "temp"]);
        assert!(load_json("[1, 2]", Path::new("t.json")).is_err());
    }

    #[test]
    fn sheets_by_name_or_number() {
        let names = vec!["Summary".to_owned(), "Run2".to_owned()];
        assert_eq!(pick_sheet(&names, "run2").as_deref(), Some("Run2"));
        assert_eq!(pick_sheet(&names, "1").as_deref(), Some("Summary"));
        assert_eq!(pick_sheet(&names, "3"), None);
    }
}
