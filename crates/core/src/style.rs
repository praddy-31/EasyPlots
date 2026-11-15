//! The look of a plot: fonts, sizes, line widths and colours. All lengths are in points
//! (1/72 inch), so a figure looks the same at any DPI and in PDF.
//!
//! Themes (`--theme=` or `style.theme`):
//! - `paper`: journal look. Black on white, a full frame with inward ticks on all four
//!   sides, no grid, STIX Two.
//! - `light`: screen/slides. White, left and bottom axes only, outward ticks, a light grid,
//!   Source Sans.
//! - `dark`: as `light` on a dark background.
//! - `palette`: as `light`/`dark` in the colours of the current Palette theme.

use crate::Error;
use crate::render::text::Font;
use crate::spec::Color;

/// The Okabe–Ito palette: eight colours that stay distinguishable for colour-blind readers.
/// Yellow is last because it is hard to see on white.
pub const OKABE_ITO: [Color; 8] = [
    Color(0x00, 0x72, 0xb2), // blue
    Color(0xe6, 0x9f, 0x00), // orange
    Color(0x00, 0x9e, 0x73), // bluish green
    Color(0xd5, 0x5e, 0x00), // vermillion
    Color(0xcc, 0x79, 0xa7), // reddish purple
    Color(0x56, 0xb4, 0xe9), // sky blue
    Color(0xf0, 0xe4, 0x42), // yellow
    Color(0x00, 0x00, 0x00), // black
];

pub const THEMES: [&str; 4] = ["paper", "light", "dark", "palette"];

#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub theme: &'static str,
    pub font: Font,
    /// Axis labels and legend text.
    pub font_size: f64,
    pub tick_font_size: f64,
    pub title_font_size: f64,
    pub line_width: f64,
    pub marker_size: f64,
    pub axis_width: f64,
    pub major_tick: f64,
    pub minor_tick: f64,
    pub tick_width: f64,
    pub minor_tick_width: f64,
    /// Ticks point into the plot (paper) or out of it (screen themes).
    pub ticks_inward: bool,
    /// A full frame with ticks on all four sides, or only left and bottom axes.
    pub full_frame: bool,
    pub minor_ticks: bool,
    pub grid: bool,
    pub grid_color: Color,
    pub grid_width: f64,
    pub foreground: Color,
    pub background: Color,
    pub legend_frame: Color,
    pub palette: Vec<Color>,
    /// Line styles and markers per series, in order (from a style file; empty: solid lines,
    /// the usual markers).
    pub line_styles: Vec<crate::spec::LineStyle>,
    pub markers: Vec<crate::spec::Marker>,
    /// Space around the whole figure.
    pub pad: f64,
}

impl Style {
    /// The `paper` theme: black on white, inward ticks on all four sides, no grid.
    pub fn paper(font_size: f64, line_width: f64, grid: bool) -> Style {
        Style {
            theme: "paper",
            font: Font::Stix,
            font_size,
            tick_font_size: font_size * 0.9,
            title_font_size: font_size * 1.1,
            line_width,
            marker_size: (line_width * 3.2).max(3.5),
            axis_width: 0.8,
            major_tick: 3.5,
            minor_tick: 2.0,
            tick_width: 0.8,
            minor_tick_width: 0.6,
            ticks_inward: true,
            full_frame: true,
            minor_ticks: true,
            grid,
            grid_color: Color(0xd9, 0xd9, 0xd9),
            grid_width: 0.5,
            foreground: Color(0, 0, 0),
            background: Color(255, 255, 255),
            legend_frame: Color(0xbf, 0xbf, 0xbf),
            palette: OKABE_ITO.to_vec(),
            line_styles: Vec::new(),
            markers: Vec::new(),
            pad: 5.0,
        }
    }

    /// Screen look on any background: left/bottom axes, outward ticks, a grid.
    fn screen(
        theme: &'static str,
        font_size: f64,
        line_width: f64,
        background: Color,
        foreground: Color,
        grid_color: Color,
        palette: Vec<Color>,
    ) -> Style {
        Style {
            theme,
            font: Font::SourceSans,
            ticks_inward: false,
            full_frame: false,
            minor_ticks: false,
            grid: true,
            grid_color,
            grid_width: 0.6,
            foreground,
            background,
            legend_frame: grid_color,
            palette,
            ..Style::paper(font_size, line_width * 1.2, true)
        }
    }

    /// Build the style from config values and `--theme` / `--font` overrides. `font` is a
    /// font name, or "auto" for the theme's own font. `palette_dir` is where Palette keeps
    /// its colour files (for the `palette` theme).
    pub fn from_settings(
        theme: &str,
        font: &str,
        font_size: f64,
        line_width: f64,
        grid: bool,
        palette_dir: &std::path::Path,
    ) -> Result<Style, Error> {
        if !(4.0..=36.0).contains(&font_size) {
            return Err(Error::new("font_size must be between 4 and 36 points"));
        }
        let mut style = match theme.to_ascii_lowercase().as_str() {
            "paper" => Style::paper(font_size, line_width, grid),
            "light" => Style::screen(
                "light",
                font_size,
                line_width,
                Color(255, 255, 255),
                Color(0x26, 0x26, 0x26),
                Color(0xe6, 0xe6, 0xe6),
                OKABE_ITO.to_vec(),
            ),
            "dark" => {
                let mut palette = OKABE_ITO.to_vec();
                // Black disappears on dark; brighten the darkest blue a little.
                palette[0] = Color(0x3d, 0x9c, 0xe0);
                palette[7] = Color(0xe6, 0xe6, 0xe6);
                Style::screen(
                    "dark",
                    font_size,
                    line_width,
                    Color(0x1e, 0x1e, 0x1e),
                    Color(0xe6, 0xe6, 0xe6),
                    Color(0x3a, 0x3a, 0x3a),
                    palette,
                )
            }
            "palette" => {
                let t = crate::ui_theme::resolve("palette", palette_dir).dark;
                let c = |k: &str| Color::parse(&t.colors[k]).unwrap_or(Color(0x80, 0x80, 0x80));
                let palette = [
                    "blue", "orange", "green", "red", "magenta", "cyan", "yellow",
                ]
                .iter()
                .map(|k| c(k))
                .chain([c("foreground")])
                .collect();
                Style::screen(
                    "palette",
                    font_size,
                    line_width,
                    c("background"),
                    c("foreground"),
                    c("selection"),
                    palette,
                )
            }
            _ => {
                return Err(Error::new(format!(
                    "unknown theme \"{theme}\" — use paper, light, dark or palette"
                )));
            }
        };
        // `style.grid` in config.toml applies to paper; the screen themes always start with a
        // grid. Either way, `grid=on|off` on a plot decides for that plot.
        if !font.eq_ignore_ascii_case("auto") {
            style.font = Font::from_name(font).ok_or_else(|| {
                let names: Vec<&str> = Font::ALL.iter().map(|f| f.name()).collect();
                Error::new(format!(
                    "unknown font \"{font}\" — use {} (or auto for the theme's font)",
                    names.join(", ")
                ))
            })?;
        }
        Ok(style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn themes_and_fonts() {
        let none = Path::new("/nonexistent");
        let paper = Style::from_settings("paper", "auto", 10.0, 1.25, false, none).unwrap();
        assert_eq!(
            (paper.font, paper.full_frame, paper.grid),
            (Font::Stix, true, false)
        );
        let dark = Style::from_settings("dark", "Latin Modern", 10.0, 1.25, false, none).unwrap();
        assert_eq!(
            (dark.font, dark.full_frame, dark.grid),
            (Font::LatinModern, false, true)
        );
        assert_eq!(dark.background, Color(0x1e, 0x1e, 0x1e));
        assert!(Style::from_settings("neon", "auto", 10.0, 1.25, false, none).is_err());
        assert!(Style::from_settings("paper", "Comic", 10.0, 1.25, false, none).is_err());
    }

    #[test]
    fn palette_theme_uses_palette_colours() {
        let dir = std::env::temp_dir().join(format!("easyplots-plotpal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("easyplots.json"),
            r##"{"theme":"x","mode":"dark","background":"#102030","blue":"#0000ff"}"##,
        )
        .unwrap();
        let s = Style::from_settings("palette", "auto", 10.0, 1.25, false, &dir).unwrap();
        assert_eq!(s.background, Color(0x10, 0x20, 0x30));
        assert_eq!(s.palette[0], Color(0, 0, 0xff));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
