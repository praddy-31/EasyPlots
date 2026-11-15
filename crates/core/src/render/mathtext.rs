//! Maths in labels: text between `$…$` uses a small part of LaTeX (see `docs/SPEC.md`):
//! superscripts and subscripts (`x^2`, `T_{max}`, nested once more), Greek letters
//! (`\alpha`, `\Delta`), common symbols (`\pm`, `\times`, `\degree`, `\leq`, `\infty`…),
//! `\sqrt{…}`, `\frac{a}{b}` (written a/b), and `\mathrm{…}` / `\text{…}` for upright text.
//! Single Latin letters in maths are italic, as in LaTeX. `\$` is a plain dollar sign.
//!
//! A label becomes a list of runs (text with a style and a raise); `to_svg` writes them as
//! `<tspan>`s and `width` measures them, so layout stays exact.

use std::fmt::Write;

use super::escape;
use super::text::{self, Font};

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub italic: bool,
    /// 0 = baseline, 1 = superscript, -1 = subscript, ±2 = one level deeper.
    pub level: i32,
}

/// Size factor and baseline raise (in units of the font size) for a script level.
fn level_metrics(level: i32) -> (f64, f64) {
    match level {
        0 => (1.0, 0.0),
        1 => (0.7, 0.38),
        -1 => (0.7, -0.2),
        2 => (0.55, 0.6),
        -2 => (0.55, -0.32),
        l if l > 0 => (0.55, 0.6),
        _ => (0.55, -0.32),
    }
}

/// True if `s` has maths in it (so the plain fast path can be skipped).
pub fn has_math(s: &str) -> bool {
    s.contains('$')
}

/// Split a label into runs.
pub fn parse(s: &str) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut plain = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'$') => {
                chars.next();
                plain.push('$');
            }
            '$' => {
                let mut body = String::new();
                let mut closed = false;
                while let Some(m) = chars.next() {
                    if m == '\\' && chars.peek() == Some(&'$') {
                        chars.next();
                        body.push_str("\\$");
                    } else if m == '$' {
                        closed = true;
                        break;
                    } else {
                        body.push(m);
                    }
                }
                if !closed {
                    // A lone $ is just a dollar sign.
                    plain.push('$');
                    plain.push_str(&body);
                    continue;
                }
                push(&mut runs, std::mem::take(&mut plain), false, 0);
                math(&body.chars().collect::<Vec<_>>(), 0, false, &mut runs);
            }
            c => plain.push(c),
        }
    }
    push(&mut runs, plain, false, 0);
    runs
}

fn push(runs: &mut Vec<Run>, text: String, italic: bool, level: i32) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut()
        && last.italic == italic
        && last.level == level
    {
        last.text.push_str(&text);
        return;
    }
    runs.push(Run {
        text,
        italic,
        level,
    });
}

/// Read a `{…}` group (or one character / one command) starting at `i`; returns the group's
/// characters and the index after it.
fn group(cs: &[char], mut i: usize) -> (Vec<char>, usize) {
    while i < cs.len() && cs[i] == ' ' {
        i += 1;
    }
    if i >= cs.len() {
        return (Vec::new(), i);
    }
    if cs[i] == '{' {
        let mut depth = 0;
        let start = i + 1;
        while i < cs.len() {
            match cs[i] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return (cs[start..i].to_vec(), i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        return (cs[start..].to_vec(), cs.len());
    }
    if cs[i] == '\\' {
        let start = i;
        i += 1;
        while i < cs.len() && cs[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i == start + 1 && i < cs.len() {
            i += 1; // a one-character command like \, or \$
        }
        return (cs[start..i].to_vec(), i);
    }
    (vec![cs[i]], i + 1)
}

fn math(cs: &[char], level: i32, upright: bool, runs: &mut Vec<Run>) {
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        match c {
            ' ' => i += 1,
            '^' | '_' => {
                let (g, next) = group(cs, i + 1);
                let deeper = if c == '^' {
                    if level >= 0 { level + 1 } else { 1 }
                } else if level <= 0 {
                    level - 1
                } else {
                    -1
                };
                math(&g, deeper, upright, runs);
                i = next;
            }
            '{' => {
                let (g, next) = group(cs, i);
                math(&g, level, upright, runs);
                i = next;
            }
            '\\' => {
                let (cmd, next) = group(cs, i);
                let name: String = cmd[1..].iter().collect();
                i = next;
                match name.as_str() {
                    "mathrm" | "text" | "textrm" | "mathbf" | "operatorname" => {
                        let (g, n) = group(cs, i);
                        i = n;
                        if name == "text" || name == "textrm" {
                            push(runs, g.iter().collect(), false, level);
                        } else {
                            math(&g, level, true, runs);
                        }
                    }
                    "sqrt" => {
                        let (g, n) = group(cs, i);
                        i = n;
                        push(runs, "√".into(), false, level);
                        if g.len() > 1 {
                            push(runs, "(".into(), false, level);
                            math(&g, level, upright, runs);
                            push(runs, ")".into(), false, level);
                        } else {
                            math(&g, level, upright, runs);
                        }
                    }
                    "frac" => {
                        let (a, n) = group(cs, i);
                        let (b, n) = group(cs, n);
                        i = n;
                        math(&a, level, upright, runs);
                        push(runs, "/".into(), false, level);
                        math(&b, level, upright, runs);
                    }
                    _ => match symbol(&name) {
                        Some((s, italic)) => push_spaced(runs, s, italic && !upright, level),
                        None => push(runs, format!("\\{name}"), false, level),
                    },
                }
            }
            c => {
                let italic = c.is_ascii_alphabetic() && !upright;
                let c = if c == '-' { '−' } else { c };
                push_spaced(runs, &c.to_string(), italic, level);
                i += 1;
            }
        }
    }
}

const RELATIONS: &str = "=<>≤≥≠≈∼∝→←";
const BINARY: &str = "+−±∓×·÷";

/// Push maths text, with space around relations (a = b) and a smaller space around
/// binary operators (a + b), as LaTeX does. Not in scripts, and not for a sign (−3).
fn push_spaced(runs: &mut Vec<Run>, s: &str, italic: bool, level: i32) {
    let mut cs = s.chars();
    let (Some(c), None) = (cs.next(), cs.next()) else {
        return push(runs, s.into(), italic, level);
    };
    let prev = runs.last().and_then(|r| r.text.chars().last());
    let after_operand = prev.is_some_and(|p| {
        !p.is_whitespace() && !"([{,".contains(p) && !RELATIONS.contains(p) && !BINARY.contains(p)
    });
    let pad = if level != 0 {
        None
    } else if RELATIONS.contains(c) {
        Some(" ")
    } else if BINARY.contains(c) && after_operand {
        Some("\u{2005}")
    } else {
        None
    };
    match pad {
        Some(pad) => {
            if !prev.is_some_and(char::is_whitespace) {
                push(runs, pad.into(), false, level);
            }
            push(runs, s.into(), false, level);
            push(runs, pad.into(), false, level);
        }
        None => push(runs, s.into(), italic, level),
    }
}

/// LaTeX command → (text, italic). Lower-case Greek is italic, as in LaTeX.
fn symbol(name: &str) -> Option<(&'static str, bool)> {
    const GREEK: &[(&str, &str)] = &[
        ("alpha", "α"),
        ("beta", "β"),
        ("gamma", "γ"),
        ("delta", "δ"),
        ("epsilon", "ϵ"),
        ("varepsilon", "ε"),
        ("zeta", "ζ"),
        ("eta", "η"),
        ("theta", "θ"),
        ("vartheta", "ϑ"),
        ("iota", "ι"),
        ("kappa", "κ"),
        ("lambda", "λ"),
        ("mu", "μ"),
        ("nu", "ν"),
        ("xi", "ξ"),
        ("pi", "π"),
        ("rho", "ρ"),
        ("sigma", "σ"),
        ("tau", "τ"),
        ("upsilon", "υ"),
        ("phi", "ϕ"),
        ("varphi", "φ"),
        ("chi", "χ"),
        ("psi", "ψ"),
        ("omega", "ω"),
    ];
    const UPPER: &[(&str, &str)] = &[
        ("Gamma", "Γ"),
        ("Delta", "Δ"),
        ("Theta", "Θ"),
        ("Lambda", "Λ"),
        ("Xi", "Ξ"),
        ("Pi", "Π"),
        ("Sigma", "Σ"),
        ("Upsilon", "Υ"),
        ("Phi", "Φ"),
        ("Psi", "Ψ"),
        ("Omega", "Ω"),
    ];
    const SYMBOLS: &[(&str, &str)] = &[
        ("pm", "±"),
        ("mp", "∓"),
        ("times", "×"),
        ("cdot", "·"),
        ("div", "÷"),
        ("degree", "°"),
        ("circ", "∘"),
        ("infty", "∞"),
        ("leq", "≤"),
        ("le", "≤"),
        ("geq", "≥"),
        ("ge", "≥"),
        ("neq", "≠"),
        ("ne", "≠"),
        ("approx", "≈"),
        ("sim", "∼"),
        ("propto", "∝"),
        ("partial", "∂"),
        ("nabla", "∇"),
        ("sum", "∑"),
        ("int", "∫"),
        ("prod", "∏"),
        ("to", "→"),
        ("rightarrow", "→"),
        ("leftarrow", "←"),
        ("AA", "Å"),
        ("ell", "ℓ"),
        ("hbar", "ℏ"),
        ("prime", "′"),
        ("ldots", "…"),
        ("cdots", "⋯"),
        ("langle", "⟨"),
        ("rangle", "⟩"),
        ("%", "%"),
        ("$", "$"),
        ("_", "_"),
        (",", "\u{2009}"),
        (";", "\u{2005}"),
        (" ", " "),
        ("quad", "\u{2003}"),
        ("{", "{"),
        ("}", "}"),
        ("minus", "−"),
    ];
    if let Some((_, s)) = GREEK.iter().find(|(n, _)| *n == name) {
        return Some((s, true));
    }
    UPPER
        .iter()
        .chain(SYMBOLS)
        .find(|(n, _)| *n == name)
        .map(|(_, s)| (*s, false))
}

/// The label's text with maths turned into plain characters (for metadata and titles).
pub fn plain(s: &str) -> String {
    parse(s).into_iter().map(|r| r.text).collect()
}

/// Width of a label at `size` points.
pub fn width(font: Font, s: &str, size: f64) -> f64 {
    if !has_math(s) {
        return text::width(font, s, size);
    }
    parse(s)
        .iter()
        .map(|r| {
            let (scale, _) = level_metrics(r.level);
            text::styled_width(font, &r.text, size * scale, r.italic)
        })
        .sum()
}

/// True if some character of `s` must come from a fallback font.
pub fn needs_fallback(font: Font, s: &str) -> bool {
    s.chars()
        .any(|c| text::fallback_family(font, c, false).is_some())
}

/// The inside of a `<text>` element for a label in `font`: plain escaped text, or
/// `<tspan>`s. Characters `font` lacks get a fallback family (see `text::fallback_family`).
///
/// With `left` set, every piece gets its own `x` (starting at `left`, measured here). That
/// makes each piece a separate SVG text chunk, which is needed when fallbacks are mixed in:
/// resvg otherwise re-shapes a whole chunk in whichever font has the missing character.
pub fn to_svg(font: Font, s: &str, size: f64, left: Option<f64>) -> String {
    if !has_math(s) && !needs_fallback(font, s) {
        return escape(s);
    }
    let mut cursor = left;
    let runs = if has_math(s) {
        parse(s)
    } else {
        vec![Run {
            text: s.to_owned(),
            italic: false,
            level: 0,
        }]
    };
    let mut out = String::new();
    let mut raised = 0.0;
    for r in runs {
        let (scale, raise) = level_metrics(r.level);
        let shift = raise * size;
        // dy moves the baseline relative to the previous run, so undo the last raise.
        let mut dy = -(shift - raised);
        raised = shift;
        // Pieces of the run that need another font.
        let mut pieces: Vec<(Option<&str>, String)> = Vec::new();
        for c in r.text.chars() {
            let fam = text::fallback_family(font, c, r.italic);
            match pieces.last_mut() {
                Some((f, t)) if *f == fam => t.push(c),
                _ => pieces.push((fam, c.to_string())),
            }
        }
        for (fam, t) in pieces {
            out.push_str("<tspan");
            if let Some(x) = cursor.as_mut() {
                let _ = write!(out, " x=\"{}\"", super::n(*x));
                *x += text::styled_width(font, &t, size * scale, r.italic);
            }
            if dy.abs() > 1e-6 {
                let _ = write!(out, " dy=\"{}\"", super::n(dy));
                dy = 0.0;
            }
            if scale < 1.0 {
                let _ = write!(out, " font-size=\"{}\"", super::n(size * scale));
            }
            if r.italic {
                out.push_str(" font-style=\"italic\"");
            }
            if let Some(fam) = fam {
                let _ = write!(out, " font-family=\"'{fam}'\"");
            }
            let _ = write!(out, ">{}</tspan>", escape(&t));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &str) -> Vec<(String, bool, i32)> {
        parse(s)
            .into_iter()
            .map(|r| (r.text, r.italic, r.level))
            .collect()
    }

    #[test]
    fn plain_text_is_one_run() {
        assert_eq!(
            texts("Temperature (C)"),
            [("Temperature (C)".into(), false, 0)]
        );
        assert_eq!(texts("costs \\$5"), [("costs $5".into(), false, 0)]);
        assert_eq!(texts("only $5"), [("only $5".into(), false, 0)]);
    }

    #[test]
    fn scripts_greek_and_symbols() {
        assert_eq!(
            texts("$x^2$"),
            [("x".into(), true, 0), ("2".into(), false, 1)]
        );
        assert_eq!(
            texts("$T_{max}$ (K)"),
            [
                ("T".into(), true, 0),
                ("max".into(), true, -1),
                (" (K)".into(), false, 0)
            ]
        );
        assert_eq!(
            plain("$\\alpha_1 \\pm 0.5\\,\\mu\\mathrm{m}$"),
            "α1\u{2005}±\u{2005}0.5\u{2009}μm"
        );
        assert_eq!(texts("$\\mathrm{kg}$"), [("kg".into(), false, 0)]);
        assert_eq!(plain("$\\Delta T$"), "ΔT");
        assert_eq!(plain("$\\frac{a}{b}$"), "a/b");
        assert_eq!(plain("$\\sqrt{2}$"), "√2");
        assert_eq!(texts("$e^{x_1}$")[2], ("1".into(), false, -1));
    }

    #[test]
    fn svg_moves_baseline_and_back() {
        let svg = to_svg(Font::Stix, "$x^2$ m", 10.0, None);
        assert!(svg.contains("font-size=\"7\""), "{svg}");
        assert!(svg.contains("dy=\"-3.8\""), "{svg}");
        assert!(svg.contains("dy=\"3.8\""), "{svg}");
        assert!(width(Font::Stix, "$x^2$", 10.0) < width(Font::Stix, "x2", 10.0));
    }

    #[test]
    fn missing_glyphs_fall_back() {
        // Latin Modern has no Greek; STIX Two Text has no ≈.
        let svg = to_svg(Font::LatinModern, "$\\alpha \\approx 1$", 10.0, Some(0.0));
        assert!(svg.contains("font-family=\"'STIX Two Text'\">α"), "{svg}");
        assert!(svg.contains("font-family=\"'STIX Two Math'\">≈"), "{svg}");
        assert!(svg.contains("<tspan x=\"0\""), "{svg}");
        assert_eq!(to_svg(Font::Stix, "plain", 10.0, None), "plain");
        assert!(to_svg(Font::LatinModern, "50 μm", 10.0, None).contains("'STIX Two Text'"));
    }

    #[test]
    fn spaces_around_operators() {
        assert_eq!(plain("$a=b$"), "a = b");
        assert_eq!(plain("$a+b$"), "a\u{2005}+\u{2005}b");
        assert_eq!(plain("$-3$"), "−3");
        assert_eq!(plain("$x \\leq 1$"), "x ≤ 1");
        assert_eq!(plain("$10^{-3}$"), "10−3");
    }
}
