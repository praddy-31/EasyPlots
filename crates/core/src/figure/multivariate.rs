//! Plots of several variables together: stacking series, grids for heatmaps and contours,
//! and the reference values a second axis is lined up with.

use super::*;

/// `stacked`: bars at the same x, and areas, sit on top of each other.
pub(super) fn stack(series: &mut [Series]) -> Result<(), Error> {
    let mut tops: HashMap<u64, f64> = HashMap::new();
    for s in series.iter_mut().filter(|s| s.kind == PlotKind::Bar) {
        let mut base = Vec::with_capacity(s.y.len());
        for (x, y) in s.x.iter().zip(&mut s.y) {
            let top = tops.entry(x.to_bits()).or_insert(0.0);
            base.push(*top);
            if y.is_finite() {
                *y += *top;
                *top = *y;
            }
        }
        s.base = Some(base);
    }
    let mut below: Option<(Vec<f64>, Vec<f64>)> = None;
    for s in series.iter_mut().filter(|s| s.kind == PlotKind::Area) {
        match &mut below {
            None => {
                s.base = Some(vec![0.0; s.y.len()]);
                below = Some((s.x.clone(), s.y.clone()));
            }
            Some((xs, tops)) => {
                if *xs != s.x {
                    return Err(Error::new(
                        "stacked areas need the same x values in every column",
                    ));
                }
                s.base = Some(tops.clone());
                for (y, t) in s.y.iter_mut().zip(tops.iter_mut()) {
                    let v = if y.is_finite() { *y } else { 0.0 };
                    *y = *t + v;
                    *t = *y;
                }
            }
        }
    }
    Ok(())
}

/// A heatmap's or contour's grid, with the names for its axes.
pub(super) struct GridLayer {
    pub(super) grid: Grid,
    pub(super) xlabels: Vec<String>,
    pub(super) ylabels: Vec<String>,
    pub(super) xname: String,
    pub(super) yname: String,
    pub(super) zname: String,
}

/// A label for a value used as a heatmap row or column name.
pub(super) fn value_label(kind: ColumnKind, v: f64) -> String {
    if kind == ColumnKind::Date {
        crate::render::ticks::date_label(v)
    } else {
        format!("{v}")
    }
}

/// Read the grid of a heatmap or contour layer, in one of two shapes:
/// - long: `x=COL y=COL z=COL`, one row per point (values at the same x and y are averaged);
/// - a table: every number column is a grid column, every row a grid row; the first column
///   names the rows if it holds text or has an empty header (`y=a,b,c` picks the columns,
///   `x=` the column that names the rows).
pub(super) fn grid_layer(
    layer: &LayerSpec,
    table: &Table,
    keep: &[usize],
) -> Result<GridLayer, Error> {
    let contour = layer.kind == PlotKind::Contour;
    let span = layer.file.span.clone();
    if let Some(zname) = &layer.z {
        let (Some(xname), [yname]) = (&layer.x, layer.y.as_slice()) else {
            return Err(Error::at(
                "with z=, give one x and one y column too: x=COL y=COL z=COL",
                zname.span.clone(),
            ));
        };
        let (_, z) = numbers(table, &column_or_calc(table, zname)?, &zname.span)?;
        // Each axis: its distinct values in order, and every row's place among them.
        let axis = |name: &Spanned<String>, up: bool| -> Result<GridAxis, Error> {
            let col = column_or_calc(table, name)?;
            let (label, kind, values, text) = match col {
                Col::Calc { name, values } => (name, ColumnKind::Number, values, None),
                Col::Index(i) => {
                    let c = &table.columns[i];
                    match &c.data {
                        ColumnData::Number(v) => {
                            (c.name.clone(), ColumnKind::Number, v.clone(), None)
                        }
                        ColumnData::Date(v) => (c.name.clone(), ColumnKind::Date, v.clone(), None),
                        ColumnData::Text(v) => (
                            c.name.clone(),
                            ColumnKind::Text,
                            Vec::new(),
                            Some(v.clone()),
                        ),
                    }
                }
            };
            if let Some(text) = text {
                if contour {
                    return Err(Error::at(
                        format!(
                            "a contour needs numbers in \"{label}\" (it holds text) — try heatmap"
                        ),
                        name.span.clone(),
                    ));
                }
                let mut names: Vec<String> = Vec::new();
                let idx: Vec<usize> = keep
                    .iter()
                    .map(|&r| {
                        names.iter().position(|n| *n == text[r]).unwrap_or_else(|| {
                            names.push(text[r].clone());
                            names.len() - 1
                        })
                    })
                    .collect();
                let n = names.len();
                // Text rows read from the top, like a table.
                let (idx, names) = if up {
                    (
                        idx.iter().map(|i| n - 1 - i).collect(),
                        names.into_iter().rev().collect(),
                    )
                } else {
                    (idx, names)
                };
                return Ok(((0..n).map(|i| i as f64).collect(), names, idx, label));
            }
            let mut distinct: Vec<f64> = keep
                .iter()
                .map(|&r| values[r])
                .filter(|v| v.is_finite())
                .collect();
            distinct.sort_by(f64::total_cmp);
            distinct.dedup();
            let idx = keep
                .iter()
                .map(|&r| {
                    distinct
                        .binary_search_by(|d| d.total_cmp(&values[r]))
                        .unwrap_or(usize::MAX)
                })
                .collect();
            let labels = distinct.iter().map(|v| value_label(kind, *v)).collect();
            let n = distinct.len();
            let pos = if contour {
                distinct
            } else {
                (0..n).map(|i| i as f64).collect()
            };
            Ok((pos, labels, idx, label))
        };
        let (xs, xlabels, xi, xn) = axis(xname, false)?;
        let (ys, ylabels, yi, yn) = axis(yname, true)?;
        if xs.len() < 2 || ys.len() < 2 {
            return Err(Error::at(
                "a grid needs at least two different x and two different y values",
                span,
            ));
        }
        let (nx, ny) = (xs.len(), ys.len());
        let mut sum = vec![0.0; nx * ny];
        let mut count = vec![0.0; nx * ny];
        for (k, &r) in keep.iter().enumerate() {
            let (i, j) = (xi[k], yi[k]);
            if i == usize::MAX || j == usize::MAX || !z[r].is_finite() {
                continue;
            }
            sum[j * nx + i] += z[r];
            count[j * nx + i] += 1.0;
        }
        let z = sum
            .iter()
            .zip(&count)
            .map(|(s, c)| if *c > 0.0 { s / c } else { f64::NAN })
            .collect();
        return Ok(GridLayer {
            grid: Grid { xs, ys, z },
            xlabels,
            ylabels,
            xname: xn,
            yname: yn,
            zname: zname.value.clone(),
        });
    }

    // A table of values.
    let names: Vec<&str> = table.column_names().collect();
    let generated = names.iter().all(|n| n.starts_with("column "));
    let label_col: Option<usize> = match &layer.x {
        Some(name) => Some(column(table, name)?),
        None => {
            let first_text = table
                .columns
                .first()
                .is_some_and(|c| c.data.kind() == ColumnKind::Text);
            // Row names: text, an empty first header cell, or a header of x values
            // ("y, 0, 0.5, 1…") above a first column of y values.
            let numeric = |n: &&str| n.trim().parse::<f64>().is_ok();
            let header_of_numbers = !generated
                && names.len() > 2
                && !numeric(&names[0])
                && names[1..].iter().all(numeric);
            (first_text || header_of_numbers || (!generated && names.first() == Some(&"column 1")))
                .then_some(0)
        }
    };
    let value_cols: Vec<usize> = if layer.y.is_empty() {
        (0..table.columns.len())
            .filter(|i| Some(*i) != label_col)
            .collect()
    } else {
        layer
            .y
            .iter()
            .map(|n| column(table, n))
            .collect::<Result<_, _>>()?
    };
    let mut cols: Vec<(String, Vec<f64>)> = Vec::new();
    for &i in &value_cols {
        let c = &table.columns[i];
        match &c.data {
            ColumnData::Number(v) => {
                cols.push((c.name.clone(), keep.iter().map(|&r| v[r]).collect()))
            }
            _ => {
                return Err(Error::at(
                    format!(
                        "column \"{}\" of {} isn't numbers — a heatmap or contour of a table needs numbers in every column but the first",
                        c.name,
                        table.file_name()
                    ),
                    span,
                ));
            }
        }
    }
    if cols.len() < 2 || keep.len() < 2 {
        return Err(Error::at(
            format!(
                "{} needs at least two rows and two number columns for a grid (or give x=, y= and z=)",
                table.file_name()
            ),
            span,
        ));
    }
    let row_labels: Vec<String> = match label_col.map(|i| &table.columns[i].data) {
        Some(ColumnData::Text(v)) => keep.iter().map(|&r| v[r].clone()).collect(),
        Some(ColumnData::Number(v)) => keep
            .iter()
            .map(|&r| value_label(ColumnKind::Number, v[r]))
            .collect(),
        Some(ColumnData::Date(v)) => keep
            .iter()
            .map(|&r| value_label(ColumnKind::Date, v[r]))
            .collect(),
        None => keep.iter().map(|r| (r + 1).to_string()).collect(),
    };
    let (nx, ny) = (cols.len(), keep.len());
    if contour {
        // Real coordinates where the names and the first column are numbers.
        let mut xs: Vec<(f64, usize)> = cols
            .iter()
            .enumerate()
            .map(|(i, (n, _))| (n.trim().parse::<f64>().unwrap_or(i as f64), i))
            .collect();
        if cols.iter().any(|(n, _)| n.trim().parse::<f64>().is_err()) {
            xs = (0..nx).map(|i| (i as f64, i)).collect();
        }
        let yv: Vec<f64> = match label_col.map(|i| &table.columns[i].data) {
            Some(ColumnData::Number(v)) => keep.iter().map(|&r| v[r]).collect(),
            _ => (0..ny).map(|k| k as f64).collect(),
        };
        let mut ys: Vec<(f64, usize)> = yv.iter().copied().zip(0..ny).collect();
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        ys.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut z = Vec::with_capacity(nx * ny);
        for &(_, row) in &ys {
            for &(_, col) in &xs {
                z.push(cols[col].1[row]);
            }
        }
        return Ok(GridLayer {
            grid: Grid {
                xs: xs.iter().map(|p| p.0).collect(),
                ys: ys.iter().map(|p| p.0).collect(),
                z,
            },
            xlabels: Vec::new(),
            ylabels: Vec::new(),
            xname: String::new(),
            yname: label_col
                .map(|i| table.columns[i].name.clone())
                .unwrap_or_default(),
            zname: String::new(),
        });
    }
    // Heatmap: the first row on top.
    let mut z = Vec::with_capacity(nx * ny);
    for j in 0..ny {
        let row = ny - 1 - j;
        for (_, v) in &cols {
            z.push(v[row]);
        }
    }
    Ok(GridLayer {
        grid: Grid {
            xs: (0..nx).map(|i| i as f64).collect(),
            ys: (0..ny).map(|j| j as f64).collect(),
            z,
        },
        xlabels: cols.iter().map(|(n, _)| n.clone()).collect(),
        ylabels: row_labels.into_iter().rev().collect(),
        xname: String::new(),
        yname: String::new(),
        zname: String::new(),
    })
}

/// Reference lines and regions after the second axes are sorted out.
pub(super) struct Refs {
    pub(super) hlines: Vec<f64>,
    pub(super) vlines: Vec<f64>,
    pub(super) yspans: Vec<(f64, f64)>,
    pub(super) hlines2: Vec<f64>,
    pub(super) yspans2: Vec<(f64, f64)>,
    pub(super) labels: Vec<RefLabel>,
}

/// `hline2=` / `yspan2=` / `vline2=`: lines in the second axes' units. Through a formula
/// axis (`y2=`, `x2=`) they become main-axis values; with `axis=right` columns the right
/// axis's own scale places them when drawing.
pub(super) fn second_axis_refs(
    spec: &PanelSpec,
    series: &[Series],
    x2: Option<&TopAxis>,
    y2conv: Option<&TopAxis>,
) -> Result<Refs, Error> {
    let mut r = Refs {
        hlines: spec.hlines.clone(),
        vlines: spec.vlines.clone(),
        yspans: spec.yspans.clone(),
        hlines2: Vec::new(),
        yspans2: Vec::new(),
        labels: spec
            .ref_labels
            .iter()
            .filter(|l| !l.second)
            .cloned()
            .collect(),
    };
    let has_vertical = !spec.hlines2.is_empty() || !spec.yspans2.is_empty();
    if spec.vlines2.is_empty() && !has_vertical {
        return Ok(r);
    }
    // The data's extent, widened, to search for the main-axis value of a second-axis one.
    let extent = |vals: Vec<f64>, lim: (Option<f64>, Option<f64>)| {
        let lo = lim
            .0
            .unwrap_or_else(|| vals.iter().copied().fold(f64::INFINITY, f64::min));
        let hi = lim
            .1
            .unwrap_or_else(|| vals.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let pad = (hi - lo).abs() * 0.5;
        (lo - pad, hi + pad)
    };
    let finite = |v: &f64| v.is_finite();
    let to_main = |axis: &TopAxis, v: f64, (lo, hi): (f64, f64), what: &str| {
        axis.inverse(v, lo, hi).ok_or_else(|| {
            Error::new(format!(
                "{what}={v} is outside the second axis's range for this data"
            ))
        })
    };
    if !spec.vlines2.is_empty() {
        let Some(axis) = x2 else {
            return Err(Error::new(
                "vline2= is in top-axis units: add a top axis, e.g. x2=\"1240/x\"",
            ));
        };
        let range = extent(
            series
                .iter()
                .flat_map(|s| s.x.iter().copied())
                .filter(finite)
                .collect(),
            spec.xlim,
        );
        for v in &spec.vlines2 {
            r.vlines.push(to_main(axis, *v, range, "vline2")?);
        }
        for l in spec
            .ref_labels
            .iter()
            .filter(|l| l.second && l.kind == RefKind::VLine)
        {
            let at = to_main(axis, l.at.0, range, "vline2")?;
            r.labels.push(RefLabel {
                at: (at, at),
                second: false,
                ..l.clone()
            });
        }
    }
    if !has_vertical {
        return Ok(r);
    }
    let right_labels = spec
        .ref_labels
        .iter()
        .filter(|l| l.second && l.kind != RefKind::VLine);
    if let Some(axis) = y2conv {
        let range = extent(
            series
                .iter()
                .flat_map(|s| s.y.iter().copied())
                .filter(finite)
                .collect(),
            spec.ylim,
        );
        for v in &spec.hlines2 {
            r.hlines.push(to_main(axis, *v, range, "hline2")?);
        }
        for (a, b) in &spec.yspans2 {
            r.yspans.push((
                to_main(axis, *a, range, "yspan2")?,
                to_main(axis, *b, range, "yspan2")?,
            ));
        }
        for l in right_labels {
            r.labels.push(RefLabel {
                at: (
                    to_main(axis, l.at.0, range, "hline2")?,
                    to_main(axis, l.at.1, range, "hline2")?,
                ),
                second: false,
                ..l.clone()
            });
        }
    } else if series.iter().any(|s| s.right) {
        r.hlines2 = spec.hlines2.clone();
        r.yspans2 = spec.yspans2.clone();
        r.labels.extend(right_labels.cloned());
    } else {
        return Err(Error::new(
            "hline2= and yspan2= are in right-axis units: put a column there (axis=right) or \
             add a converted axis (y2=\"y*1.8+32\")",
        ));
    }
    Ok(r)
}

/// Types drawn as a line through the points (they get dash patterns in turn).
pub(super) fn is_line_kind(kind: PlotKind) -> bool {
    matches!(
        kind,
        PlotKind::Line | PlotKind::Step | PlotKind::Polar | PlotKind::Fft
    )
}
