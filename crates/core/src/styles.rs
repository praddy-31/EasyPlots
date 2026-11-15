//! Style files: `--style=thesis` reads `styles/thesis.toml` next to `config.toml`, so one
//! word sets a whole look. Any key can be left out; the command still wins over the file,
//! and the file wins over `config.toml`.
//!
//! ```toml
//! theme      = "paper"            # paper | light | dark | palette
//! font       = "Latin Modern"
//! font_size  = 9
//! line_width = 1.0
//! grid       = false
//! legend     = "best"
//! size       = "3.5x2.6in"
//! dpi        = 600
//! format     = "pdf"
//! colors     = ["black", "#d62728", "blue"]   # the series colours, in order
//! line_styles = ["solid", "dashed"]           # the series line styles, in order
//! markers    = ["o", "s"]                     # the series markers, in order
//! logx       = false
//! logy       = true
//! xstep      = 5                             # tick steps (as in xlim=::5)
//! ystep      = 0.5
//! ```
//!
//! ⌘D on a preview writes one of these from the command: only the look (theme, font, size,
//! colours, line styles, markers, line width, legend, grid, log axes, tick steps), never
//! files, columns, labels or titles, so it fits any data.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::spec::{Color, Format, LegendPos, LineStyle, Marker, NAMED_COLORS, PlotSpec, Spanned};
use crate::{Config, Error};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(skip_serializing_if = "Option::is_none")]
    theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    grid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    legend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dpi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    colors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_styles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    markers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logx: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xstep: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ystep: Option<f64>,
}

/// A loaded style file.
#[derive(Debug, Clone)]
pub struct StyleFile {
    raw: Raw,
    path: PathBuf,
    /// The series colours, if the file sets them.
    pub colors: Option<Vec<Color>>,
    /// The series line styles and markers, in order (empty: the usual defaults).
    pub line_styles: Vec<LineStyle>,
    pub markers: Vec<Marker>,
}

/// The style names available in `dir` (file names without `.toml`), sorted.
pub fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    name.strip_suffix(".toml").map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn load(dir: &Path, name: &Spanned<String>) -> Result<StyleFile, Error> {
    let path = dir.join(format!("{}.toml", name.value));
    let text = std::fs::read_to_string(&path).map_err(|_| {
        let known = names(dir);
        let list = if known.is_empty() {
            format!(" — create {} to make one", path.display())
        } else {
            format!(" (styles: {})", known.join(", "))
        };
        Error::at(
            format!(
                "no style \"{}\"{}{list}",
                name.value,
                crate::error::suggestion(&name.value, known.iter().map(String::as_str))
            ),
            name.span.clone(),
        )
    })?;
    let raw: Raw = toml::from_str(&text).map_err(|e| {
        Error::at(
            format!("{}: {}", path.display(), e.message()),
            name.span.clone(),
        )
    })?;
    let colors = match &raw.colors {
        None => None,
        Some(list) if list.is_empty() => None,
        Some(list) => Some(
            list.iter()
                .map(|c| {
                    Color::parse(c).ok_or_else(|| {
                        Error::at(
                            format!("{}: unknown colour \"{c}\"", path.display()),
                            name.span.clone(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    let bad = |what: &str, v: &str| {
        Error::at(
            format!("{}: unknown {what} \"{v}\"", path.display()),
            name.span.clone(),
        )
    };
    let line_styles = raw
        .line_styles
        .iter()
        .flatten()
        .map(|s| LineStyle::from_name(s).ok_or_else(|| bad("line style", s)))
        .collect::<Result<_, _>>()?;
    let markers = raw
        .markers
        .iter()
        .flatten()
        .map(|s| Marker::from_name(s).ok_or_else(|| bad("marker", s)))
        .collect::<Result<_, _>>()?;
    Ok(StyleFile {
        raw,
        path,
        colors,
        line_styles,
        markers,
    })
}

/// A style file's text for the look of `spec` (⌘D on a preview): what the command set for
/// the look, on top of the style file it used (if any). Files, columns, labels and titles
/// are left out.
pub fn from_command(spec: &PlotSpec, base: Option<&StyleFile>) -> Result<String, Error> {
    let mut raw = base.map(|b| b.raw.clone()).unwrap_or_default();
    let f = &spec.figure;
    if let Some(t) = &f.theme {
        raw.theme = Some(t.clone());
    }
    if let Some(v) = &f.font {
        raw.font = Some(v.clone());
    }
    if let Some(name) = &f.size_name {
        raw.size = Some(name.clone());
    } else if let Some((w, h)) = f.size {
        raw.size = Some(format!("{}x{}in", trim(w / 72.0), trim(h / 72.0)));
    }
    if let Some(p) = spec.panels.first() {
        if let Some(l) = p.legend {
            raw.legend = Some(legend_name(l).to_owned());
        }
        if let Some(g) = p.grid {
            raw.grid = Some(g);
        }
        if p.logx {
            raw.logx = Some(true);
        }
        if p.logy {
            raw.logy = Some(true);
        }
        if let Some(s) = &p.xstep {
            raw.xstep = Some(s.value);
        }
        if let Some(s) = &p.ystep {
            raw.ystep = Some(s.value);
        }
        // Per-series settings, in series order, as far as every layer before sets them.
        let each = |get: &dyn Fn(&crate::spec::LayerSpec) -> Vec<String>| -> Vec<String> {
            let mut out = Vec::new();
            for l in &p.layers {
                let v = get(l);
                if v.is_empty() {
                    break;
                }
                let columns = l.y.len().max(1);
                out.extend((0..columns).filter_map(|k| crate::spec::per_column(&v, k)));
            }
            out
        };
        let colors = each(&|l| l.color.iter().map(|c| color_name(*c)).collect());
        if !colors.is_empty() {
            raw.colors = Some(colors);
        }
        let styles = each(&|l| l.style.iter().map(|s| style_name(*s).to_owned()).collect());
        if !styles.is_empty() {
            raw.line_styles = Some(styles);
        }
        let markers = each(&|l| {
            l.marker
                .iter()
                .map(|m| marker_name(*m).to_owned())
                .collect()
        });
        if !markers.is_empty() {
            raw.markers = Some(markers);
        }
        if let Some(w) = p.layers.first().and_then(|l| l.width) {
            raw.line_width = Some(w);
        }
    }
    if raw == Raw::default() {
        return Err(Error::new(
            "this command sets nothing about the look yet (theme, font, size, colours, styles…), so there is no style to keep — use ⌘⇧D to keep the whole command as a preset",
        ));
    }
    toml::to_string(&raw).map_err(|e| Error::new(format!("cannot write the style: {e}")))
}

/// Write `styles/NAME.toml`, refusing to replace an existing style.
pub fn save(dir: &Path, name: &str, text: &str) -> Result<PathBuf, Error> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(Error::new(
            "a style name is one word of letters, digits, - or _, e.g. thesis",
        ));
    }
    let path = dir.join(format!("{name}.toml"));
    if path.exists() {
        return Err(Error::new(format!(
            "there is already a style \"{name}\" — pick another name (delete styles in Settings → Presets & styles)"
        )));
    }
    std::fs::create_dir_all(dir)
        .and_then(|_| {
            std::fs::write(
                &path,
                format!("# Saved from the launcher (⌘D). Used with --style={name}.\n{text}"),
            )
        })
        .map_err(|e| Error::new(format!("cannot write {}: {e}", path.display())))?;
    Ok(path)
}

/// Delete `styles/NAME.toml`.
pub fn delete(dir: &Path, name: &str) -> Result<(), Error> {
    if name.contains(['/', '\\', '.']) {
        return Err(Error::new("not a style name"));
    }
    std::fs::remove_file(dir.join(format!("{name}.toml")))
        .map_err(|e| Error::new(format!("cannot delete style \"{name}\": {e}")))
}

fn trim(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn color_name(c: Color) -> String {
    NAMED_COLORS
        .iter()
        .find(|(_, v)| *v == c)
        .map_or_else(|| c.hex(), |(n, _)| (*n).to_owned())
}

fn style_name(s: LineStyle) -> &'static str {
    match s {
        LineStyle::Solid => "solid",
        LineStyle::Dashed => "dashed",
        LineStyle::Dotted => "dotted",
        LineStyle::DashDot => "dashdot",
    }
}

fn marker_name(m: Marker) -> &'static str {
    m.name()
}

fn legend_name(l: LegendPos) -> &'static str {
    match l {
        LegendPos::Best => "best",
        LegendPos::TopRight => "tr",
        LegendPos::TopLeft => "tl",
        LegendPos::BottomRight => "br",
        LegendPos::BottomLeft => "bl",
        LegendPos::Out => "out",
        LegendPos::Off => "off",
    }
}

impl StyleFile {
    /// `config` with this file's settings on top.
    pub fn apply(&self, config: &Config) -> Config {
        let mut c = config.clone();
        let r = self.raw.clone();
        if let Some(v) = r.theme {
            c.style.theme = v;
        }
        if let Some(v) = r.font {
            c.style.font = v;
        }
        if let Some(v) = r.font_size {
            c.style.font_size = v;
        }
        if let Some(v) = r.line_width {
            c.style.line_width = v;
        }
        if let Some(v) = r.grid {
            c.style.grid = v;
        }
        if let Some(v) = r.legend {
            c.style.legend = v;
        }
        if let Some(v) = r.size {
            c.output.size = v;
        }
        if let Some(v) = r.dpi {
            c.output.dpi = v;
        }
        c
    }

    /// Log axes and tick steps go into the command's panels (where it doesn't set them).
    pub fn apply_to_spec(&self, spec: &mut PlotSpec, span: std::ops::Range<usize>) {
        for p in &mut spec.panels {
            p.logx |= self.raw.logx == Some(true);
            p.logy |= self.raw.logy == Some(true);
            if p.xstep.is_none() {
                p.xstep = self.raw.xstep.map(|value| Spanned {
                    value,
                    span: span.clone(),
                });
            }
            if p.ystep.is_none() {
                p.ystep = self.raw.ystep.map(|value| Spanned {
                    value,
                    span: span.clone(),
                });
            }
        }
    }

    /// Whether this style sets the font size (which then wins over a journal size's).
    pub fn sets_font_size(&self) -> bool {
        self.raw.font_size.is_some()
    }

    pub fn format(&self) -> Result<Option<Format>, Error> {
        match &self.raw.format {
            None => Ok(None),
            Some(f) => Format::from_name(f).map(Some).ok_or_else(|| {
                Error::new(format!(
                    "{}: format \"{f}\" must be png, pdf or svg",
                    self.path.display()
                ))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example_is_a_valid_style_file() {
        let spec = include_str!("../../../docs/SPEC.md");
        let section = spec.find("### Style files").unwrap();
        let start = section + spec[section..].find("```toml\n").unwrap() + 8;
        let end = start + spec[start..].find("```").unwrap();
        let dir = std::env::temp_dir().join(format!("easyplots-specstyle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ex.toml"), &spec[start..end]).unwrap();
        let name = Spanned {
            value: "ex".to_owned(),
            span: 0..2,
        };
        let f = load(&dir, &name).unwrap();
        assert_eq!(f.colors.as_ref().map(Vec::len), Some(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_and_applies() {
        let dir = std::env::temp_dir().join(format!("easyplots-styles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("thesis.toml"),
            "font = \"Latin Modern\"\nsize = \"3.5x2.6in\"\nformat = \"pdf\"\ncolors = [\"black\", \"red\"]\n",
        )
        .unwrap();
        let name = |s: &str| Spanned {
            value: s.to_owned(),
            span: 0..s.len(),
        };
        let f = load(&dir, &name("thesis")).unwrap();
        let c = f.apply(&Config::default());
        assert_eq!(c.style.font, "Latin Modern");
        assert_eq!(c.output.size, "3.5x2.6in");
        assert_eq!(f.format().unwrap(), Some(Format::Pdf));
        assert_eq!(f.colors.as_ref().unwrap()[0], Color(0, 0, 0));
        let e = load(&dir, &name("thesys")).unwrap_err();
        assert!(
            e.message.contains("did you mean \"thesis\""),
            "{}",
            e.message
        );
        std::fs::write(dir.join("bad.toml"), "colour = 1\n").unwrap();
        assert!(load(&dir, &name("bad")).is_err());
        assert_eq!(names(&dir), ["bad", "thesis"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
