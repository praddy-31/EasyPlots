//! The command language (see `docs/SPEC.md` §1): text in, `PlotSpec` out.
//!
//! ```text
//! TYPE FILE [layer options] [+ [TYPE] FILE [layer options]]… [; TYPE FILE …]… [--figure options]
//! ```
//!
//! Parsing happens in two steps: `tokenize` splits the text into words (respecting quotes)
//! and `parse` walks the words, building panels and layers. Every error carries the span of
//! the word it is about.

use std::ops::Range;

use crate::error::{Error, suggestion};
use crate::spec::*;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Plus,
    Semi,
    Word(Word),
}

#[derive(Debug, Clone, PartialEq)]
struct Word {
    /// Text before the first unquoted `=`, if there is one.
    key: Option<String>,
    /// Text after the `=` (or the whole word), with quotes removed.
    value: String,
    /// `value` split at unquoted commas, each with its span in the command.
    parts: Vec<(String, Range<usize>)>,
    /// Started with `--` (a figure option).
    dashed: bool,
    key_span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Token {
    tok: Tok,
    span: Range<usize>,
}

fn tokenize(src: &str) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut chars = src.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == ';' {
            chars.next();
            tokens.push(Token {
                tok: Tok::Semi,
                span: start..start + 1,
            });
            continue;
        }
        // Read one word.
        let mut text = String::new();
        let mut quote: Option<(char, usize)> = None;
        let mut any_quoted = false;
        let mut eq_at: Option<(usize, usize)> = None; // (index in text, byte offset in src)
        let mut part_breaks: Vec<(usize, usize)> = Vec::new(); // unquoted commas after '='
        // Commas inside brackets or backticks belong to a calculation: smooth(temp,5).
        let mut depth = 0usize;
        let mut backtick = false;
        let mut end = start;
        while let Some(&(i, c)) = chars.peek() {
            match quote {
                Some((q, _)) if c == q => {
                    quote = None;
                }
                // `\"` and `\\` escape; any other backslash stays (maths: `$\alpha$`).
                Some((q, _)) if c == '\\' => {
                    chars.next();
                    end = i + 1;
                    match chars.peek() {
                        Some(&(j, n)) if n == q || n == '\\' => {
                            text.push(n);
                            end = j + n.len_utf8();
                            chars.next();
                        }
                        _ => text.push('\\'),
                    }
                    continue;
                }
                Some(_) => text.push(c),
                None if c.is_whitespace() || c == ';' => break,
                None if c == '"' || c == '\'' => {
                    quote = Some((c, i));
                    any_quoted = true;
                }
                None if c == '=' && eq_at.is_none() => {
                    eq_at = Some((text.len(), i));
                    text.push(c);
                }
                None if c == ',' && eq_at.is_some() && depth == 0 && !backtick => {
                    part_breaks.push((text.len(), i));
                    text.push(c);
                }
                None if c == '`' => {
                    backtick = !backtick;
                    text.push(c);
                }
                None if c == '(' && !backtick => {
                    depth += 1;
                    text.push(c);
                }
                None if c == ')' && !backtick => {
                    depth = depth.saturating_sub(1);
                    text.push(c);
                }
                None => text.push(c),
            }
            end = i + c.len_utf8();
            chars.next();
        }
        if let Some((q, at)) = quote {
            return Err(Error::at(format!("missing closing {q}"), at..src.len()));
        }
        let span = start..end;
        if text == "+" && !any_quoted {
            tokens.push(Token {
                tok: Tok::Plus,
                span,
            });
            continue;
        }

        let dashed = src[start..].starts_with("--");
        let skip = if dashed { 2 } else { 0 };
        let (key, value, value_text_start, value_src_start) = match eq_at {
            Some((ti, si)) => (
                Some(text[skip..ti].to_owned()),
                text[ti + 1..].to_owned(),
                ti + 1,
                si + 1,
            ),
            None => (None, text[skip..].to_owned(), skip, start + skip),
        };
        let key_span = match eq_at {
            Some((_, si)) => start..si,
            None => span.clone(),
        };
        // Split the value at unquoted commas, keeping approximate spans for each part.
        let mut parts = Vec::new();
        let mut piece_start_text = value_text_start;
        let mut piece_start_src = value_src_start;
        for &(ti, si) in part_breaks
            .iter()
            .chain(std::iter::once(&(text.len(), end)))
        {
            parts.push((text[piece_start_text..ti].to_owned(), piece_start_src..si));
            piece_start_text = ti + 1;
            piece_start_src = si + 1;
        }
        tokens.push(Token {
            tok: Tok::Word(Word {
                key,
                value,
                parts,
                dashed,
                key_span,
            }),
            span,
        });
    }
    Ok(tokens)
}

const LAYER_KEYS: &[&str] = &[
    "x",
    "y",
    "label",
    "color",
    "colour",
    "style",
    "dash",
    "width",
    "lw",
    "marker",
    "alpha",
    "where",
    "err",
    "yerr",
    "xerr",
    "axis",
    "fit",
    "stat",
    "fitrange",
    "start",
    "step",
    "z",
    "smooth",
    "skiplines",
    "skiprows",
    "every",
    "clip",
    "sheet",
    "by",
    "groups",
    "baseline",
    "norm",
    "c",
];
const PANEL_KEYS: &[&str] = &[
    "title",
    "xlabel",
    "ylabel",
    "xlim",
    "ylim",
    "legend",
    "grid",
    "y2label",
    "y2lim",
    "bins",
    "cmap",
    "zlim",
    "zlabel",
    "levels",
    "bw",
    "angle",
    "hline",
    "vline",
    "span",
    "yspan",
    "hline2",
    "vline2",
    "yspan2",
    "note",
    "arrow",
    "x2",
    "x2label",
    "y2",
    "scale",
    "window",
    "inset",
    "inset_at",
    "rect",
    "circle",
    "segment",
    "peaks",
    "dips",
    "onset",
    "area",
    "values",
    "fitlabel",
    "fitdigits",
    "fitnames",
    "corr",
    "axiscolor",
    "axiscolour",
    "highlight",
    "mean",
    "median",
    "cross",
    "crosslabel",
    "fitstats",
    "ybreak",
];
const FIGURE_KEYS: &[&str] = &[
    "out", "format", "size", "dpi", "theme", "font", "style", "grid", "layout", "suptitle",
    "widths", "heights", "legend",
];
/// `--sharex` and friends: figure options without a value.
const FIGURE_FLAGS: &[&str] = &["sharex", "sharey", "letters"];
/// Bare words: `markers` and `between` belong to the layer before them, the rest to the
/// panel.
const LAYER_FLAGS: &[&str] = &[
    "markers",
    "between",
    "band",
    "dropna",
    "fitband",
    "through0",
    "weighted",
    "slope",
    "integral",
    "showbaseline",
];
const PANEL_FLAGS: &[&str] = &[
    "logx",
    "logy",
    "logy2",
    "corr",
    "stacked",
    "grouped",
    "density",
    "filled",
    "values",
    "summary",
    "residuals",
    "peaks",
    "dips",
    "onset",
    "horizontal",
    "mean",
    "median",
    "jitter",
    "cross",
    "fitstats",
    "hist",
];

/// Pairs of options that set the same thing in two ways: written together, the second is
/// refused (owner, 2026-11-15: "throw an error when both are present").
const CLASHES: &[(&str, &str, &str)] = &[
    (
        "legend",
        "legend",
        "legend= places this panel's legend and --legend= one legend for all panels: keep one",
    ),
    (
        "markers",
        "marker",
        "marker= already turns markers on (markers is the short form with the usual shapes): \
         keep one",
    ),
];

/// Options meant to be written several times (each adds a mark).
const REPEATABLE: &[&str] = &[
    "hline", "vline", "span", "yspan", "hline2", "vline2", "yspan2", "note", "arrow", "rect",
    "circle", "segment",
];

/// Where an option belongs, for spotting one written twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Figure,
    Panel,
    Layer,
}

/// An option's level and plain name (`mean` and `mean=label` are both "mean"; `colour=`
/// is "color"), or None for a file name, a plot type or a repeatable mark.
fn option_id(word: &Word) -> Option<(Level, String)> {
    if word.dashed {
        let k = word
            .key
            .as_deref()
            .unwrap_or(&word.value)
            .to_ascii_lowercase();
        return Some((Level::Figure, if k == "grid" { "layout".into() } else { k }));
    }
    match &word.key {
        Some(k) => {
            let k = k.to_ascii_lowercase();
            let level = if LAYER_KEYS.contains(&k.as_str()) {
                Level::Layer
            } else if PANEL_KEYS.contains(&k.as_str()) {
                Level::Panel
            } else {
                return None;
            };
            let name = match k.as_str() {
                "colour" => "color",
                "yerr" => "err",
                "lw" => "width",
                "style" => "dash",
                "axiscolour" => "axiscolor",
                other => other,
            };
            (!REPEATABLE.contains(&name)).then(|| (level, name.to_owned()))
        }
        None => {
            let w = word.value.to_ascii_lowercase();
            if LAYER_FLAGS.contains(&w.as_str()) {
                Some((Level::Layer, w))
            } else if PANEL_FLAGS.contains(&w.as_str()) {
                Some((Level::Panel, w))
            } else {
                None
            }
        }
    }
}

/// Options already written: (panel, layer, name), plus which file of the panel wrote it
/// and with what value. A second one is an error rather than a silent "the last one
/// wins", which would hide a typing slip. A panel option (`peaks`, `title=`) is written
/// once per panel wherever it stands, like `--legend=` once per figure (owner,
/// 2026-11-15; this replaces the 2026-11-14 "same value on another file" exception).
/// Two different options that set the same thing (`legend=` and `--legend=`, `markers`
/// and `marker=`) are refused together too (`CLASHES`).
#[derive(Default)]
struct Seen(Vec<((usize, usize, String), usize, String)>);

impl Seen {
    fn check(
        &mut self,
        word: &Word,
        panel: usize,
        layer: usize,
        span: &Range<usize>,
    ) -> Result<(), Error> {
        let Some((level, name)) = option_id(word) else {
            return Ok(());
        };
        let key = match level {
            Level::Figure => (usize::MAX, 0, name),
            Level::Panel => (panel, usize::MAX, name),
            Level::Layer => (panel, layer, name),
        };
        // A flag's value is the flag itself; only `key=value` words carry one.
        let value = word
            .key
            .as_ref()
            .map(|_| word.value.clone())
            .unwrap_or_default();
        for (a, b, why) in CLASHES {
            let other = if key.2 == *a {
                b
            } else if key.2 == *b {
                a
            } else {
                continue;
            };
            let hit = self.0.iter().any(|((p, l, n), _, _)| {
                n == other
                    && match level {
                        // legend= in any panel against --legend=, and back.
                        Level::Figure => *p != usize::MAX,
                        Level::Panel => *p == usize::MAX,
                        Level::Layer => *p == panel && *l == layer && n != &key.2,
                    }
            });
            if hit {
                return Err(Error::at(*why, span.clone()));
            }
        }
        if self.0.iter().any(|(k, _, _)| *k == key) {
            let shown = if word.dashed {
                format!("--{}", key.2)
            } else {
                key.2.clone()
            };
            return Err(Error::at(
                format!("{shown} is given twice: keep one"),
                span.clone(),
            ));
        }
        self.0.push((key, layer, value));
        Ok(())
    }
}

/// `stored` with `more` added at the end, as `again PLOT more` and `preset NAME FILE
/// more` need: whatever `more` gives again is first taken out of `stored` (from the
/// figure, the last panel or the last file, where the added words land), so the new
/// value replaces the old one instead of being "given twice".
pub fn with_more(stored: &str, more: &str) -> String {
    let (Ok(old), Ok(new)) = (tokenize(stored), tokenize(more)) else {
        return format!("{stored} {more}");
    };
    let ids: Vec<(Level, String)> = new
        .iter()
        .map_while(|t| match &t.tok {
            Tok::Word(w) => Some(option_id(w)),
            _ => None,
        })
        .flatten()
        .collect();
    let panel_from = old
        .iter()
        .rposition(|t| t.tok == Tok::Semi)
        .map_or(0, |i| i + 1);
    let layer_from = old
        .iter()
        .rposition(|t| matches!(t.tok, Tok::Semi | Tok::Plus))
        .map_or(0, |i| i + 1);
    let mut out = stored.to_owned();
    for (i, t) in old.iter().enumerate().rev() {
        let Tok::Word(w) = &t.tok else {
            continue;
        };
        let Some(id) = option_id(w) else {
            continue;
        };
        let from = match id.0 {
            Level::Figure => 0,
            Level::Panel => panel_from,
            Level::Layer => layer_from,
        };
        if i >= from && ids.contains(&id) {
            out.replace_range(t.span.clone(), "");
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{out} {more}")
}

/// Every bare word option (`markers`, `logy`, `dropna`…), for colouring in the launcher.
pub fn bare_flags() -> impl Iterator<Item = &'static str> {
    LAYER_FLAGS.iter().chain(PANEL_FLAGS).copied()
}

/// Options and flags that are planned, with the milestone that brings them.
const PLANNED_OPTIONS: &[(&str, &str)] = &[];

fn planned(name: &str) -> Option<&'static str> {
    PLANNED_OPTIONS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, m)| *m)
}

/// Stray words right after an unquoted `key=value` (`err=Std error (%)`): probably one
/// name with spaces. Returns the range from the key to the last stray word, and the same
/// text with the value in quotes.
fn quote_fix(src: &str, stray_start: usize) -> Option<(Range<usize>, String)> {
    let before = src[..stray_start].trim_end();
    let key_start = before.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let prev = &before[key_start..];
    let (key, value) = prev.split_once('=')?;
    if key.is_empty() || key.starts_with("--") || value.is_empty() || prev.contains(['"', '\'']) {
        return None;
    }
    // The stray words: up to the next option, flag, `+` or `;`.
    let mut end = stray_start;
    // In a list (`y=temp,Std error`) only the last name gets the quotes.
    let (list, last) = match value.rsplit_once(',') {
        Some((a, b)) => (format!("{a},"), b),
        None => (String::new(), value),
    };
    if last.is_empty() {
        return None;
    }
    let mut words = vec![last.to_owned()];
    let rest = &src[stray_start..];
    let mut at = 0;
    for w in rest.split_whitespace() {
        let off = rest[at..].find(w)? + at;
        let flag = LAYER_FLAGS
            .iter()
            .chain(PANEL_FLAGS)
            .any(|f| f.eq_ignore_ascii_case(w));
        if w.contains(['=', '"', '\'', ';']) || w == "+" || w.starts_with("--") || flag {
            break;
        }
        words.push(w.to_owned());
        at = off + w.len();
        end = stray_start + at;
    }
    let joined = words.join(" ");
    Some((
        key_start..end,
        format!("{key}={list}\"{}\"", joined.replace('"', "\\\"")),
    ))
}

/// Parse a command into a `PlotSpec`.
pub fn parse(src: &str) -> Result<PlotSpec, Error> {
    let tokens = tokenize(src)?;
    let mut panels: Vec<PanelSpec> = Vec::new();
    let mut figure = FigureOpts::default();
    let mut panel: Option<PanelSpec> = None;
    // After `+`: a type and then a file are expected.
    let mut after_plus: Option<(Option<PlotKind>, Range<usize>)> = None;
    let mut seen = Seen::default();

    if tokens.is_empty() {
        return Err(Error::new(
            "empty command — start with a plot type, e.g. line data.csv x=time y=temp",
        ));
    }

    for token in tokens {
        let span = token.span.clone();
        match token.tok {
            Tok::Semi => {
                finish_panel(&mut panels, panel.take(), &after_plus, span.clone())?;
                after_plus = None;
            }
            Tok::Plus => {
                let Some(p) = &panel else {
                    return Err(Error::at("+ must come after a plot type and file", span));
                };
                if p.layers.is_empty() || after_plus.is_some() {
                    return Err(Error::at("expected a file name before +", span));
                }
                after_plus = Some((None, span));
            }
            Tok::Word(word) if word.dashed => {
                seen.check(&word, 0, 0, &span)?;
                apply_figure(&mut figure, &word, span)?
            }
            Tok::Word(word) if word.key.is_some() => {
                let Some(p) = panel.as_mut() else {
                    return Err(Error::at(
                        "start with a plot type, e.g. line data.csv x=time y=temp",
                        span,
                    ));
                };
                // `+ y=yield` with no file: the same file (and x, sheet, skip) as the layer
                // before, so several columns of one file can each get their own options.
                let key = word.key.as_deref().unwrap_or_default().to_ascii_lowercase();
                if after_plus.is_some()
                    && LAYER_KEYS.contains(&key.as_str())
                    && let Some(prev) = p.layers.last()
                {
                    let kind = after_plus.take().and_then(|(k, _)| k).unwrap_or(p.kind);
                    let mut layer = LayerSpec::new(kind, prev.file.clone());
                    layer.x.clone_from(&prev.x);
                    layer.read = prev.read.clone();
                    p.layers.push(layer);
                }
                seen.check(&word, panels.len(), p.layers.len(), &span)?;
                apply_option(p, &word, span, after_plus.is_some())?;
            }
            Tok::Word(word) => {
                let text = word.value.as_str();
                let Some(p) = panel.as_mut() else {
                    let kind = plot_kind(text, span)?;
                    panel = Some(PanelSpec::new(kind));
                    continue;
                };
                if let Some((kind @ None, plus_span)) = &mut after_plus
                    && let Some(k) = PlotKind::from_name(text)
                {
                    *kind = Some(k);
                    *plus_span = plus_span.start..span.end;
                    continue;
                }
                seen.check(&word, panels.len(), p.layers.len(), &span)?;
                match text.to_ascii_lowercase().as_str() {
                    "logx" => p.logx = true,
                    "logy" => p.logy = true,
                    "logy2" => p.logy2 = true,
                    "corr" => p.corr = Some(Corr::Pearson),
                    "stacked" => p.stacked = true,
                    "grouped" => p.stacked = false,
                    "density" => p.density = true,
                    "filled" => p.filled = true,
                    "values" => p.values = true,
                    "summary" => p.stats = true,
                    "residuals" => p.residuals = true,
                    "horizontal" => p.horizontal = true,
                    "jitter" => p.jitter = true,
                    "hist" => p.resid_hist = true,
                    "cross" => {
                        return Err(Error::at(
                            "cross takes a value: cross=curves (where the curves cross each \
                             other) or cross=50 (where each curve crosses y = 50)",
                            span,
                        ));
                    }
                    "fitstats" => p.fit_stats = Some(LegendPos::Best),
                    "mean" => {
                        p.mean = Some(Spanned {
                            value: MeanMark::Line,
                            span: span.clone(),
                        })
                    }
                    "median" => {
                        p.median = Some(Spanned {
                            value: MeanMark::Line,
                            span: span.clone(),
                        })
                    }
                    "peaks" => {
                        p.peaks = Some(Spanned {
                            value: 1,
                            span: span.clone(),
                        })
                    }
                    "dips" => {
                        p.dips = Some(Spanned {
                            value: 1,
                            span: span.clone(),
                        })
                    }
                    "onset" => {
                        p.onset = Some(Spanned {
                            value: None,
                            span: span.clone(),
                        })
                    }
                    flag @ ("markers" | "between" | "band" | "dropna" | "fitband" | "through0"
                    | "weighted" | "slope" | "integral" | "showbaseline") => match p
                        .layers
                        .last_mut()
                    {
                        Some(layer) if after_plus.is_none() => match flag {
                            "slope" | "integral" => {
                                let d = if flag == "slope" {
                                    Derive::Slope
                                } else {
                                    Derive::Integral
                                };
                                if layer.derive != Derive::None && layer.derive != d {
                                    return Err(Error::at(
                                        "slope and integral can't both apply to one file; give \
                                         the file twice with + to plot both",
                                        span,
                                    ));
                                }
                                layer.derive = d;
                            }
                            "markers" => layer.markers = true,
                            "between" => layer.between = true,
                            "dropna" => layer.dropna = true,
                            "fitband" => layer.fitting.band = true,
                            "through0" => layer.fitting.through_zero = true,
                            "weighted" => layer.fitting.weighted = true,
                            "showbaseline" => layer.show_baseline = true,
                            _ => layer.band = true,
                        },
                        _ => {
                            return Err(Error::at(format!("put {text} after the file name"), span));
                        }
                    },
                    _ => {}
                }
                if LAYER_FLAGS
                    .iter()
                    .chain(PANEL_FLAGS)
                    .any(|f| f.eq_ignore_ascii_case(text))
                {
                    continue;
                }
                if let Some(m) = planned(text) {
                    return Err(Error::at(format!("{text} arrives in milestone {m}"), span));
                }
                if FIGURE_FLAGS.iter().any(|f| f.eq_ignore_ascii_case(text)) {
                    return Err(Error::at(
                        format!("{text} applies to the whole figure: write --{text}"),
                        span,
                    ));
                }
                if p.layers.is_empty() || after_plus.is_some() {
                    let kind = after_plus.take().and_then(|(k, _)| k).unwrap_or(p.kind);
                    p.layers.push(LayerSpec::new(
                        kind,
                        Spanned {
                            value: text.to_owned(),
                            span,
                        },
                    ));
                    continue;
                }
                let hint = if looks_like_file(text) {
                    format!("to overlay {text}, put + before it")
                } else {
                    format!(
                        "unexpected word \"{text}\"{}",
                        suggestion(
                            text,
                            LAYER_FLAGS
                                .iter()
                                .chain(PANEL_FLAGS)
                                .chain(LAYER_KEYS)
                                .chain(PANEL_KEYS)
                                .copied()
                        )
                    )
                };
                let mut err = Error::at(hint, span.clone());
                if let Some((range, fixed)) = quote_fix(src, span.start) {
                    err.message = format!(
                        "{}: a name with spaces needs quotes, {fixed} (⇥ puts them in)",
                        err.message
                    );
                    err.fix = Some((range, fixed));
                }
                return Err(err);
            }
        }
    }
    let end = src.len()..src.len();
    finish_panel(&mut panels, panel, &after_plus, end)?;
    if let Some((r, c)) = figure.grid
        && panels.len() > r * c
    {
        return Err(Error::new(format!(
            "{} panels don't fit in --layout={r}x{c}",
            panels.len()
        )));
    }
    Ok(PlotSpec { panels, figure })
}

fn finish_panel(
    panels: &mut Vec<PanelSpec>,
    panel: Option<PanelSpec>,
    after_plus: &Option<(Option<PlotKind>, Range<usize>)>,
    span: Range<usize>,
) -> Result<(), Error> {
    if let Some((_, plus_span)) = after_plus {
        return Err(Error::at("expected a file name after +", plus_span.clone()));
    }
    match panel {
        None => Err(Error::at(
            "expected a plot type, e.g. line data.csv x=time y=temp",
            span,
        )),
        Some(p) if p.layers.is_empty() => Err(Error::at(
            format!("expected a CSV file after {}", p.kind.name()),
            span,
        )),
        Some(mut p) => {
            // Error bars belong to plots of values at x; elsewhere err= was quietly
            // ignored or drawn oddly (owner, round 13: "should err work with every plot?").
            for l in &p.layers {
                use PlotKind::*;
                let kind = l.kind;
                if let Some(e) = l.err.first()
                    && !matches!(kind, Line | Scatter | Bar | Step)
                {
                    return Err(Error::at(
                        format!(
                            "err= isn't used by {}: error bars go on line, scatter, bar and step \
                             plots",
                            kind.name()
                        ),
                        e.span.clone(),
                    ));
                }
                if let Some(e) = &l.xerr
                    && !matches!(kind, Line | Scatter)
                {
                    return Err(Error::at(
                        format!(
                            "xerr= isn't used by {}: sideways error bars go on line and \
                             scatter plots",
                            kind.name()
                        ),
                        e.span.clone(),
                    ));
                }
            }
            p.layers = split_pairs(std::mem::take(&mut p.layers))?;
            panels.push(p);
            Ok(())
        }
    }
}

/// `x=t1,t2 y=a,b`: one layer per (x, y) pair, each with its own entry of the colour,
/// label, style, marker and error lists.
fn split_pairs(layers: Vec<LayerSpec>) -> Result<Vec<LayerSpec>, Error> {
    let mut out = Vec::new();
    for layer in layers {
        if layer.pair_x.is_empty() {
            out.push(layer);
            continue;
        }
        let xs: Vec<Spanned<String>> = layer.x.iter().chain(&layer.pair_x).cloned().collect();
        if layer.y.len() != xs.len() {
            let span = xs[0].span.start..xs.last().expect("two or more").span.end;
            return Err(Error::at(
                format!(
                    "{} x columns pair up with as many y columns (x=t1,t2 y=a,b), but there {}",
                    xs.len(),
                    match layer.y.len() {
                        0 => "is no y=".to_owned(),
                        1 => "is 1".to_owned(),
                        n => format!("are {n}"),
                    }
                ),
                span,
            ));
        }
        for (k, x) in xs.into_iter().enumerate() {
            let mut l = layer.clone();
            l.x = Some(x);
            l.pair_x = Vec::new();
            l.y = vec![layer.y[k].clone()];
            l.label = one(crate::spec::per_column(&layer.label, k));
            l.color = one(crate::spec::per_column(&layer.color, k));
            l.style = one(crate::spec::per_column(&layer.style, k));
            l.marker = one(crate::spec::per_column(&layer.marker, k));
            l.err = one(crate::spec::per_column(&layer.err, k));
            out.push(l);
        }
    }
    Ok(out)
}

fn one<T>(v: Option<T>) -> Vec<T> {
    v.into_iter().collect()
}

fn plot_kind(text: &str, span: Range<usize>) -> Result<PlotKind, Error> {
    if let Some(k) = PlotKind::from_name(text) {
        return Ok(k);
    }
    if let Some((name, m)) = PLANNED_KINDS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(text))
    {
        return Err(Error::at(
            format!(
                "{name} plots arrive in milestone {m} — available now: {}",
                kind_list()
            ),
            span,
        ));
    }
    if text.eq_ignore_ascii_case("barh") {
        return Err(Error::at(
            "for horizontal bars write bar … horizontal (also for box, violin and strip)",
            span,
        ));
    }
    if looks_like_file(text) {
        return Err(Error::at(
            format!("start with a plot type, e.g. line {text} …"),
            span,
        ));
    }
    let names = PlotKind::ALL.map(PlotKind::name);
    Err(Error::at(
        format!(
            "unknown plot type \"{text}\"{} (available: {})",
            suggestion(text, names),
            kind_list()
        ),
        span,
    ))
}

fn kind_list() -> String {
    PlotKind::ALL.map(PlotKind::name).join(", ")
}

fn looks_like_file(text: &str) -> bool {
    text.contains('.') || text.contains('/') || text.starts_with('~')
}

fn apply_option(
    panel: &mut PanelSpec,
    word: &Word,
    span: Range<usize>,
    waiting_for_file: bool,
) -> Result<(), Error> {
    let key = word.key.as_deref().unwrap_or_default().to_ascii_lowercase();
    let value = word.value.as_str();
    let key_span = word.key_span.clone();

    if LAYER_KEYS.contains(&key.as_str()) {
        let layer = match panel.layers.last_mut() {
            Some(l) if !waiting_for_file => l,
            _ => {
                return Err(Error::at(
                    format!("put {key}= after the file name"),
                    key_span,
                ));
            }
        };
        match key.as_str() {
            // Histograms and kde count the values on the x axis, so `x=a,b` gives one each
            // (the same as y=a,b, which they also take).
            "x" if matches!(layer.kind, PlotKind::Hist | PlotKind::Kde) && word.parts.len() > 1 => {
                layer.y = word
                    .parts
                    .iter()
                    .map(|(name, s)| non_empty(name, s))
                    .collect::<Result<_, _>>()?;
            }
            "x" => {
                let mut xs = word.parts.iter().map(|(name, s)| non_empty(name, s));
                layer.x = xs.next().transpose()?;
                // x=t1,t2 y=a,b pairs them up (the layer is split after parsing).
                layer.pair_x = xs.collect::<Result<_, _>>()?;
            }
            "y" => {
                layer.y = word
                    .parts
                    .iter()
                    .map(|(name, s)| non_empty(name, s))
                    .collect::<Result<_, _>>()?;
            }
            "err" | "yerr" => {
                layer.err = word
                    .parts
                    .iter()
                    .map(|(name, s)| non_empty(name, s))
                    .collect::<Result<_, _>>()?;
            }
            "z" => {
                let [(name, s)] = word.parts.as_slice() else {
                    return Err(Error::at("z takes one column", span));
                };
                layer.z = Some(non_empty(name, s)?);
            }
            "c" => {
                let [(name, s)] = word.parts.as_slice() else {
                    return Err(Error::at(
                        "c takes one number column to colour the points by (for a fixed \
                         colour write color=)",
                        span,
                    ));
                };
                layer.c = Some(non_empty(name, s)?);
            }
            "xerr" => {
                let [(name, s)] = word.parts.as_slice() else {
                    return Err(Error::at("xerr takes one column", span));
                };
                layer.xerr = Some(non_empty(name, s)?);
            }
            "axis" => {
                layer.right = value
                    .split(',')
                    .map(|v| match v.trim().to_ascii_lowercase().as_str() {
                        "right" | "r" | "y2" | "2" => Ok(true),
                        "left" | "l" | "y" | "1" => Ok(false),
                        _ => Err(Error::at(
                            "axis is left or right, or one per y column: axis=left,right",
                            span.clone(),
                        )),
                    })
                    .collect::<Result<_, _>>()?;
            }
            "stat" => {
                layer.stat = Some(match value.to_ascii_lowercase().as_str() {
                    "mean" | "average" => Stat::Mean,
                    "median" => Stat::Median,
                    _ => {
                        return Err(Error::at(
                            "stat is mean or median (one value per file or group)",
                            span,
                        ));
                    }
                });
            }
            "fit" => {
                if let Some(f) = Fit::from_name(value) {
                    layer.fit = Some(f);
                } else if crate::expr::looks_like_expr(value) {
                    // Your own formula: checked when fitting, where the error can say more.
                    layer.fitting.formula = Some(Spanned {
                        value: value.to_owned(),
                        span: span.clone(),
                    });
                } else {
                    return Err(Error::at(
                        format!(
                            "unknown fit \"{value}\"{} — use {}, or a formula of x such as \
                             fit=\"a*exp(-x/b)+c\"",
                            suggestion(value, Fit::NAMES),
                            Fit::NAMES.join(", ")
                        ),
                        span.clone(),
                    ));
                }
            }
            "fitrange" => {
                layer.fitting.range = match limits(value, &span)? {
                    (Some(a), Some(b)) => Some((a, b)),
                    _ => {
                        return Err(Error::at(
                            "fitrange needs both ends of the x range, e.g. fitrange=2:8",
                            span,
                        ));
                    }
                };
            }
            "start" => {
                layer.fitting.start = value
                    .split(',')
                    .map(|part| {
                        let (name, v) = part.split_once(':').ok_or_else(|| {
                            Error::at(
                                "start gives each letter of the formula a rough value: start=a:1,b:10",
                                span.clone(),
                            )
                        })?;
                        Ok((name.trim().to_owned(), number(v.trim(), &span, "a starting value")?))
                    })
                    .collect::<Result<_, Error>>()?;
            }
            "step" => {
                layer.step = StepWhere::from_name(value)
                    .ok_or_else(|| Error::at("step is pre, mid or post", span.clone()))?
            }
            // Labels, colours, styles and markers take one value for every column, or a
            // list in the order of the y columns: `color=orange,blue`.
            "label" => layer.label = word.parts.iter().map(|(p, _)| p.clone()).collect(),
            "color" | "colour" => {
                layer.color = list(&word.parts, |value, span| {
                    Color::parse(value).ok_or_else(|| {
                        Error::at(
                            format!(
                                "unknown colour \"{value}\"{} — use a name (blue, red, purple, teal…) or #rrggbb",
                                suggestion(value, NAMED_COLORS.iter().map(|(n, _)| *n))
                            ),
                            span,
                        )
                    })
                })?
            }
            // `dash=` is the name now; `style=` (clashing with --style=NAME) still works.
            "style" | "dash" => {
                layer.style = list(&word.parts, |value, span| {
                    LineStyle::from_name(value).ok_or_else(|| {
                        Error::at(
                            format!("unknown line style \"{value}\" — use solid, dashed, dotted or dashdot"),
                            span,
                        )
                    })
                })?
            }
            "width" | "lw" => layer.width = Some(positive(value, &span, "width")?),
            "marker" => {
                layer.marker = list(&word.parts, |value, span| {
                    Marker::from_name(value).ok_or_else(|| {
                        Error::at(
                            format!("unknown marker \"{value}\" — use o, s, ^, d, x, + or none"),
                            span,
                        )
                    })
                })?
            }
            "where" => {
                if value.trim().is_empty() {
                    return Err(Error::at(
                        "where needs a condition, e.g. where=\"temp > 20\"",
                        span,
                    ));
                }
                layer.filter = Some(Spanned {
                    value: value.to_owned(),
                    span: key_span.end + 1..span.end,
                });
            }
            "skiplines" | "skiprows" | "every" => {
                let n = number(value, &span, &key)?;
                let least = if key == "every" { 1.0 } else { 0.0 };
                if n.fract() != 0.0 || n < least {
                    return Err(Error::at(
                        match key.as_str() {
                            "skiplines" => {
                                "skiplines is how many lines to ignore above the header, e.g. \
                                 skiplines=10"
                            }
                            "skiprows" => {
                                "skiprows is how many data rows to leave out first, e.g. skiprows=10"
                            }
                            _ => "every is a whole number: every=5 keeps every 5th row",
                        },
                        span,
                    ));
                }
                match key.as_str() {
                    "skiplines" => layer.read.skip = n as usize,
                    "skiprows" => layer.skiprows = n as usize,
                    _ => layer.every = n as usize,
                }
            }
            "clip" => {
                let (lo, hi) = limits(value, &span)?;
                layer.clip = Some((lo.unwrap_or(f64::NEG_INFINITY), hi.unwrap_or(f64::INFINITY)));
            }
            "sheet" => {
                if value.trim().is_empty() {
                    return Err(Error::at(
                        "sheet is a sheet name or number, e.g. sheet=2",
                        span,
                    ));
                }
                layer.read.sheet = Some(value.trim().to_owned());
            }
            "smooth" => {
                let n = number(value, &span, "smooth")?;
                if n.fract() != 0.0 || !(1.0..=10_000.0).contains(&n) {
                    return Err(Error::at(
                        "smooth is how many rows to average, a whole number like 5",
                        span,
                    ));
                }
                layer.smooth = Some(n as usize);
            }
            "baseline" => layer.baseline = Some(baseline(value, &span)?),
            "norm" => {
                let Some(n) = Norm::from_name(value) else {
                    return Err(Error::at(
                        "norm is max (top at 1), first (first value 1), area (area 1) or \
                         range (from 0 to 1)",
                        span,
                    ));
                };
                layer.norm = Some(Spanned { value: n, span });
            }
            "by" => {
                let [(name, s)] = word.parts.as_slice() else {
                    return Err(Error::at(
                        "by takes one column: by=bed makes one series per value of bed",
                        span,
                    ));
                };
                layer.by = Some(non_empty(name, s)?);
            }
            "groups" => {
                let names: Vec<String> = word
                    .parts
                    .iter()
                    .map(|(p, _)| p.trim().to_owned())
                    .filter(|p| !p.is_empty())
                    .collect();
                if names.is_empty() {
                    return Err(Error::at(
                        "groups lists the groups to keep, in order: groups=Cold,Hot",
                        span,
                    ));
                }
                layer.groups = Some(Spanned {
                    value: names,
                    span: key_span.end + 1..span.end,
                });
            }
            "alpha" => {
                let a = number(value, &span, "alpha")?;
                if !(0.0..=1.0).contains(&a) {
                    return Err(Error::at("alpha must be between 0 and 1", span));
                }
                layer.alpha = Some(a);
            }
            _ => unreachable!(),
        }
        return Ok(());
    }

    match key.as_str() {
        "title" => panel.title = Some(value.to_owned()),
        "xlabel" => panel.xlabel = Some(value.to_owned()),
        "ylabel" => panel.ylabel = Some(value.to_owned()),
        "xlim" => (panel.xlim, panel.xstep) = limits_and_step(value, &span)?,
        "ylim" => (panel.ylim, panel.ystep) = limits_and_step(value, &span)?,
        // hline=0, hline=20:"limit" (a label after a colon), several with commas.
        "hline" | "vline" | "hline2" | "vline2" => {
            let second = key.ends_with('2');
            let kind = if key.starts_with('h') {
                RefKind::HLine
            } else {
                RefKind::VLine
            };
            for (part, span) in &word.parts {
                let (at, label) = match part.split_once(':') {
                    Some((a, l)) => (a, Some(l)),
                    None => (part.as_str(), None),
                };
                let at = number(at, span, "a line position")?;
                match (kind, second) {
                    (RefKind::HLine, false) => panel.hlines.push(at),
                    (RefKind::HLine, true) => panel.hlines2.push(at),
                    (_, false) => panel.vlines.push(at),
                    (_, true) => panel.vlines2.push(at),
                }
                if let Some(text) = label.map(str::trim).filter(|l| !l.is_empty()) {
                    panel.ref_labels.push(RefLabel {
                        kind,
                        at: (at, at),
                        text: text.to_owned(),
                        second,
                    });
                }
            }
        }
        // span=2:5, span=2:5:"heating".
        "span" | "yspan" | "yspan2" => {
            let second = key == "yspan2";
            let kind = if key == "span" {
                RefKind::Span
            } else {
                RefKind::YSpan
            };
            for (part, span) in &word.parts {
                let mut bits = part.splitn(3, ':');
                let range = format!(
                    "{}:{}",
                    bits.next().unwrap_or(""),
                    bits.next().unwrap_or("")
                );
                let (a, b) = match limits(&range, span)? {
                    (Some(a), Some(b)) => (a, b),
                    _ => {
                        return Err(Error::at(
                            format!(
                                "{key} needs both ends, e.g. {key}=2:5 (several: {key}=2:5,8:9)"
                            ),
                            span.clone(),
                        ));
                    }
                };
                match key.as_str() {
                    "span" => panel.spans.push((a, b)),
                    "yspan" => panel.yspans.push((a, b)),
                    _ => panel.yspans2.push((a, b)),
                }
                if let Some(text) = bits.next().map(str::trim).filter(|l| !l.is_empty()) {
                    panel.ref_labels.push(RefLabel {
                        kind,
                        at: (a, b),
                        text: text.to_owned(),
                        second,
                    });
                }
            }
        }
        // note="peak"@12,30 / arrow="peak"@12,30: text at a point (x may be a date).
        "note" | "arrow" => {
            let Some((text, at)) = value.rsplit_once('@') else {
                return Err(Error::at(
                    format!("{key} is text and a point: {key}=\"peak\"@12,30"),
                    span,
                ));
            };
            // A corner instead of a point: `note="r = 0.9"@tr`.
            let corner = LegendPos::from_name(at.trim()).filter(|c| {
                matches!(
                    c,
                    LegendPos::TopLeft
                        | LegendPos::TopRight
                        | LegendPos::BottomLeft
                        | LegendPos::BottomRight
                )
            });
            if let Some(c) = corner {
                if key == "arrow" {
                    return Err(Error::at(
                        "an arrow points at a point: arrow=\"peak\"@12,30 (a corner works for note=)",
                        span,
                    ));
                }
                if text.trim().is_empty() {
                    return Err(Error::at("note needs some text: note=\"r = 0.9\"@tr", span));
                }
                panel.notes.push(Note {
                    text: text.to_owned(),
                    x: f64::NAN,
                    y: f64::NAN,
                    arrow: false,
                    corner: Some(c),
                });
                return Ok(());
            }
            let Some((x, y)) = at.split_once(',') else {
                return Err(Error::at(
                    format!(
                        "{key} needs x and y after @ ({key}=\"peak\"@12,30), or a corner: tl, tr, bl, br"
                    ),
                    span,
                ));
            };
            let x = crate::data::parse_number(x.trim(), false)
                .or_else(|| crate::data::parse_date(x.trim()))
                .ok_or_else(|| {
                    Error::at(
                        format!("{key}: \"{x}\" is not a number or date"),
                        span.clone(),
                    )
                })?;
            let y = number(y.trim(), &span, "the note's y")?;
            if text.trim().is_empty() {
                return Err(Error::at(
                    format!("{key} needs some text: {key}=\"peak\"@12,30"),
                    span,
                ));
            }
            panel.notes.push(Note {
                text: text.to_owned(),
                x,
                y,
                arrow: key == "arrow",
                corner: None,
            });
        }
        // rect=2:5:20:25[:"label"]: x range, y range, a label inside its top left.
        "rect" => {
            let bad = || {
                Error::at(
                    "rect is an x range and a y range: rect=2:5:20:25 (a label may follow: :\"A\")",
                    span.clone(),
                )
            };
            let parts: Vec<&str> = value.splitn(5, ':').collect();
            if parts.len() < 4 {
                return Err(bad());
            }
            let x0 = x_value(parts[0]).ok_or_else(bad)?;
            let x1 = x_value(parts[1]).ok_or_else(bad)?;
            let y0 = number(parts[2], &span, "rect's y")?;
            let y1 = number(parts[3], &span, "rect's y")?;
            let label = parts
                .get(4)
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty());
            panel.shapes.push(Shape::Rect {
                x: (x0.min(x1), x0.max(x1)),
                y: (y0.min(y1), y0.max(y1)),
                label,
            });
        }
        // circle=@3.5,25.6 or circle="peak"@3.5,25.6.
        "circle" => {
            let Some((text, at)) = value.rsplit_once('@') else {
                return Err(Error::at(
                    "circle is a point after @: circle=@3.5,25.6 (or circle=\"peak\"@3.5,25.6)",
                    span,
                ));
            };
            let (x, y) = point(at, &span, "circle")?;
            let label = Some(text.trim().to_owned()).filter(|t| !t.is_empty());
            panel.shapes.push(Shape::Circle { x, y, label });
        }
        // segment=0,20:10,30: from one point to another.
        "segment" => {
            let Some((a, b)) = value.split_once(':') else {
                return Err(Error::at("segment is two points: segment=0,20:10,30", span));
            };
            let from = point(a, &span, "segment")?;
            let to = point(b, &span, "segment")?;
            panel.shapes.push(Shape::Segment { from, to });
        }
        "x2" => {
            if value.trim().is_empty() {
                return Err(Error::at("x2 is a formula of x, e.g. x2=\"1240/x\"", span));
            }
            panel.x2 = Some(Spanned {
                value: value.to_owned(),
                span: key_span.end + 1..span.end,
            });
        }
        "x2label" => panel.x2label = Some(value.to_owned()),
        "y2" => {
            if value.trim().is_empty() {
                return Err(Error::at(
                    "y2 is a formula of y, e.g. y2=\"y*1.8+32\"",
                    span,
                ));
            }
            panel.y2 = Some(Spanned {
                value: value.to_owned(),
                span: key_span.end + 1..span.end,
            });
        }
        // inset=5:8 or inset=5:8:20:25 (x range, then y range).
        "inset" => {
            let bad = || {
                Error::at(
                    "inset is an x range like 5:8, or 5:8:20:25 with a y range",
                    span.clone(),
                )
            };
            let nums: Vec<f64> = value
                .split(':')
                .map(|v| v.trim().parse::<f64>().ok().filter(|v| v.is_finite()))
                .collect::<Option<_>>()
                .ok_or_else(bad)?;
            let pair = |a: f64, b: f64| (a.min(b), a.max(b));
            let (x, y) = match nums[..] {
                [a, b] if a != b => (pair(a, b), None),
                [a, b, c, d] if a != b && c != d => (pair(a, b), Some(pair(c, d))),
                _ => return Err(bad()),
            };
            let at = panel.inset.as_ref().and_then(|i| i.value.at);
            panel.inset = Some(Spanned {
                value: Inset { x, y, at },
                span: span.clone(),
            });
        }
        "inset_at" => {
            let at = match LegendPos::from_name(value) {
                Some(
                    p @ (LegendPos::TopRight
                    | LegendPos::TopLeft
                    | LegendPos::BottomRight
                    | LegendPos::BottomLeft),
                ) => p,
                _ => return Err(Error::at("inset_at is a corner: tr, tl, br or bl", span)),
            };
            match &mut panel.inset {
                Some(i) => i.value.at = Some(at),
                None => {
                    panel.inset = Some(Spanned {
                        value: Inset {
                            x: (f64::NAN, f64::NAN),
                            y: None,
                            at: Some(at),
                        },
                        span: span.clone(),
                    })
                }
            }
        }
        "legend" => {
            panel.legend = Some(LegendPos::from_name(value).ok_or_else(|| {
                Error::at(
                    format!(
                        "unknown legend position \"{value}\" — use best, tr, tl, br, bl, out or off"
                    ),
                    span.clone(),
                )
            })?)
        }
        "grid" => match value.to_ascii_lowercase().as_str() {
            "x" => (panel.grid, panel.grid_axis) = (Some(true), Some(GridLines::X)),
            "y" => (panel.grid, panel.grid_axis) = (Some(true), Some(GridLines::Y)),
            "both" => (panel.grid, panel.grid_axis) = (Some(true), None),
            _ => {
                panel.grid = Some(on_off(value, &span).map_err(|_| {
                    Error::at(
                        "grid is on, off, x (vertical lines) or y (horizontal)",
                        span.clone(),
                    )
                })?);
                panel.grid_axis = None;
            }
        },
        "axiscolor" | "axiscolour" => panel.axis_color = on_off(value, &span)?,
        "highlight" => {
            let names: Vec<String> = word
                .parts
                .iter()
                .map(|(p, _)| p.trim().to_owned())
                .filter(|p| !p.is_empty())
                .collect();
            if names.is_empty() {
                return Err(Error::at(
                    "highlight names the bar or group to colour, e.g. highlight=\"Bed temperature\"",
                    span,
                ));
            }
            panel.highlight = Some(Spanned {
                value: names,
                span: key_span.end + 1..span.end,
            });
        }
        "mean" | "median" => {
            let mark = match value.trim().to_ascii_lowercase().as_str() {
                "on" | "line" | "yes" => Some(MeanMark::Line),
                "label" | "value" | "values" => Some(MeanMark::Label),
                "off" | "no" | "none" => None,
                _ => {
                    return Err(Error::at(
                        format!(
                            "{key} is a line at each group's {key}; {key}=label writes the value too"
                        ),
                        span,
                    ));
                }
            };
            let mark = mark.map(|value| Spanned { value, span });
            if key == "mean" {
                panel.mean = mark;
            } else {
                panel.median = mark;
            }
        }
        "values" => {
            panel.values = true;
            panel.values_format = Some(NumberFormat::parse(value).ok_or_else(|| {
                Error::at(
                    format!("values= takes decimals like %.1f, %.0f% or 2, not \"{value}\""),
                    span.clone(),
                )
            })?);
        }
        "fitlabel" => {
            let v = value.trim();
            panel.fit_label = if let Some(at) = v.strip_prefix('@') {
                let (x, y) = at.split_once(',').ok_or_else(|| {
                    Error::at(
                        "fitlabel=@x,y needs both, e.g. fitlabel=@20,50",
                        span.clone(),
                    )
                })?;
                FitLabel::At(
                    number(x.trim(), &span, "the label's x")?,
                    number(y.trim(), &span, "the label's y")?,
                )
            } else {
                match v.to_ascii_lowercase().as_str() {
                    "legend" => FitLabel::Legend,
                    "off" | "none" | "no" => FitLabel::Off,
                    other => match LegendPos::from_name(other) {
                        Some(
                            c @ (LegendPos::TopLeft
                            | LegendPos::TopRight
                            | LegendPos::BottomLeft
                            | LegendPos::BottomRight),
                        ) => FitLabel::Corner(c),
                        _ => {
                            return Err(Error::at(
                                "fitlabel is legend, off, a corner (tl, tr, bl, br) or a point (@20,50)",
                                span,
                            ));
                        }
                    },
                }
            };
        }
        "fitnames" => {
            panel.fit_names = match value.to_ascii_lowercase().as_str() {
                "columns" | "column" => FitNames::Columns,
                "labels" | "label" => FitNames::Labels,
                _ => {
                    return Err(Error::at(
                        "fitnames is columns (yield = 89 − 13.2·dose) or labels (the equation \
                         uses xlabel= and ylabel=)",
                        span,
                    ));
                }
            }
        }
        "fitdigits" => {
            let d: usize = value
                .trim()
                .parse()
                .ok()
                .filter(|d| (1..=8).contains(d))
                .ok_or_else(|| {
                    Error::at("fitdigits is 1 to 8 significant figures", span.clone())
                })?;
            panel.fit_digits = Some(d);
        }
        "corr" => {
            panel.corr = Some(match value.to_ascii_lowercase().as_str() {
                "pearson" | "r" | "on" => Corr::Pearson,
                "spearman" | "rs" | "rank" => Corr::Spearman,
                _ => return Err(Error::at("corr is pearson or spearman", span)),
            });
        }
        "y2label" => panel.y2label = Some(value.to_owned()),
        "zlabel" => panel.zlabel = Some(value.to_owned()),
        "zlim" => panel.zlim = limits(value, &span)?,
        "cmap" => {
            if crate::colormap::Colormap::from_name(value).is_none() {
                let names = crate::colormap::NAMES;
                return Err(Error::at(
                    format!(
                        "unknown colour map \"{value}\"{} — use {}",
                        suggestion(value, names),
                        names.join(", ")
                    ),
                    span,
                ));
            }
            panel.cmap = Some(value.to_ascii_lowercase());
        }
        "levels" => {
            let v = number(value, &span, "levels")?;
            if v.fract() != 0.0 || !(1.0..=100.0).contains(&v) {
                return Err(Error::at("levels is a whole number from 1 to 100", span));
            }
            panel.levels = Some(v as usize);
        }
        "bw" => panel.bw = Some(positive(value, &span, "bw")?),
        "scale" => {
            panel.fft_scale = match value.to_ascii_lowercase().as_str() {
                "amplitude" | "amp" => FftScale::Amplitude,
                "power" | "psd" => FftScale::Power,
                "db" => FftScale::Db,
                _ => return Err(Error::at("scale is amplitude, power or db", span)),
            }
        }
        "window" => {
            panel.fft_window = match value.to_ascii_lowercase().as_str() {
                "hann" | "hanning" => FftWindow::Hann,
                "none" | "off" => FftWindow::None,
                _ => return Err(Error::at("window is hann or none", span)),
            }
        }
        "angle" => {
            panel.radians = match value.to_ascii_lowercase().as_str() {
                "rad" | "radians" => true,
                "deg" | "degrees" => false,
                _ => return Err(Error::at("angle is deg or rad", span)),
            }
        }
        "y2lim" => (panel.y2lim, panel.y2step) = limits_and_step(value, &span)?,
        "peaks" | "dips" => {
            // `peaks=3` how many, `peaks=storage,loss` which curves, or both: `peaks=storage,3`.
            let (numbers, names) = curve_names(value, &span);
            let n = match numbers.as_slice() {
                [] => 1.0,
                [n] => number(n, &span, &key)?,
                _ => {
                    return Err(Error::at(
                        format!(
                            "{key}= takes one count: {key}=3, or curve names: {key}=storage,loss"
                        ),
                        span,
                    ));
                }
            };
            if n.fract() != 0.0 || !(1.0..=50.0).contains(&n) {
                return Err(Error::at(
                    format!(
                        "{key} is how many to mark, a whole number like {key}=3 (or which \
                         curves: {key}=storage)"
                    ),
                    span,
                ));
            }
            let v = Some(Spanned {
                value: n as usize,
                span,
            });
            if key == "peaks" {
                panel.peaks = v;
                panel.read_on.peaks = names;
            } else {
                panel.dips = v;
                panel.read_on.dips = names;
            }
        }
        "onset" => {
            // `onset=20:40` the flat stretch, `onset=loss` which curve, or both.
            let (ranges, names) = curve_names(value, &span);
            let range = match ranges.as_slice() {
                [] => None,
                [r] => match limits(r, &span) {
                    Ok((Some(a), Some(b))) if a != b => Some((a, b)),
                    _ => None,
                },
                _ => None,
            };
            if range.is_none() && (!ranges.is_empty() || names.is_empty()) {
                return Err(Error::at(
                    "onset= is the x range of the flat stretch before the step, e.g. \
                     onset=20:40, or which curve: onset=loss (or plain onset to find it)",
                    span,
                ));
            }
            panel.onset = Some(Spanned { value: range, span });
            panel.read_on.onset = names;
        }
        "area" => {
            let range = match limits(value, &span) {
                Ok((Some(a), Some(b))) if a != b => (a, b),
                _ => {
                    return Err(Error::at(
                        "area= is the x range to measure, e.g. area=2:8",
                        span,
                    ));
                }
            };
            panel.area = Some(Spanned { value: range, span });
        }
        "cross" if value.eq_ignore_ascii_case("curves") => {
            panel.cross = Some(Spanned {
                value: Cross::Curves,
                span,
            });
        }
        "cross" => {
            let levels: Vec<f64> = value
                .split(',')
                .map(|v| number(v, &span, "cross"))
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    Error::at(
                        "cross=50 marks where each curve crosses y = 50 (cross=20,80 for two); \
                         cross=curves marks where the curves cross each other",
                        span.clone(),
                    )
                })?;
            panel.cross = Some(Spanned {
                value: Cross::Levels(levels),
                span,
            });
        }
        "ybreak" => {
            let range = match limits(value, &span) {
                Ok((Some(a), Some(b))) => (a, b),
                _ => {
                    return Err(Error::at(
                        "ybreak leaves an empty range out of the y axis: ybreak=20:80 (from:to, \
                         the smaller first)",
                        span,
                    ));
                }
            };
            panel.ybreak = Some(Spanned { value: range, span });
        }
        "crosslabel" => {
            let what = match value.to_ascii_lowercase().as_str() {
                "x" => CrossLabel::X,
                "y" => CrossLabel::Y,
                "xy" | "both" => CrossLabel::Xy,
                "off" | "none" | "no" => CrossLabel::Off,
                _ => {
                    return Err(Error::at(
                        "crosslabel is x, y, xy or off (what is written beside each crossing)",
                        span,
                    ));
                }
            };
            panel.cross_label = Some(Spanned { value: what, span });
        }
        "fitstats" => {
            panel.fit_stats = Some(match LegendPos::from_name(value) {
                Some(LegendPos::Off) => return Ok(()),
                Some(LegendPos::Out) | None => {
                    return Err(Error::at(
                        "fitstats goes in a corner: fitstats=tl, tr, bl or br (plain fitstats: \
                         the emptiest)",
                        span,
                    ));
                }
                Some(c) => c,
            });
        }
        "bins" => {
            panel.bins = if value.eq_ignore_ascii_case("auto") {
                None
            } else {
                let b = number(value, &span, "bins")?;
                if b.fract() != 0.0 || !(1.0..=1000.0).contains(&b) {
                    return Err(Error::at(
                        "bins is a whole number from 1 to 1000, or auto",
                        span,
                    ));
                }
                Some(b as usize)
            }
        }
        _ => return Err(unknown_key(&key, key_span, false)),
    }
    Ok(())
}

fn apply_figure(figure: &mut FigureOpts, word: &Word, span: Range<usize>) -> Result<(), Error> {
    let key = word
        .key
        .as_deref()
        .unwrap_or(&word.value)
        .to_ascii_lowercase();
    let value = word.value.as_str();
    if word.key.is_none() {
        match key.as_str() {
            "sharex" => figure.sharex = true,
            "sharey" => figure.sharey = true,
            "letters" => figure.letters = true,
            _ => {}
        }
        if FIGURE_FLAGS.contains(&key.as_str()) {
            return Ok(());
        }
        if FIGURE_KEYS.contains(&key.as_str()) {
            return Err(Error::at(format!("--{key} needs a value: --{key}=…"), span));
        }
        return Err(unknown_key(&key, span, true));
    }
    match key.as_str() {
        "out" => {
            if value.is_empty() {
                return Err(Error::at("--out needs a file name", span));
            }
            figure.out = Some(Spanned {
                value: value.to_owned(),
                span,
            });
        }
        "format" => {
            figure.format = Some(Format::from_name(value).ok_or_else(|| {
                Error::at(
                    format!("unknown format \"{value}\" — use png, pdf or svg"),
                    span.clone(),
                )
            })?)
        }
        "size" => {
            figure.size = Some(size(value, &span)?);
            figure.size_name = journal(value).map(|_| value.trim().to_ascii_lowercase());
        }
        "dpi" => {
            let d = number(value, &span, "dpi")?;
            if !(30.0..=2400.0).contains(&d) {
                return Err(Error::at("dpi must be between 30 and 2400", span));
            }
            figure.dpi = Some(d);
        }
        "theme" => figure.theme = Some(value.to_owned()),
        "font" => figure.font = Some(value.to_owned()),
        "suptitle" => figure.suptitle = Some(value.to_owned()).filter(|t| !t.is_empty()),
        "style" => {
            if value.trim().is_empty() {
                return Err(Error::at("--style needs a style name", span));
            }
            figure.style = Some(Spanned {
                value: value.trim().to_owned(),
                span,
            });
        }
        "layout" => {
            let bad = || Error::at("--layout looks like 2x3 (rows x columns)", span.clone());
            let (r, c) = value.split_once(['x', 'X', '×']).ok_or_else(bad)?;
            let r: usize = r.trim().parse().map_err(|_| bad())?;
            let c: usize = c.trim().parse().map_err(|_| bad())?;
            if !(1..=10).contains(&r) || !(1..=10).contains(&c) {
                return Err(Error::at("--layout allows 1 to 10 rows and columns", span));
            }
            figure.grid = Some((r, c));
        }
        which @ ("widths" | "heights") => {
            let sizes: Vec<f64> = value
                .split(',')
                .map(|v| {
                    v.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| *v > 0.0 && v.is_finite())
                })
                .collect::<Option<_>>()
                .ok_or_else(|| {
                    Error::at(
                        format!(
                            "--{which} takes relative sizes above zero, one per panel {}: \
                             --{which}=2,1",
                            if which == "widths" { "column" } else { "row" }
                        ),
                        span.clone(),
                    )
                })?;
            if which == "widths" {
                figure.widths = sizes;
            } else {
                figure.heights = sizes;
            }
        }
        "legend" => {
            figure.legend = match value.to_ascii_lowercase().as_str() {
                "bottom" | "below" | "b" => Some(SharedLegend::Bottom),
                "right" | "r" => Some(SharedLegend::Right),
                "top" | "above" | "t" => Some(SharedLegend::Top),
                "off" | "none" | "no" | "panels" => Some(SharedLegend::Panels),
                _ => {
                    return Err(Error::at(
                        "--legend puts one legend beside all panels: --legend=top, bottom or \
                         right; --legend=off gives each panel its own (a single panel's \
                         legend is legend=tr, out, off… without --)",
                        span,
                    ));
                }
            };
        }
        flag @ ("sharex" | "sharey" | "letters") => {
            let on = on_off(value, &span)?;
            match flag {
                "sharex" => figure.sharex = on,
                "sharey" => figure.sharey = on,
                _ => figure.letters = on,
            }
        }
        _ => return Err(unknown_key(&key, word.key_span.clone(), true)),
    }
    Ok(())
}

/// `peaks=storage,3` / `onset=loss,20:40`: the number-like parts (a count, a range) and the
/// curve names, in the order written.
fn curve_names(value: &str, span: &Range<usize>) -> (Vec<String>, Vec<Spanned<String>>) {
    let mut numbers = Vec::new();
    let mut names = Vec::new();
    for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let numeric = part
            .split(':')
            .all(|n| n.is_empty() || n.trim().parse::<f64>().is_ok());
        if numeric {
            numbers.push(part.to_owned());
        } else {
            names.push(Spanned {
                value: part.to_owned(),
                span: span.clone(),
            });
        }
    }
    (numbers, names)
}

fn unknown_key(key: &str, span: Range<usize>, figure: bool) -> Error {
    if !figure && matches!(key, "rolling" | "movingaverage" | "moving") {
        return Error::at(
            format!("for a moving average write smooth= (e.g. smooth=11), not {key}="),
            span,
        );
    }
    if let Some(m) = planned(key) {
        return Error::at(format!("{key} arrives in milestone {m}"), span);
    }
    if !figure && FIGURE_KEYS.contains(&key) {
        return Error::at(
            format!("{key} applies to the whole figure: write --{key}=…"),
            span,
        );
    }
    if !figure && FIGURE_FLAGS.contains(&key) {
        return Error::at(
            format!("{key} applies to the whole figure: write --{key}"),
            span,
        );
    }
    let candidates: Vec<&str> = if figure {
        FIGURE_KEYS.iter().chain(FIGURE_FLAGS).copied().collect()
    } else {
        LAYER_KEYS.iter().chain(PANEL_KEYS).copied().collect()
    };
    let prefix = if figure { "--" } else { "" };
    Error::at(
        format!(
            "unknown option {prefix}{key}{}",
            suggestion(key, candidates)
        ),
        span,
    )
}

fn non_empty(name: &str, span: &Range<usize>) -> Result<Spanned<String>, Error> {
    if name.trim().is_empty() {
        return Err(Error::at("empty column name", span.clone()));
    }
    Ok(Spanned {
        value: name.trim().to_owned(),
        span: span.clone(),
    })
}

/// A position on the x axis: a number or a date.
fn x_value(s: &str) -> Option<f64> {
    crate::data::parse_number(s.trim(), false).or_else(|| crate::data::parse_date(s.trim()))
}

/// `x,y`: a point in data units (x may be a date).
fn point(s: &str, span: &Range<usize>, key: &str) -> Result<(f64, f64), Error> {
    let bad = || {
        Error::at(
            format!("{key}: \"{s}\" should be a point like 3.5,25.6"),
            span.clone(),
        )
    };
    let (x, y) = s.split_once(',').ok_or_else(bad)?;
    let x = x_value(x).ok_or_else(bad)?;
    let y = y
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(bad)?;
    Ok((x, y))
}

/// `baseline=0:5,90:100`, `baseline=poly2,0:5,90:100`, `baseline=linear` (the ends).
fn baseline(value: &str, span: &Range<usize>) -> Result<Baseline, Error> {
    let bad = || {
        Error::at(
            "baseline is the x ranges to fit it on, e.g. baseline=0:5,90:100, with the shape \
             first if not a straight line: baseline=poly2,0:5,90:100 (const, linear, poly2, \
             poly3)",
            span.clone(),
        )
    };
    let mut b = Baseline {
        degree: 1,
        ranges: Vec::new(),
        span: span.clone(),
    };
    for (k, part) in value.split(',').map(str::trim).enumerate() {
        let degree = match part.to_ascii_lowercase().as_str() {
            "const" | "constant" | "flat" | "poly0" => Some(0),
            "linear" | "lin" | "line" | "poly1" => Some(1),
            "poly2" | "quadratic" => Some(2),
            "poly3" | "cubic" => Some(3),
            _ => None,
        };
        match degree {
            Some(d) if k == 0 => b.degree = d,
            Some(_) => return Err(bad()),
            None => match limits(part, span) {
                Ok((Some(a), Some(c))) if a != c => b.ranges.push((a.min(c), a.max(c))),
                _ => return Err(bad()),
            },
        }
    }
    Ok(b)
}

fn number(value: &str, span: &Range<usize>, what: &str) -> Result<f64, Error> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| {
            Error::at(
                format!("{what} must be a number, not \"{value}\""),
                span.clone(),
            )
        })
}

fn positive(value: &str, span: &Range<usize>, what: &str) -> Result<f64, Error> {
    let v = number(value, span, what)?;
    if v <= 0.0 {
        return Err(Error::at(
            format!("{what} must be more than 0"),
            span.clone(),
        ));
    }
    Ok(v)
}

fn on_off(value: &str, span: &Range<usize>) -> Result<bool, Error> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "yes" | "true" | "1" => Ok(true),
        "off" | "no" | "false" | "0" => Ok(false),
        _ => Err(Error::at("use on or off", span.clone())),
    }
}

/// `0:10`, `:5`, `2:` → (from, to).
/// Read each comma-separated part of a value (each with its own place for errors).
fn list<T>(
    parts: &[(String, Range<usize>)],
    read: impl Fn(&str, Range<usize>) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    parts
        .iter()
        .map(|(p, span)| read(p.trim(), span.clone()))
        .collect()
}

type Limits = (Option<f64>, Option<f64>);

/// `0:100` or `0:100:20`: the axis range, and the tick step as a third part (`::20` keeps
/// the range automatic and only sets the step).
fn limits_and_step(
    value: &str,
    span: &Range<usize>,
) -> Result<(Limits, Option<Spanned<f64>>), Error> {
    let (range, step) = match value.matches(':').count() {
        2 => {
            let (range, step) = value.rsplit_once(':').expect("two colons");
            (range, Some(step))
        }
        _ => (value, None),
    };
    let step = match step.map(str::trim) {
        None | Some("") => None,
        Some(s) => {
            let v = number(s, span, "the tick step")?;
            if v <= 0.0 {
                return Err(Error::at(
                    "the tick step (third number) must be above zero, e.g. 0:100:20",
                    span.clone(),
                ));
            }
            Some(Spanned {
                value: v,
                span: span.clone(),
            })
        }
    };
    Ok((limits(value_or_open(range), span)?, step))
}

/// `::20` leaves both ends open.
fn value_or_open(range: &str) -> &str {
    if range.is_empty() { ":" } else { range }
}

fn limits(value: &str, span: &Range<usize>) -> Result<(Option<f64>, Option<f64>), Error> {
    let Some((a, b)) = value.split_once(':') else {
        return Err(Error::at(
            "limits look like 0:10, :10 (top only), 0: (bottom only), or 0:100:20 with a tick step",
            span.clone(),
        ));
    };
    let side = |s: &str| -> Result<Option<f64>, Error> {
        if s.trim().is_empty() {
            Ok(None)
        } else {
            number(s, span, "a limit").map(Some)
        }
    };
    let (lo, hi) = (side(a)?, side(b)?);
    if let (Some(lo), Some(hi)) = (lo, hi)
        && lo >= hi
    {
        return Err(Error::at("the first limit must be smaller", span.clone()));
    }
    Ok((lo, hi))
}

/// `6x4in`, `15x10cm`, `160x100mm`, `432x288pt`; no unit means inches. Returns points.
/// A journal's figure size: width and height in points, and the font size its figures
/// usually use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Journal {
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
}

/// Named sizes, by publisher (journals of one publisher share column widths): the name
/// alone is one column, `-double` the full page width, `-mid` the in-between width where a
/// publisher has one. Widths in millimetres.
pub const JOURNALS: &[(&str, f64, &str)] = &[
    ("column", 88.9, "one column (3.5 in)"),
    ("double", 177.8, "full page width (7 in)"),
    ("acs", 82.55, "ACS: JCTC, J. Phys. Chem., JACS (3.25 in)"),
    ("acs-double", 177.8, "ACS full width (7 in)"),
    ("aip", 85.0, "AIP: J. Chem. Phys., APL (8.5 cm)"),
    ("aip-double", 170.0, "AIP full width (17 cm)"),
    ("aps", 85.7, "APS: Phys. Rev. Lett., PRA, PRB (3 3/8 in)"),
    ("aps-double", 171.5, "APS full width (6 3/4 in)"),
    ("rsc", 83.0, "RSC: PCCP, Chem. Sci. (8.3 cm)"),
    ("rsc-double", 171.0, "RSC full width (17.1 cm)"),
    ("nature", 89.0, "Nature, npj, Nature Comms (89 mm)"),
    ("nature-double", 183.0, "Nature full width (183 mm)"),
    ("science", 55.0, "Science one column (5.5 cm)"),
    ("science-mid", 120.0, "Science two columns (12 cm)"),
    ("science-double", 183.0, "Science full width (18.3 cm)"),
    (
        "elsevier",
        90.0,
        "Elsevier: Comput. Phys. Commun., Chem. Phys. (90 mm)",
    ),
    ("elsevier-mid", 140.0, "Elsevier 1.5 columns (140 mm)"),
    ("elsevier-double", 190.0, "Elsevier full width (190 mm)"),
];

/// `--size=acs` and friends. Narrow figures are ¾ as tall as wide; wide ones flatter, so a
/// full-width figure doesn't fill the page.
pub fn journal(name: &str) -> Option<Journal> {
    let name = name.trim().to_ascii_lowercase();
    let &(_, mm, _) = JOURNALS.iter().find(|j| j.0 == name)?;
    let width = mm / 25.4 * 72.0;
    let ratio = if mm > 130.0 { 0.42 } else { 0.75 };
    Some(Journal {
        width,
        height: (width * ratio).max(72.0),
        font_size: if name.starts_with("nature") || name.starts_with("science") {
            7.0
        } else {
            8.0
        },
    })
}

pub(crate) fn size(value: &str, span: &Range<usize>) -> Result<(f64, f64), Error> {
    if let Some(j) = journal(value) {
        return Ok((j.width, j.height));
    }
    let v = value.trim().to_ascii_lowercase();
    if !v.chars().any(|c| c.is_ascii_digit()) {
        return Err(Error::at(
            format!(
                "unknown size \"{value}\"{} (journal sizes: acs, aip, aps, rsc, nature, science, elsevier, column, double; add -double for full width)",
                suggestion(&v, JOURNALS.iter().map(|j| j.0))
            ),
            span.clone(),
        ));
    }
    let (dims, per_unit) = if let Some(d) = v.strip_suffix("in") {
        (d, 72.0)
    } else if let Some(d) = v.strip_suffix("cm") {
        (d, 72.0 / 2.54)
    } else if let Some(d) = v.strip_suffix("mm") {
        (d, 72.0 / 25.4)
    } else if let Some(d) = v.strip_suffix("pt") {
        (d, 1.0)
    } else {
        (v.as_str(), 72.0)
    };
    let bad = || {
        Error::at(
            "size looks like 6x4in, 15x10cm or 160x100mm, or a journal size such as acs, aip-double or column",
            span.clone(),
        )
    };
    let (w, h) = dims.split_once(['x', '×']).ok_or_else(bad)?;
    let w: f64 = w.trim().parse().map_err(|_| bad())?;
    let h: f64 = h.trim().parse().map_err(|_| bad())?;
    let (w, h) = (w * per_unit, h * per_unit);
    if !(72.0..=72.0 * 40.0).contains(&w) || !(72.0..=72.0 * 40.0).contains(&h) {
        return Err(Error::at(
            "each side must be between 1 and 40 inches",
            span.clone(),
        ));
    }
    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> PlotSpec {
        parse(src).unwrap_or_else(|e| panic!("{}", e.display_with(src)))
    }

    fn err(src: &str) -> Error {
        parse(src).expect_err(src)
    }

    #[test]
    fn an_option_given_twice_is_an_error() {
        let src = "strip f.csv x=a y=b mean mean=label";
        let e = err(src);
        assert_eq!(e.message, "mean is given twice: keep one");
        assert_eq!(&src[e.span.clone().unwrap()], "mean=label");
        // Other spellings of one option count as the same.
        assert!(
            err("line f.csv y=b color=red colour=blue")
                .message
                .contains("color is given twice")
        );
        assert!(
            err("line f.csv y=b title=A title=B")
                .message
                .contains("twice")
        );
        assert!(
            err("line f.csv y=b --size=acs --size=3x2in")
                .message
                .contains("--size is given twice")
        );
        assert!(
            err("line f.csv y=b cross=curves cross=50")
                .message
                .contains("cross is given twice")
        );
        // Each file and each panel has its own; marks may repeat.
        ok("line a.csv y=t color=red + b.csv y=t color=blue + y=p color=green");
        ok("line a.csv y=t title=A ; line b.csv y=t title=B");
        ok(r#"line a.csv y=t hline=1 hline=2 note="a"@tr note="b"@tl"#);
    }

    #[test]
    fn a_panel_option_is_written_once_wherever_it_stands() {
        // Like --legend= for the figure: once per panel, anywhere in it (owner,
        // 2026-11-15; the 2026-11-14 "same value on another file" exception is gone).
        for twice in [
            "line a.csv x=t y=e peaks onset + b.csv x=t y=f axis=right peaks onset",
            "line a.csv y=t peaks=3 + b.csv y=u peaks=3",
            "line a.csv y=t title=A + b.csv y=u title=A",
            "line a.csv y=t peaks peaks",
        ] {
            let e = err(twice).message;
            assert!(e.contains("is given twice"), "{twice}: {e}");
        }
        ok("line a.csv x=t y=e + b.csv x=t y=f axis=right peaks onset");
        ok("line a.csv y=t title=A ; line b.csv y=u title=A");
    }

    #[test]
    fn peaks_dips_and_onset_can_name_their_curves() {
        let p = &ok("line a.csv x=T y=storage,loss peaks=storage").panels[0];
        assert_eq!(p.peaks.as_ref().unwrap().value, 1);
        assert_eq!(p.read_on.peaks[0].value, "storage");
        let p = &ok("line a.csv x=T y=storage,loss peaks=loss,3 dips=storage onset=loss,20:40")
            .panels[0];
        assert_eq!(p.peaks.as_ref().unwrap().value, 3);
        assert_eq!(p.read_on.peaks[0].value, "loss");
        assert_eq!(p.read_on.dips[0].value, "storage");
        assert_eq!(p.onset.as_ref().unwrap().value, Some((20.0, 40.0)));
        assert_eq!(p.read_on.onset[0].value, "loss");
        let p = &ok("line a.csv x=T y=storage onset=storage").panels[0];
        assert_eq!(p.onset.as_ref().unwrap().value, None);
        assert!(
            err("line a.csv y=t peaks=2,3")
                .message
                .contains("one count")
        );
        assert!(err("line a.csv y=t onset=20").message.contains("x range"));
    }

    #[test]
    fn options_that_set_one_thing_two_ways_are_refused_together() {
        for (both, says) in [
            ("line a.csv y=t legend=tr --legend=top", "keep one"),
            (
                "line a.csv y=t --legend=top ; line b.csv y=u legend=br",
                "keep one",
            ),
            (
                "line a.csv y=t markers marker=o",
                "marker= already turns markers on",
            ),
            (
                "line a.csv y=t marker=o markers",
                "marker= already turns markers on",
            ),
            ("line a.csv y=t cross", "cross=curves"),
            // Old names are plain unknown words (owner, round 14: no rename messages).
            ("line a.csv y=t stats", "unexpected word"),
            (
                "line a.csv y=t ; line a.csv y=t --grid=2x1",
                "unknown option --grid",
            ),
        ] {
            let e = err(both).message;
            assert!(e.contains(says), "{both}: {e}");
        }
        // Each on its own, or on different files, is fine.
        ok("line a.csv y=t markers + b.csv y=u marker=s");
        ok("line a.csv y=t legend=tr ; line b.csv y=u legend=out");
        let p = &ok("line a.csv y=t,u cross=curves summary").panels[0];
        assert_eq!(p.cross.as_ref().unwrap().value, Cross::Curves);
        assert!(p.stats);
    }

    #[test]
    fn fitnames_picks_the_equation_names() {
        assert_eq!(
            ok("scatter a.csv x=d y=r fit=linear").panels[0].fit_names,
            FitNames::Columns
        );
        assert_eq!(
            ok("scatter a.csv x=d y=r fit=linear fitnames=labels").panels[0].fit_names,
            FitNames::Labels
        );
        assert!(
            err("scatter a.csv x=d y=r fitnames=axes")
                .message
                .contains("columns")
        );
    }

    #[test]
    fn more_options_replace_stored_ones() {
        assert_eq!(
            with_more(
                r#"line a.csv x=t y=temp title="Run 3" color=red --format=png"#,
                "title=New --format=pdf"
            ),
            "line a.csv x=t y=temp color=red title=New --format=pdf"
        );
        // Only the last file's and the last panel's options are replaced.
        assert_eq!(
            with_more(
                "line a.csv y=t color=red + b.csv y=t color=blue mean",
                "color=green mean=label"
            ),
            "line a.csv y=t color=red + b.csv y=t color=green mean=label"
        );
        assert_eq!(
            with_more("line a.csv y=t logy ; line b.csv y=t", "logy hline=3"),
            "line a.csv y=t logy ; line b.csv y=t logy hline=3"
        );
        assert!(parse(&with_more("strip f.csv y=b mean", "mean=label")).is_ok());
    }

    #[test]
    fn values_with_the_error() {
        let f = NumberFormat::parse("%.1f+-").unwrap();
        assert!(f.error && f.decimals == Some(1) && f.after.is_empty());
        let f = NumberFormat::parse("%.0f±%").unwrap();
        assert!(f.error && f.after == "%");
        assert!(!NumberFormat::parse("%.0f%").unwrap().error);
    }

    #[test]
    fn second_axes() {
        let p = &ok(
            r#"line a.csv y=t,p axis=left,right hline2=5:"top",6 yspan2=1:2 vline2=3 y2="y*2""#,
        )
        .panels[0];
        assert_eq!(p.hlines2, [5.0, 6.0]);
        assert_eq!(p.yspans2, [(1.0, 2.0)]);
        assert_eq!(p.vlines2, [3.0]);
        assert!(p.ref_labels[0].second);
        assert_eq!(p.y2.as_ref().unwrap().value, "y*2");
        assert!(err("line a.csv axis=up").message.contains("left or right"));
    }

    #[test]
    fn names_with_spaces_get_a_quoted_fix() {
        let src = "line d.csv x=t y=temp,Std error (%) band title=x";
        let e = err(src);
        let (range, text) = e.fix.clone().expect("a fix");
        assert_eq!(&src[range.clone()], "y=temp,Std error (%)");
        assert_eq!(text, "y=temp,\"Std error (%)\"");
        let fixed = format!("{}{text}{}", &src[..range.start], &src[range.end..]);
        assert!(parse(&fixed).is_ok(), "{fixed}");
        assert!(e.message.contains("quotes"), "{}", e.message);
        // Not after a quoted value or a plain word.
        assert!(err("line d.csv y=\"a\" b").fix.is_none());
    }

    #[test]
    fn basic_line() {
        let s = ok("line data.csv x=time y=temp");
        let l = &s.panels[0].layers[0];
        assert_eq!(s.panels[0].kind, PlotKind::Line);
        assert_eq!(l.file.value, "data.csv");
        assert_eq!(l.x.as_ref().unwrap().value, "time");
        assert_eq!(l.y[0].value, "temp");
    }

    #[test]
    fn several_y_and_quotes() {
        let src = r#"line data.csv x=time y=temp,"Voltage (V)" title="Run 3" legend=tr"#;
        let s = ok(src);
        let l = &s.panels[0].layers[0];
        let ys: Vec<_> = l.y.iter().map(|y| y.value.as_str()).collect();
        assert_eq!(ys, ["temp", "Voltage (V)"]);
        assert_eq!(&src[l.y[0].span.clone()], "temp");
        assert_eq!(s.panels[0].title.as_deref(), Some("Run 3"));
        assert_eq!(s.panels[0].legend, Some(LegendPos::TopRight));
    }

    #[test]
    fn overlay_with_plus() {
        let s = ok("line a.csv x=t y=temp label=A + scatter b.csv x=t y=temp label=B");
        let layers = &s.panels[0].layers;
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[1].kind, PlotKind::Scatter);
        assert_eq!(layers[1].label, vec!["B".to_owned()]);
        assert_eq!(layers[0].kind, PlotKind::Line);
    }

    #[test]
    fn figure_options() {
        let s = ok("bar sales.csv x=month y=north --size=15x10cm --dpi=150 --out=sales.pdf");
        let (w, h) = s.figure.size.unwrap();
        assert!((w - 425.2).abs() < 0.1 && (h - 283.5).abs() < 0.1);
        assert_eq!(s.figure.dpi, Some(150.0));
        assert_eq!(s.figure.out.unwrap().value, "sales.pdf");
    }

    #[test]
    fn markers_flag_and_limits() {
        let s = ok("line d.csv x=1 y=2 markers xlim=0: ylim=-1:5.5");
        assert!(s.panels[0].layers[0].markers);
        assert_eq!(s.panels[0].xlim, (Some(0.0), None));
        assert_eq!(s.panels[0].ylim, (Some(-1.0), Some(5.5)));
    }

    #[test]
    fn log_axes_legend_out_and_where() {
        let src = r#"scatter d.csv x=1 y=2 logy logx legend=out where="temp > 20 and t < 5""#;
        let s = ok(src);
        let p = &s.panels[0];
        assert!(p.logx && p.logy);
        assert_eq!(p.legend, Some(LegendPos::Out));
        let f = p.layers[0].filter.as_ref().unwrap();
        assert_eq!(f.value, "temp > 20 and t < 5");
        assert_eq!(&src[f.span.clone()], "\"temp > 20 and t < 5\"");
    }

    #[test]
    fn helpful_errors() {
        assert!(err("lnie d.csv").message.contains("did you mean \"line\""));
        assert!(
            err("heatmap d.csv cmap=viridsi")
                .message
                .contains("did you mean \"viridis\"")
        );
        assert!(
            err("data.csv x=1")
                .message
                .contains("start with a plot type")
        );
        assert!(err("line").message.contains("expected a CSV file"));
        assert!(err("line a.csv b.csv").message.contains("put + before"));
        assert!(
            err("line a.csv lable=x")
                .message
                .contains("did you mean \"label\"")
        );
        assert!(err("line a.csv bins=3.5").message.contains("whole number"));
        assert!(
            err("line a.csv fit=cubik")
                .message
                .contains("did you mean \"cubic\"")
                || err("line a.csv fit=cubik").message.contains("poly")
        );
        assert!(
            err("line a.csv ; line b.csv --layout=1x1")
                .message
                .contains("don't fit")
        );
        assert!(err("line a.csv sharex").message.contains("--sharex"));
        assert!(err("line a.csv +").message.contains("after +"));
        assert!(
            err("line x=1 a.csv")
                .message
                .contains("after the file name")
        );
        assert!(err(r#"line a.csv title="oops"#).message.contains("closing"));
        assert!(err("line a.csv out=x.png").message.contains("--out"));
    }

    #[test]
    fn labels_notes_and_axes_options() {
        let s = ok(
            r#"scatter f.csv x=a y=b fit=linear fitlabel=tr fitdigits=4 corr=spearman grid=y values=%.0f% note="n = 6"@br logy2 axiscolor=off"#,
        );
        let p = &s.panels[0];
        assert_eq!(p.fit_label, FitLabel::Corner(LegendPos::TopRight));
        assert_eq!(p.fit_digits, Some(4));
        assert_eq!(p.corr, Some(Corr::Spearman));
        assert_eq!((p.grid, p.grid_axis), (Some(true), Some(GridLines::Y)));
        assert!(p.values);
        let f = p.values_format.as_ref().unwrap();
        assert_eq!((f.decimals, f.after.as_str()), (Some(0), "%"));
        assert_eq!(p.notes[0].corner, Some(LegendPos::BottomRight));
        assert!(p.logy2 && !p.axis_color);
        let p = &ok("scatter f.csv x=a y=b corr fitlabel=@2,30 values").panels[0];
        assert_eq!(p.corr, Some(Corr::Pearson));
        assert_eq!(p.fit_label, FitLabel::At(2.0, 30.0));
        assert!(p.values && p.values_format.is_none());
        assert!(err("line f.csv y=b grid=z").message.contains("x (vertical"));
        assert!(
            err("line f.csv y=b fitlabel=middle")
                .message
                .contains("corner")
        );
        assert!(
            err(r#"line f.csv y=b arrow="x"@tr"#)
                .message
                .contains("arrow")
        );
        assert!(err("line f.csv y=b values=abc").message.contains("%.1f"));
    }

    #[test]
    fn plus_without_a_file_reuses_the_last_one() {
        let s = ok(
            "line f.csv x=dose skiplines=2 y=growth fit=linear + y=yield axis=right fit=log + scatter y=growth",
        );
        let l = &s.panels[0].layers;
        assert_eq!(l.len(), 3);
        assert_eq!(l[1].file.value, "f.csv");
        assert_eq!(l[1].x.as_ref().unwrap().value, "dose");
        assert_eq!(l[1].read, l[0].read);
        assert_eq!(l[1].y[0].value, "yield");
        assert_eq!(l[1].right, [true]);
        assert_eq!(l[1].fit, Some(Fit::Log));
        assert_eq!(l[0].fit, Some(Fit::Poly(1)));
        assert_eq!(l[2].kind, PlotKind::Scatter);
        // A flag alone after + still needs a file.
        assert!(
            err("line f.csv y=a + markers")
                .message
                .contains("after the file name")
        );
    }

    #[test]
    fn panels_and_new_options() {
        let src = "line a.csv x=t y=temp err=sd axis=right fit=poly2 ; hist b.csv y=v bins=30 density stacked --layout=2x1 --sharex --suptitle=\"Runs\" --style=thesis";
        let s = ok(src);
        assert_eq!(s.panels.len(), 2);
        let l = &s.panels[0].layers[0];
        assert_eq!(l.err[0].value, "sd");
        assert_eq!(l.right, [true]);
        assert_eq!(l.fit, Some(Fit::Poly(2)));
        let both = &ok("line a.csv y=storage,loss axis=left,right").panels[0].layers[0];
        assert!(!both.right_at(0) && both.right_at(1));
        let p = &s.panels[1];
        assert_eq!(p.kind, PlotKind::Hist);
        assert_eq!(p.bins, Some(30));
        assert!(p.density && p.stacked);
        assert_eq!(s.figure.grid, Some((2, 1)));
        assert!(s.figure.sharex && !s.figure.sharey);
        assert_eq!(s.figure.suptitle.as_deref(), Some("Runs"));
        assert_eq!(s.figure.style.unwrap().value, "thesis");
        let s = ok("area a.csv y=lo,hi between step=mid y2label=V y2lim=0:5");
        assert!(s.panels[0].layers[0].between);
        assert_eq!(s.panels[0].layers[0].step, StepWhere::Mid);
        assert_eq!(s.panels[0].y2lim, (Some(0.0), Some(5.0)));
    }

    #[test]
    fn point_shapes() {
        let s = ok("scatter a.csv y=b,c,d,e marker=v,open-circle,Open_Square,diamond-open");
        assert_eq!(
            s.panels[0].layers[0].marker,
            vec![
                Marker::TriangleDown,
                Marker::OpenCircle,
                Marker::OpenSquare,
                Marker::OpenDiamond
            ]
        );
        assert!(
            err("scatter a.csv marker=open-plus")
                .message
                .contains("marker")
        );
        // Every shape's name reads back as itself (style files store names).
        for m in Marker::CYCLE {
            assert_eq!(Marker::from_name(m.name()), Some(m));
        }
    }

    #[test]
    fn shapes() {
        let s = ok(
            r#"line a.csv rect=5:2:20:25:"zone A" circle=@3.5,25.6 circle="peak"@2024-03-01,30 segment=0,20:10,30"#,
        );
        let sh = &s.panels[0].shapes;
        assert_eq!(
            sh[0],
            Shape::Rect {
                x: (2.0, 5.0),
                y: (20.0, 25.0),
                label: Some("zone A".into())
            }
        );
        assert_eq!(
            sh[1],
            Shape::Circle {
                x: 3.5,
                y: 25.6,
                label: None
            }
        );
        assert!(
            matches!(&sh[2], Shape::Circle { x, label: Some(l), .. } if *x > 1e9 && l == "peak")
        );
        assert_eq!(
            sh[3],
            Shape::Segment {
                from: (0.0, 20.0),
                to: (10.0, 30.0)
            }
        );
        assert!(
            err("line a.csv rect=1:2:3")
                .message
                .contains("x range and a y range")
        );
        assert!(err("line a.csv circle=3,4").message.contains("after @"));
        assert!(
            err("line a.csv segment=0,20")
                .message
                .contains("two points")
        );
        assert!(
            err("line a.csv segment=0,20:ten,3")
                .message
                .contains("point like")
        );
    }

    #[test]
    fn journal_sizes_fft_and_inset() {
        let s = ok("fft a.csv x=t y=v scale=db window=none inset=40:60 inset_at=bl --size=ACS");
        let p = &s.panels[0];
        assert_eq!(p.kind, PlotKind::Fft);
        assert_eq!((p.fft_scale, p.fft_window), (FftScale::Db, FftWindow::None));
        let i = p.inset.as_ref().unwrap().value;
        assert_eq!(
            (i.x, i.y, i.at),
            ((40.0, 60.0), None, Some(LegendPos::BottomLeft))
        );
        assert_eq!(s.figure.size_name.as_deref(), Some("acs"));
        assert!((s.figure.size.unwrap().0 - 234.0).abs() < 0.1);
        let s = ok("line a.csv inset=8:5:30:20 --size=aip-double");
        let i = s.panels[0].inset.as_ref().unwrap().value;
        assert_eq!((i.x, i.y), ((5.0, 8.0), Some((20.0, 30.0))));
        assert!(
            err("line a.csv --size=acss")
                .message
                .contains("did you mean \"acs\"")
        );
        assert!(err("line a.csv inset=5").message.contains("x range"));
        assert!(err("line a.csv inset_at=middle").message.contains("corner"));
        assert!(err("fft a.csv scale=loud").message.contains("amplitude"));
        // Every journal size is between 1 and 40 inches wide.
        for (name, _, _) in JOURNALS {
            assert!(size(name, &(0..0)).is_ok(), "{name}");
        }
    }

    #[test]
    fn labels_notes_and_top_axis() {
        let s = ok(
            r#"line a.csv hline=0,20:"limit" span=2:5:"heating, on" note="start"@1,2 arrow="peak"@2024-03-01,30 x2="1240/x" x2label=eV"#,
        );
        let p = &s.panels[0];
        assert_eq!(p.hlines, vec![0.0, 20.0]);
        assert_eq!(p.spans, vec![(2.0, 5.0)]);
        assert_eq!(p.ref_labels.len(), 2);
        assert_eq!(p.ref_labels[0].text, "limit");
        assert_eq!(p.ref_labels[1].text, "heating, on");
        assert_eq!(p.notes.len(), 2);
        assert_eq!(
            (p.notes[0].x, p.notes[0].y, p.notes[0].arrow),
            (1.0, 2.0, false)
        );
        assert!(p.notes[1].arrow && p.notes[1].x > 1e9);
        assert_eq!(p.x2.as_ref().unwrap().value, "1240/x");
        assert_eq!(p.x2label.as_deref(), Some("eV"));
    }

    #[test]
    fn error_spans_point_at_the_word() {
        let src = "line a.csv colour=mauve";
        let e = err(src);
        assert_eq!(&src[e.span.unwrap()], "mauve");
        // In a list, the wrong one.
        let src = "line a.csv y=b,c color=orange,mauve";
        assert_eq!(&src[err(src).span.unwrap()], "mauve");
    }

    #[test]
    fn grouped_data_options() {
        let s = ok(
            r#"strip f.csv x=bed y=width by=bed groups="Full sun",Cold mean=label jitter horizontal highlight=Cold"#,
        );
        let p = &s.panels[0];
        assert_eq!(p.kind, PlotKind::Strip);
        let l = &p.layers[0];
        assert_eq!(l.by.as_ref().unwrap().value, "bed");
        assert_eq!(l.groups.as_ref().unwrap().value, ["Full sun", "Cold"]);
        assert_eq!(p.mean.as_ref().unwrap().value, MeanMark::Label);
        assert!(p.jitter && p.horizontal);
        assert_eq!(p.highlight.as_ref().unwrap().value, ["Cold"]);
        assert_eq!(
            ok("strip f.csv mean").panels[0]
                .mean
                .as_ref()
                .unwrap()
                .value,
            MeanMark::Line
        );
        assert!(ok("strip f.csv mean=off").panels[0].mean.is_none());
        let p = &ok("strip f.csv mean median=label").panels[0];
        assert_eq!(p.median.as_ref().unwrap().value, MeanMark::Label);
        assert!(
            err("strip f.csv median=avg")
                .message
                .contains("median=label")
        );
        assert!(err("barh f.csv").message.contains("horizontal"));
        assert!(err("strip f.csv mean=avg").message.contains("mean=label"));
        assert!(err("strip f.csv by=a,b").message.contains("one column"));
    }

    #[test]
    fn analysis_options_of_round_7() {
        let s = ok(
            "line f.csv y=a baseline=poly2,0:5,95:100 showbaseline norm=area cross=curves fitstats=bl",
        );
        let (p, l) = (&s.panels[0], &s.panels[0].layers[0]);
        let b = l.baseline.as_ref().unwrap();
        assert_eq!(
            (b.degree, b.ranges.clone()),
            (2, vec![(0.0, 5.0), (95.0, 100.0)])
        );
        assert!(l.show_baseline);
        assert_eq!(l.norm.as_ref().unwrap().value, Norm::Area);
        assert_eq!(p.cross.as_ref().unwrap().value, Cross::Curves);
        assert_eq!(p.fit_stats, Some(LegendPos::BottomLeft));
        let s = ok("line f.csv baseline=linear cross=50,80 fitstats");
        assert!(
            s.panels[0].layers[0]
                .baseline
                .as_ref()
                .unwrap()
                .ranges
                .is_empty()
        );
        assert_eq!(
            s.panels[0].cross.as_ref().unwrap().value,
            Cross::Levels(vec![50.0, 80.0])
        );
        assert_eq!(s.panels[0].fit_stats, Some(LegendPos::Best));
        // Residual plot types, also after + without a file.
        let s = ok(
            "scatter f.csv x=a y=b fit=linear + resid y=b fit=linear axis=right ; qq f.csv y=b fit=exp",
        );
        assert_eq!(s.panels[0].layers[1].kind, PlotKind::Resid);
        assert_eq!(s.panels[0].layers[1].file.value, "f.csv");
        assert_eq!(s.panels[1].kind, PlotKind::Qq);
        assert!(
            err("line f.csv rolling=5").message.contains("smooth=5")
                || err("line f.csv rolling=5").message.contains("smooth=")
        );
        assert!(err("line f.csv norm=big").message.contains("range"));
        assert!(
            err("line f.csv baseline=0:5,poly2")
                .message
                .contains("shape first")
        );
        assert!(err("line f.csv cross=high").message.contains("cross=50"));
        assert!(err("line f.csv fitstats=out").message.contains("corner"));
        assert!(
            err("line showbaseline f.csv")
                .message
                .contains("after the file")
        );
    }

    #[test]
    fn round_8_options() {
        // Colour by a number column.
        let s = ok("scatter f.csv x=a y=b c=temp cmap=magma zlabel=T");
        assert_eq!(s.panels[0].layers[0].c.as_ref().unwrap().value, "temp");
        assert!(
            err("scatter f.csv x=a y=b c=t,u")
                .message
                .contains("one number column")
        );
        // A residuals histogram is a flag on resid; `hist` after + is still the type.
        let s = ok("resid f.csv x=a y=b fit=linear hist bins=12");
        assert!(s.panels[0].resid_hist);
        assert_eq!(s.panels[0].bins, Some(12));
        let s = ok("line f.csv x=a y=b + hist g.csv y=b");
        assert!(!s.panels[0].resid_hist);
        assert_eq!(s.panels[0].layers[1].kind, PlotKind::Hist);
        // What a crossing's label shows.
        let s = ok("line f.csv y=a,b cross=curves crosslabel=xy");
        assert_eq!(
            s.panels[0].cross_label.as_ref().unwrap().value,
            CrossLabel::Xy
        );
        assert_eq!(CrossLabel::Xy.text(1.5, 20.0), "(1.5, 20)");
        assert_eq!(CrossLabel::Off.text(1.5, 20.0), "");
        // A broken axis, panel sizes and one legend for the figure.
        let s = ok(
            "bar f.csv x=a y=b ybreak=20:460 ; line f.csv y=b --layout=1x2 --widths=2,1 --heights=1 --legend=bottom",
        );
        assert_eq!(s.panels[0].ybreak.as_ref().unwrap().value, (20.0, 460.0));
        assert_eq!(s.figure.widths, vec![2.0, 1.0]);
        assert_eq!(s.figure.heights, vec![1.0]);
        assert_eq!(s.figure.legend, Some(SharedLegend::Bottom));
        assert_eq!(
            ok("line f.csv y=b --legend=off").figure.legend,
            Some(SharedLegend::Panels)
        );
        assert!(
            err("bar f.csv y=b ybreak=80:20")
                .message
                .contains("ybreak=20:80")
        );
        assert!(
            err("bar f.csv y=b ybreak=20")
                .message
                .contains("ybreak=20:80")
        );
        assert!(
            err("line f.csv y=b --widths=2,0")
                .message
                .contains("above zero")
        );
        assert!(
            err("line f.csv y=b --legend=left")
                .message
                .contains("top, bottom or")
        );
        assert!(
            err("line f.csv cross=curves crosslabel=z")
                .message
                .contains("x, y, xy or off")
        );
    }

    #[test]
    fn lists_follow_the_columns_and_x_lists_pair_up() {
        let s = ok("line a.csv y=b,c color=orange,blue style=solid,dashed label=B,C");
        let l = &s.panels[0].layers[0];
        assert_eq!(l.color.len(), 2);
        assert_eq!(l.style, vec![LineStyle::Solid, LineStyle::Dashed]);
        assert_eq!(l.label, vec!["B".to_owned(), "C".to_owned()]);
        // The new names: dash= for style=, --layout= (--grid= is refused now).
        let s = ok("line a.csv y=b,c dash=dotted,dashed ; line a.csv y=b --layout=2x1");
        assert_eq!(
            s.panels[0].layers[0].style,
            vec![LineStyle::Dotted, LineStyle::Dashed]
        );
        assert_eq!(s.figure.grid, Some((2, 1)));
        // A quoted comma stays inside one label.
        let s = ok(r#"line a.csv y=b label="Run A, warm""#);
        assert_eq!(s.panels[0].layers[0].label, vec!["Run A, warm".to_owned()]);
        // x=t1,t2 y=a,b: two layers, each with its own colour.
        let s = ok("scatter a.csv x=t1,t2 y=a,b color=orange,blue err=e1,e2");
        let ls = &s.panels[0].layers;
        assert_eq!(ls.len(), 2);
        assert_eq!(
            (
                ls[1].x.as_ref().unwrap().value.as_str(),
                ls[1].y[0].value.as_str()
            ),
            ("t2", "b")
        );
        assert_eq!(ls[1].color, vec![Color::parse("blue").unwrap()]);
        assert_eq!(ls[1].err[0].value, "e2");
        // A comma inside brackets or backticks is part of a calculation, not a new column.
        let s = ok("line a.csv y=smooth(temp,5),`a,b`,c smooth=3");
        let ys: Vec<_> = s.panels[0].layers[0]
            .y
            .iter()
            .map(|y| y.value.as_str())
            .collect();
        assert_eq!(ys, ["smooth(temp,5)", "`a,b`", "c"]);
        assert_eq!(s.panels[0].layers[0].smooth, Some(3));
        // Counts that don't match point at the x columns.
        let src = "line a.csv x=t1,t2 y=a,b,c";
        let e = err(src);
        assert!(e.message.contains("pair up"), "{}", e.message);
        assert_eq!(&src[e.span.unwrap()], "t1,t2");
    }
}
