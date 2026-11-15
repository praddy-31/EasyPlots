//! Choosing and reading columns: which column is x and which are y, calculated columns,
//! row filters, splitting a layer into groups (`by=`) and summarising groups (`stat=`).

use super::*;

pub(super) fn axis_kind_name(k: AxisKind) -> &'static str {
    match k {
        AxisKind::Number => "numbers",
        AxisKind::Date => "dates",
        AxisKind::Category => "text",
    }
}

pub(super) fn choose_columns(
    layer: &LayerSpec,
    table: &Table,
) -> Result<(XValues, YColumns), Error> {
    let file_span = layer.file.span.clone();
    let x = match &layer.x {
        Some(name) => Some(column_or_calc(table, name)?),
        None => None,
    };
    let mut ys: YColumns = layer
        .y
        .iter()
        .map(|name| column_or_calc(table, name).map(|c| (c, name.span.clone())))
        .collect::<Result<_, _>>()?;

    // A histogram counts the values of y (or of x, if only x is given).
    if matches!(layer.kind, PlotKind::Hist | PlotKind::Kde) {
        if ys.is_empty()
            && let (Some(c), Some(name)) = (x, &layer.x)
        {
            return Ok((XValues::Index, vec![(c, name.span.clone())]));
        }
        if ys.is_empty() {
            ys = number_columns(table, None, &file_span)?;
        }
        return Ok((XValues::Index, ys));
    }

    let first_is_text = table
        .columns
        .first()
        .is_some_and(|c| c.data.kind() == ColumnKind::Text);
    let x = match (x, ys.is_empty()) {
        (Some(Col::Index(i)), _) => XValues::Column(i),
        (Some(Col::Calc { values, .. }), _) => XValues::Calc(values),
        // A box plot of all columns, unless the first one names groups.
        (None, true)
            if matches!(
                layer.kind,
                PlotKind::Box | PlotKind::Violin | PlotKind::Strip
            ) && !first_is_text =>
        {
            XValues::Index
        }
        // Nothing chosen: first column on x (if there are at least two), numbers on y.
        (None, true) if table.columns.len() >= 2 => XValues::Column(0),
        (None, _) => XValues::Index,
    };
    if ys.is_empty() {
        let skip = match x {
            XValues::Column(i) => Some(i),
            _ => None,
        };
        ys = number_columns(table, skip, &file_span)?;
    }
    Ok((x, ys))
}

pub(super) fn number_columns(
    table: &Table,
    skip: Option<usize>,
    span: &std::ops::Range<usize>,
) -> Result<YColumns, Error> {
    let ys: YColumns = table
        .columns
        .iter()
        .enumerate()
        .filter(|(i, c)| Some(*i) != skip && c.data.kind() == ColumnKind::Number)
        .map(|(i, _)| (Col::Index(i), span.clone()))
        .collect();
    if ys.is_empty() {
        return Err(Error::at(
            format!(
                "{} has no number columns to plot (columns: {})",
                table.file_name(),
                describe_columns(table)
            ),
            span.clone(),
        ));
    }
    Ok(ys)
}

/// A column by name or number, or else a calculation such as `power/area`.
pub(super) fn column_or_calc(table: &Table, name: &Spanned<String>) -> Result<Col, Error> {
    match column(table, name) {
        Ok(i) => Ok(Col::Index(i)),
        Err(_) if expr::looks_like_expr(&name.value) => {
            let values = expr::parse(&name.value)
                .and_then(|x| x.eval(table))
                .map_err(|m| Error::at(m, name.span.clone()))?;
            Ok(Col::Calc {
                name: name.value.trim().to_owned(),
                values,
            })
        }
        Err(e) => Err(e),
    }
}

/// The numbers in a y (or error) column, with its name; a pointed error for dates or text.
pub(super) fn numbers(
    table: &Table,
    col: &Col,
    span: &std::ops::Range<usize>,
) -> Result<(String, Vec<f64>), Error> {
    let i = match col {
        Col::Calc { name, values } => return Ok((name.clone(), values.clone())),
        Col::Index(i) => *i,
    };
    let col = &table.columns[i];
    match &col.data {
        ColumnData::Number(v) => Ok((col.name.clone(), v.clone())),
        ColumnData::Date(_) => Err(Error::at(
            format!(
                "column \"{}\" holds dates — dates can only go on the x axis",
                col.name
            ),
            span.clone(),
        )),
        ColumnData::Text(v) => {
            let (row, example) = v
                .iter()
                .enumerate()
                .find(|(_, c)| {
                    !data::is_missing(c) && data::parse_number(c, table.decimal_comma).is_none()
                })
                .map(|(i, c)| (i + 1, c.as_str()))
                .unwrap_or((1, ""));
            Err(Error::at(
                format!(
                    "column \"{}\" in {} contains text (e.g. \"{example}\" in data row {row}), so it can't be plotted as y",
                    col.name,
                    table.file_name()
                ),
                span.clone(),
            ))
        }
    }
}

/// Find a column by name (any case) or by number (1 = first).
pub fn column(table: &Table, name: &Spanned<String>) -> Result<usize, Error> {
    let wanted = name.value.trim();
    if let Some(i) = table
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(wanted))
    {
        return Ok(i);
    }
    if let Ok(n) = wanted.parse::<usize>()
        && (1..=table.columns.len()).contains(&n)
    {
        return Ok(n - 1);
    }
    let hint = suggestion(wanted, table.column_names());
    let mut list = if hint.is_empty() {
        format!(" (columns: {})", describe_columns(table))
    } else {
        String::new()
    };
    // Every name made up: the file was read without a header row.
    if table
        .columns
        .iter()
        .enumerate()
        .all(|(i, c)| c.name == format!("column {}", i + 1))
    {
        list.push_str("; no header row was found (if you used skiplines=, it may skip the header)");
    }
    Err(Error::at(
        format!(
            "no column \"{wanted}\" in {}{hint}{list}",
            table.file_name()
        ),
        name.span.clone(),
    ))
}

pub(super) fn describe_columns(table: &Table) -> String {
    table
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// How a summary's error bars are worked out (`err=` with `stat=`).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Spread {
    Sd,
    Sem,
    Ci95,
    /// From the lower to the upper quartile (the middle half of the values).
    Iqr,
}

/// Student's t for a two-sided 95% interval with `df` degrees of freedom.
pub(super) fn t95(df: usize) -> f64 {
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

/// The group a row belongs to, written as text: a text value (trimmed), a number or a date.
pub(super) fn group_value(col: &ColumnData, r: usize) -> Option<String> {
    match col {
        ColumnData::Text(v) => Some(v[r].trim().to_owned()).filter(|t| !t.is_empty()),
        ColumnData::Number(v) => v[r].is_finite().then(|| data::short(v[r])),
        ColumnData::Date(v) => v[r]
            .is_finite()
            .then(|| crate::render::ticks::date_label(v[r])),
    }
}

/// The rows `where=` keeps (all rows without one).
pub(super) fn filtered_rows(layer: &LayerSpec, table: &Table) -> Result<Vec<usize>, Error> {
    Ok(match &layer.filter {
        None => (0..table.rows).collect(),
        Some(f) => {
            let mask = filter::parse(&f.value)
                .and_then(|flt| flt.mask(table))
                .map_err(|m| Error::at(format!("where: {m}"), f.span.clone()))?;
            (0..table.rows).filter(|&i| mask[i]).collect()
        }
    })
}

/// "Did you mean" for a group's name: one that starts with (or holds) what was typed
/// (`Cold` → `Shade`), else the closest spelling.
pub(super) fn name_hint(typed: &str, names: &[String]) -> String {
    let t = typed.trim().to_lowercase();
    let starts: Vec<&String> = names
        .iter()
        .filter(|n| n.to_lowercase().starts_with(&t))
        .collect();
    let holds: Vec<&String> = names
        .iter()
        .filter(|n| n.to_lowercase().contains(&t))
        .collect();
    match (starts.as_slice(), holds.as_slice()) {
        ([one], _) | ([], [one]) if !t.is_empty() => format!(" — did you mean \"{one}\"?"),
        _ => suggestion(typed, names.iter().map(String::as_str)),
    }
}

/// Names from `groups=` or `highlight=`: a quoted list (`"Full sun,Shade"`) that isn't
/// itself a name is split at its commas.
pub(super) fn split_names(names: &[String], known: &[String]) -> Vec<String> {
    names
        .iter()
        .flat_map(|n| {
            if n.contains(',') && !known.iter().any(|k| k.eq_ignore_ascii_case(n.trim())) {
                n.split(',')
                    .map(|p| p.trim().to_owned())
                    .filter(|p| !p.is_empty())
                    .collect()
            } else {
                vec![n.clone()]
            }
        })
        .collect()
}

/// The groups to draw: those found (in the order first seen), or the ones `groups=`
/// names, in its order (any case).
pub(super) fn choose_groups(
    found: Vec<String>,
    wanted: Option<&Spanned<Vec<String>>>,
    column: &str,
) -> Result<Vec<String>, Error> {
    let Some(w) = wanted else {
        return Ok(found);
    };
    split_names(&w.value, &found)
        .iter()
        .map(|name| {
            found
                .iter()
                .find(|f| f.eq_ignore_ascii_case(name.trim()))
                .cloned()
                .ok_or_else(|| {
                    Error::at(
                        format!(
                            "groups: \"{name}\" isn't a value of {column}{} (it holds: {})",
                            name_hint(name, &found),
                            found.join(", ")
                        ),
                        w.span.clone(),
                    )
                })
        })
        .collect()
}

/// `by=bed`: a layer with a group column becomes one layer per group (in the order first
/// seen, or as `groups=` lists them), each keeping only its rows. Lists of labels,
/// colours, markers and dashes follow the groups. A strip of a text x column is split by
/// that column, so each group gets its own colour.
pub(super) fn split_groups(
    p: &PanelSpec,
    lookup: &Lookup,
    cache: &mut DataCache,
) -> Result<PanelSpec, Error> {
    if !p
        .layers
        .iter()
        .any(|l| l.by.is_some() || l.kind == PlotKind::Strip)
    {
        return Ok(p.clone());
    }
    let mut out = p.clone();
    out.layers = Vec::new();
    for layer in &p.layers {
        let by = match (&layer.by, &layer.x) {
            (Some(b), _) => Some(b.clone()),
            (None, Some(x)) if layer.kind == PlotKind::Strip => {
                let table = cache
                    .get_with(&lookup.resolve(&layer.file)?, &layer.read)
                    .map_err(|e| e.or_at(layer.file.span.clone()))?;
                column(&table, x)
                    .is_ok_and(|i| table.columns[i].data.kind() == ColumnKind::Text)
                    .then(|| x.clone())
            }
            _ => None,
        };
        let Some(by) = by else {
            out.layers.push(layer.clone());
            continue;
        };
        if layer.kind.is_grid() || matches!(layer.kind, PlotKind::Polar) {
            return Err(Error::at(
                format!("by= doesn't work with {} plots", layer.kind.name()),
                by.span.clone(),
            ));
        }
        let table = cache
            .get_with(&lookup.resolve(&layer.file)?, &layer.read)
            .map_err(|e| e.or_at(layer.file.span.clone()))?;
        let ci = column(&table, &by)?;
        let name = table.columns[ci].name.clone();
        let col = &table.columns[ci].data;
        let mut found: Vec<String> = Vec::new();
        for r in filtered_rows(layer, &table)? {
            if let Some(g) = group_value(col, r)
                && !found.contains(&g)
            {
                found.push(g);
            }
        }
        if found.is_empty() {
            return Err(Error::at(
                format!("by: column \"{name}\" has no values in the rows drawn"),
                by.span.clone(),
            ));
        }
        let groups = choose_groups(found, layer.groups.as_ref(), &format!("\"{name}\""))?;
        if groups.len() > 40 {
            return Err(Error::at(
                format!(
                    "by: column \"{name}\" has {} different values, too many to tell apart \
                     (up to 40) — pick some with groups=, or use it as x",
                    groups.len()
                ),
                by.span.clone(),
            ));
        }
        for (option, given) in [
            ("label", layer.label.len()),
            ("color", layer.color.len()),
            ("marker", layer.marker.len()),
            ("dash", layer.style.len()),
        ] {
            if given > 1 && given > groups.len() {
                return Err(Error::at(
                    format!(
                        "{given} values in {option}= but {} group{} in by={name} — one each, in \
                         the order of the groups",
                        groups.len(),
                        if groups.len() == 1 { "" } else { "s" }
                    ),
                    layer.file.span.clone(),
                ));
            }
        }
        let many = groups.len() > 1;
        for (g, value) in groups.into_iter().enumerate() {
            let mut l = layer.clone();
            l.by = None;
            l.groups = None;
            let (label, labelled) = match layer.label.as_slice() {
                [] => (value.clone(), false),
                [one] if many => (format!("{one} {value}"), true),
                [one] => (one.clone(), true),
                list => match list.get(g) {
                    Some(l) => (l.clone(), true),
                    None => (value.clone(), false),
                },
            };
            l.group = Some(GroupOf {
                column: by.clone(),
                value,
                labelled,
            });
            l.label = vec![label];
            l.color = per_column(&layer.color, g).into_iter().collect();
            l.marker = per_column(&layer.marker, g).into_iter().collect();
            l.style = per_column(&layer.style, g).into_iter().collect();
            out.layers.push(l);
        }
    }
    Ok(out)
}

/// What `summarise` hands the panel builder: the summary's spec and table, hints for the
/// advisor, and with `summary` the raw values' N and mean ± sd under each group.
pub(super) struct Summary {
    pub spec: PanelSpec,
    pub table: Arc<Table>,
    pub hints: Vec<String>,
    pub notes: Vec<String>,
}

/// `stat=mean` / `stat=median`: each file (one layer per file, e.g. from `sample_*.csv`),
/// or each group of the text column `x=` in one or more files, becomes one value with
/// an error bar. The result is a small table (group, value, error) drawn as usual.
pub(super) fn summarise(
    p: &PanelSpec,
    lookup: &Lookup,
    cache: &mut DataCache,
) -> Result<Option<Summary>, Error> {
    let Some(first) = p.layers.iter().find(|l| l.stat.is_some()) else {
        return Ok(None);
    };
    let stat = first.stat.unwrap_or(Stat::Mean);
    if let Some(l) = p.layers.iter().find(|l| l.stat != Some(stat)) {
        return Err(Error::at(
            "stat= sums up every file of the panel: give each the same stat= (or use a \
             pattern like sample_*.csv)",
            l.file.span.clone(),
        ));
    }
    if !matches!(
        first.kind,
        PlotKind::Bar | PlotKind::Scatter | PlotKind::Line
    ) {
        return Err(Error::at(
            format!(
                "stat= gives one value per file or group, drawn as bars or points: use bar, \
                 scatter or line, not {}",
                first.kind.name()
            ),
            first.file.span.clone(),
        ));
    }
    let spread = match first.err.as_slice() {
        // The sd describes spread around a mean; a median's partner is the quartiles.
        [] => Some(match stat {
            Stat::Mean => Spread::Sd,
            Stat::Median => Spread::Iqr,
        }),
        [e] => match e.value.to_ascii_lowercase().as_str() {
            "sd" | "std" | "stdev" => Some(Spread::Sd),
            "sem" | "se" => Some(Spread::Sem),
            "ci95" | "ci" => Some(Spread::Ci95),
            "iqr" | "quartiles" => Some(Spread::Iqr),
            "none" | "off" => None,
            _ => {
                return Err(Error::at(
                    "with stat=, err= says what the error bars show: sd (standard deviation, \
                     the default for a mean), sem (standard error), ci95 (95% confidence), \
                     iqr (lower to upper quartile, the default for a median) or none",
                    e.span.clone(),
                ));
            }
        },
        [_, second, ..] => {
            return Err(Error::at(
                "with stat=, give one err=: sd, sem, ci95, iqr or none",
                second.span.clone(),
            ));
        }
    };
    if first.y.is_empty() {
        return Err(Error::at(
            "stat= needs the values to sum up: y=COLUMN",
            first.file.span.clone(),
        ));
    }
    let ys = &first.y;
    // Every file is averaged into the same values: they must measure the same thing
    // (owner, 2026-11-15: temp and pressure were averaged together).
    for l in &p.layers[1..] {
        let same = l.y.len() == ys.len()
            && l.y
                .iter()
                .zip(ys)
                .all(|(a, b)| a.value.eq_ignore_ascii_case(&b.value));
        if !same && l.group.is_none() {
            let span = l.y.first().map_or(l.file.span.clone(), |y| y.span.clone());
            return Err(Error::at(
                format!(
                    "stat= averages the files together: give them the same y column ({} ≠ {})",
                    l.y.iter()
                        .map(|y| y.value.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                    ys.iter()
                        .map(|y| y.value.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                span,
            ));
        }
    }
    let mut groups: Vec<String> = Vec::new();
    // One series per y column, or per (by= group, y column): its key and the layer it
    // came from (for its label and colour).
    let mut keys: Vec<(Option<String>, usize)> = Vec::new();
    let mut key_layer: Vec<usize> = Vec::new();
    // values[key][g]: the numbers of one series in x group g.
    let mut values: Vec<Vec<Vec<f64>>> = Vec::new();
    // Every x= column holds numbers: the groups are put in numeric order afterwards.
    let mut numeric_x = first.x.is_some();
    for (li, layer) in p.layers.iter().enumerate() {
        let path = lookup.resolve(&layer.file)?;
        let table = cache
            .get_with(&path, &layer.read)
            .map_err(|e| e.or_at(layer.file.span.clone()))?;
        let mut rows: Vec<usize> = filtered_rows(layer, &table)?
            .into_iter()
            .filter(|&r| r >= layer.skiprows)
            .step_by(layer.every.max(1))
            .collect();
        if let Some(g) = &layer.group {
            let col = &table.columns[column(&table, &g.column)?].data;
            rows.retain(|&r| group_value(col, r).as_deref() == Some(g.value.as_str()));
        }
        // The group of each row: a value of x=, or the file (its label, else its name; a
        // by= layer's label names its group, not the file).
        let file_group = match layer.group {
            None => layer.label.first().cloned(),
            Some(_) => None,
        }
        .unwrap_or_else(|| table.file_stem());
        let group_col = match &layer.x {
            Some(x) => Some(&table.columns[column(&table, x)?].data),
            None => None,
        };
        numeric_x &= matches!(group_col, Some(ColumnData::Number(_)));
        let group_of = |r: usize| -> Option<String> {
            match group_col {
                None => Some(file_group.clone()),
                Some(c) => group_value(c, r),
            }
        };
        for (k, y) in ys.iter().enumerate() {
            let key = (layer.group.as_ref().map(|g| g.value.clone()), k);
            let ki = match keys.iter().position(|e| *e == key) {
                Some(i) => i,
                None => {
                    keys.push(key);
                    key_layer.push(li);
                    values.push(vec![Vec::new(); groups.len()]);
                    keys.len() - 1
                }
            };
            let col = column_or_calc(&table, y)?;
            let (_, v) = numbers(&table, &col, &y.span)?;
            for &r in &rows {
                let (Some(g), Some(value)) = (group_of(r), v.get(r).copied()) else {
                    continue;
                };
                if !value.is_finite() || layer.clip.is_some_and(|(lo, hi)| value < lo || value > hi)
                {
                    continue;
                }
                let gi = match groups.iter().position(|e| *e == g) {
                    Some(i) => i,
                    None => {
                        groups.push(g);
                        groups.len() - 1
                    }
                };
                for list in values.iter_mut() {
                    list.resize(groups.len(), Vec::new());
                }
                values[ki][gi].push(value);
            }
        }
    }
    // Numbers in numeric order, not in the order first seen (owner, round 13: two files
    // with different x steps gave an axis running 0…20, then 0.5…15.5).
    let x_numbers: Option<Vec<f64>> = numeric_x
        .then(|| groups.iter().map(|g| g.parse::<f64>().ok()).collect())
        .flatten();
    if let Some(xs) = &x_numbers {
        let mut order: Vec<usize> = (0..groups.len()).collect();
        order.sort_by(|&a, &b| xs[a].total_cmp(&xs[b]));
        groups = order.iter().map(|&i| groups[i].clone()).collect();
        for list in values.iter_mut() {
            list.resize(order.len(), Vec::new());
            *list = order
                .iter()
                .map(|&i| std::mem::take(&mut list[i]))
                .collect();
        }
    }
    let x_numbers = x_numbers.map(|mut xs| {
        xs.sort_by(f64::total_cmp);
        xs
    });
    let by_groups = keys.iter().any(|(g, _)| g.is_some());
    if groups.is_empty() {
        return Err(Error::at(
            "stat= found no numbers to sum up",
            first.file.span.clone(),
        ));
    }
    // Nothing to average: one value in every group (one file, one row per x).
    let most = values.iter().flatten().map(Vec::len).max().unwrap_or(0);
    if most <= 1 {
        return Err(Error::at(
            "stat= needs several files, or repeated x values, to average: each x has one \
             value here (leave stat= out to draw the values)",
            first.file.span.clone(),
        ));
    }
    let mut hints = Vec::new();
    let fewest = values
        .iter()
        .flatten()
        .map(Vec::len)
        .filter(|&n| n > 0)
        .min()
        .unwrap_or(0);
    if fewest == 2 {
        hints.push(
            "some error bars rest on only 2 values: read them with care (or leave err= \
             out with err=none)"
                .to_owned(),
        );
    }
    // Files averaged at each x (`line run_*.csv x=t stat=mean`): name them.
    let files: Vec<String> = if first.x.is_some() && !by_groups && p.layers.len() > 1 {
        p.layers
            .iter()
            .map(|l| {
                l.label
                    .first()
                    .cloned()
                    // The file's name only, not the folders (owner, round 13).
                    .unwrap_or_else(|| {
                        std::path::Path::new(&l.file.value).file_stem().map_or_else(
                            || l.file.value.clone(),
                            |n| n.to_string_lossy().into_owned(),
                        )
                    })
            })
            .collect()
    } else {
        Vec::new()
    };
    // The table: the group names, then per y column its value and its error.
    let group_name = first
        .x
        .as_ref()
        .map_or("file", |x| x.value.as_str())
        .to_owned();
    let mut columns = vec![data::Column {
        name: group_name.clone(),
        // Along a number line (line, scatter…), numbers stay numbers so the spacing is
        // true; bars and boxes keep one slot per value.
        data: match &x_numbers {
            Some(xs)
                if !matches!(
                    first.kind,
                    PlotKind::Bar | PlotKind::Box | PlotKind::Violin | PlotKind::Strip
                ) =>
            {
                ColumnData::Number(xs.clone())
            }
            _ => ColumnData::Text(groups.clone()),
        },
    }];
    let mut err_names = Vec::new();
    let mut err_low_names = Vec::new();
    let mut y_names = Vec::new();
    for (ki, (group, k)) in keys.iter().enumerate() {
        let y = &ys[*k];
        // The column's name: the y column, or its group (and the column, with several).
        let mut name = match group {
            None => y.value.clone(),
            Some(g) if ys.len() == 1 => g.clone(),
            Some(g) => format!("{g} {}", y.value),
        };
        while columns.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
            name.push('\u{2009}');
        }
        y_names.push(Spanned {
            value: name.clone(),
            span: y.span.clone(),
        });
        let mut centre = Vec::new();
        let mut spreads = Vec::new();
        // Quartile bars: how far down the lower quartile is (`spreads` is then how far
        // up the upper one is).
        let mut lows = Vec::new();
        for g in 0..groups.len() {
            let st = values[ki].get(g).and_then(|v| data::Stats::of(v));
            let mid = st.map_or(f64::NAN, |s| match stat {
                Stat::Mean => s.mean,
                Stat::Median => s.median,
            });
            centre.push(mid);
            let quartiles = values[ki]
                .get(g)
                .and_then(|v| BoxStats::of(0.0, v))
                .map(|b| (b.q1, b.q3));
            spreads.push(st.map_or(f64::NAN, |s| {
                let sem = s.sd / (s.n as f64).sqrt();
                match spread {
                    Some(Spread::Sd) | None => s.sd,
                    Some(Spread::Sem) => sem,
                    Some(Spread::Ci95) => t95(s.n - 1) * sem,
                    // A mean can lie outside the middle half: no bar on that side then.
                    Some(Spread::Iqr) => quartiles.map_or(f64::NAN, |(_, q3)| (q3 - mid).max(0.0)),
                }
            }));
            lows.push(quartiles.map_or(f64::NAN, |(q1, _)| (mid - q1).max(0.0)));
        }
        columns.push(data::Column {
            name: name.clone(),
            data: ColumnData::Number(centre),
        });
        let err_name = format!("{name} error");
        columns.push(data::Column {
            name: err_name.clone(),
            data: ColumnData::Number(spreads),
        });
        let err_span = first.err.first().map_or(y.span.clone(), |e| e.span.clone());
        err_names.push(Spanned {
            value: err_name,
            span: err_span.clone(),
        });
        if spread == Some(Spread::Iqr) {
            let low_name = format!("{name} error low");
            columns.push(data::Column {
                name: low_name.clone(),
                data: ColumnData::Number(lows),
            });
            err_low_names.push(Spanned {
                value: low_name,
                span: err_span,
            });
        }
    }
    let table = Table {
        path: PathBuf::from("summary"),
        columns,
        rows: groups.len(),
        delimiter: b',',
        decimal_comma: false,
    };
    let mut layer = first.clone();
    layer.stat = None;
    layer.group = None;
    layer.y = y_names;
    layer.x = Some(Spanned {
        value: group_name,
        span: first.file.span.clone(),
    });
    layer.err = if spread.is_some() {
        err_names
    } else {
        Vec::new()
    };
    layer.err_low = err_low_names;
    layer.filter = None;
    layer.skiprows = 0;
    layer.every = 1;
    layer.clip = None;
    layer.read = ReadOptions::default();
    // Per-file labels named the groups; the legend names the y columns, or with by= the
    // groups (with the colours, markers and dashes given for them).
    if by_groups {
        let of = |ki: usize| &p.layers[key_layer[ki]];
        layer.label = (0..keys.len())
            .map(|ki| match (&of(ki).label.first(), ys.len()) {
                (Some(l), 1) => (*l).clone(),
                (Some(l), _) => format!("{l} {}", ys[keys[ki].1].value),
                (None, _) => y_names_label(&keys[ki], ys),
            })
            .collect();
        let all = |f: &dyn Fn(&LayerSpec) -> bool| (0..keys.len()).all(|ki| f(of(ki)));
        layer.color = if all(&|l| !l.color.is_empty()) {
            (0..keys.len()).map(|ki| of(ki).color[0]).collect()
        } else {
            Vec::new()
        };
        layer.marker = if all(&|l| !l.marker.is_empty()) {
            (0..keys.len()).map(|ki| of(ki).marker[0]).collect()
        } else {
            Vec::new()
        };
        layer.style = if all(&|l| !l.style.is_empty()) {
            (0..keys.len()).map(|ki| of(ki).style[0]).collect()
        } else {
            Vec::new()
        };
    } else if first.x.is_none() {
        layer.label = Vec::new();
    } else if !files.is_empty() && ys.len() == 1 {
        layer.label = vec![format!("{} of {} files", stat.name(), files.len())];
    }
    let mut out = p.clone();
    out.layers = vec![layer];
    if first.x.is_none() && out.xlabel.is_none() {
        out.xlabel = Some(String::new());
    }
    if ys.len() == 1 && out.ylabel.is_none() {
        let what = match spread {
            Some(Spread::Sd) => " ± sd",
            Some(Spread::Sem) => " ± sem",
            Some(Spread::Ci95) => ", 95% CI",
            Some(Spread::Iqr) => ", Q1–Q3",
            None => "",
        };
        let of = if files.is_empty() {
            String::new()
        } else {
            format!(" of {}", files.join(", "))
        };
        out.ylabel = Some(format!("{} ({}{of}{what})", ys[0].value, stat.name()));
    }
    // `summary` describes the measurements, not the bar heights: N and mean ± sd of
    // each group's raw values under its name, as box plots do (owner, 2026-11-15).
    let mut notes = Vec::new();
    if p.stats {
        out.stats = false;
        if keys.len() == 1 {
            notes = values[0]
                .iter()
                .map(|v| {
                    data::Stats::of(v).map_or_else(String::new, |st| {
                        format!("N = {}\n{} ± {}", st.n, st.show(st.mean), st.show(st.sd))
                    })
                })
                .collect();
        } else {
            hints.push(
                "summary with stat= writes N and mean ± sd under the groups for one y column \
                 without by="
                    .to_owned(),
            );
        }
    }
    Ok(Some(Summary {
        spec: out,
        table: Arc::new(table),
        hints,
        notes,
    }))
}

/// A summary series' legend name: its group, and the column when there are several.
pub(super) fn y_names_label(key: &(Option<String>, usize), ys: &[Spanned<String>]) -> String {
    match (&key.0, ys.len()) {
        (Some(g), 1) => g.clone(),
        (Some(g), _) => format!("{g} {}", ys[key.1].value),
        (None, _) => ys[key.1].value.clone(),
    }
}
