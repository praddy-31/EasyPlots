//! The bundled plot fonts, and text measurement for layout (how wide a label is, how tall
//! the letters are), so margins fit the labels exactly.
//!
//! Three families: STIX Two Text (journal look, the `paper` default), Latin Modern (the
//! classic LaTeX look) and Source Sans 3 (clean sans-serif, the screen themes' default).
//! A character a font lacks (Latin Modern has no Greek, say) is drawn from STIX Two Text,
//! and maths symbols none of them have (≈, ∝, ∞…) from a small cut of STIX Two Math;
//! measurement follows the same chain.

use std::sync::OnceLock;

use ttf_parser::Face;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    Stix,
    LatinModern,
    SourceSans,
}

const STIX: [&[u8]; 3] = [
    include_bytes!("../../../../assets/fonts/STIXTwoText-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/STIXTwoText-Italic.ttf"),
    include_bytes!("../../../../assets/fonts/STIXTwoText-Bold.ttf"),
];
const LATIN_MODERN: [&[u8]; 3] = [
    include_bytes!("../../../../assets/fonts/lmroman10-regular.otf"),
    include_bytes!("../../../../assets/fonts/lmroman10-italic.otf"),
    include_bytes!("../../../../assets/fonts/lmroman10-bold.otf"),
];
/// Greek, arrows and maths operators only (see `assets/fonts/make_static.py`).
const MATH: &[u8] = include_bytes!("../../../../assets/fonts/STIXTwoMath-Symbols.otf");
const MATH_FAMILY: &str = "STIX Two Math";
const SOURCE_SANS: [&[u8]; 3] = [
    include_bytes!("../../../../assets/fonts/SourceSans3-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/SourceSans3-Italic.ttf"),
    include_bytes!("../../../../assets/fonts/SourceSans3-Bold.ttf"),
];

impl Font {
    pub const ALL: [Font; 3] = [Font::Stix, Font::LatinModern, Font::SourceSans];

    /// The family name the SVG asks for (as the font database knows it).
    pub fn family(self) -> &'static str {
        match self {
            Font::Stix => "STIX Two Text",
            Font::LatinModern => "Latin Modern Roman",
            Font::SourceSans => "Source Sans 3",
        }
    }

    /// The name people type: `--font="Latin Modern"`.
    pub fn name(self) -> &'static str {
        match self {
            Font::Stix => "STIX Two",
            Font::LatinModern => "Latin Modern",
            Font::SourceSans => "Source Sans",
        }
    }

    pub fn from_name(s: &str) -> Option<Font> {
        let f: String = s
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        Some(match f.as_str() {
            "stix" | "stixtwo" | "stixtwotext" => Font::Stix,
            "latinmodern" | "lm" | "lmroman" | "lmroman10" | "computermodern" | "cm" | "latex" => {
                Font::LatinModern
            }
            "sourcesans" | "sourcesans3" | "sourcesanspro" | "sans" => Font::SourceSans,
            _ => return None,
        })
    }

    fn files(self) -> [&'static [u8]; 3] {
        match self {
            Font::Stix => STIX,
            Font::LatinModern => LATIN_MODERN,
            Font::SourceSans => SOURCE_SANS,
        }
    }
}

/// Every bundled font file, for the renderer's font database.
pub fn all_font_data() -> impl Iterator<Item = &'static [u8]> {
    Font::ALL.into_iter().flat_map(|f| f.files()).chain([MATH])
}

fn math_metrics() -> &'static Metrics {
    static M: OnceLock<Metrics> = OnceLock::new();
    M.get_or_init(|| {
        let face = Face::parse(MATH, 0).expect("bundled font is valid");
        let units = f64::from(face.units_per_em());
        Metrics { face, units }
    })
}

/// The family to draw `c` with when `font` lacks it, or `None` if `font` has it.
pub fn fallback_family(font: Font, c: char, italic: bool) -> Option<&'static str> {
    if c == ' ' || metrics(font, italic).face.glyph_index(c).is_some() {
        None
    } else if metrics(Font::Stix, italic).face.glyph_index(c).is_some() {
        Some(Font::Stix.family())
    } else {
        Some(MATH_FAMILY)
    }
}

struct Metrics {
    face: Face<'static>,
    units: f64,
}

/// Faces by font and style (regular, italic), parsed once.
fn metrics(font: Font, italic: bool) -> &'static Metrics {
    static M: OnceLock<Vec<Metrics>> = OnceLock::new();
    let all = M.get_or_init(|| {
        Font::ALL
            .iter()
            .flat_map(|f| [f.files()[0], f.files()[1]])
            .map(|data| {
                let face = Face::parse(data, 0).expect("bundled font is valid");
                let units = f64::from(face.units_per_em());
                Metrics { face, units }
            })
            .collect()
    });
    let i = Font::ALL.iter().position(|f| *f == font).unwrap_or(0) * 2 + usize::from(italic);
    &all[i]
}

fn advance(m: &Metrics, c: char) -> Option<f64> {
    m.face
        .glyph_index(c)
        .and_then(|g| m.face.glyph_hor_advance(g))
        .map(|a| f64::from(a) / m.units)
}

/// Width of `text` set at `size` points.
pub fn width(font: Font, text: &str, size: f64) -> f64 {
    styled_width(font, text, size, false)
}

/// Width of `text` in the regular or italic style.
pub fn styled_width(font: Font, text: &str, size: f64, italic: bool) -> f64 {
    let m = metrics(font, italic);
    let fallback = metrics(Font::Stix, italic);
    text.chars()
        .map(|c| {
            advance(m, c)
                .or_else(|| advance(fallback, c))
                .or_else(|| advance(math_metrics(), c))
                .unwrap_or(0.5)
        })
        .sum::<f64>()
        * size
}

/// Height of capital letters (and digits) above the baseline.
pub fn cap_height(font: Font, size: f64) -> f64 {
    let m = metrics(font, false);
    let cap = m.face.capital_height().map_or(m.units * 0.66, f64::from);
    cap / m.units * size
}

/// Distance from the baseline to the bottom of letters like "g" (positive).
pub fn descent(font: Font, size: f64) -> f64 {
    let m = metrics(font, false);
    -f64::from(m.face.descender()) / m.units * size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_sensibly() {
        for font in Font::ALL {
            let w = width(font, "0.5", 10.0);
            assert!(w > 7.0 && w < 15.0, "{font:?} {w}");
            assert!(width(font, "WWW", 10.0) > width(font, "iii", 10.0));
            let c = cap_height(font, 10.0);
            assert!(c > 5.5 && c < 7.5, "{font:?} {c}");
            assert!(descent(font, 10.0) > 1.0);
        }
    }

    #[test]
    fn missing_glyphs_use_stix_widths() {
        // Latin Modern has no Greek; the width comes from STIX instead of a guess.
        let lm = width(Font::LatinModern, "α", 10.0);
        let stix = width(Font::Stix, "α", 10.0);
        assert!((lm - stix).abs() < 1e-9);
    }

    #[test]
    fn names() {
        assert_eq!(Font::from_name("Latin Modern"), Some(Font::LatinModern));
        assert_eq!(Font::from_name("STIX Two"), Some(Font::Stix));
        assert_eq!(Font::from_name("source-sans"), Some(Font::SourceSans));
        assert_eq!(Font::from_name("Comic"), None);
    }
}
