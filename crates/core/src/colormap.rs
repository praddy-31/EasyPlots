//! Colour maps for heatmaps and contours (`cmap=`): a value from low to high becomes a
//! colour along a scale. The anchor colours are samples of matplotlib's maps, joined by
//! straight lines in RGB.
//!
//! `viridis` (the default), `cividis`, `magma`, `plasma` and `inferno` go from dark to light
//! evenly and stay readable in grey and for colour-blind readers. `coolwarm` is diverging
//! (blue, white, red) for values around a middle; `blues`, `reds` and `gray` are single-hue.

use crate::spec::Color;

pub const NAMES: [&str; 9] = [
    "viridis", "cividis", "magma", "plasma", "inferno", "coolwarm", "blues", "reds", "gray",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colormap(&'static [u32]);

const VIRIDIS: &[u32] = &[
    0x440154, 0x482878, 0x3e4989, 0x31688e, 0x26828e, 0x1f9e89, 0x35b779, 0x6ece58, 0xb5de2b,
    0xfde725,
];
const CIVIDIS: &[u32] = &[
    0x00224e, 0x123570, 0x3b496c, 0x575d6d, 0x707173, 0x8a8678, 0xa59c74, 0xc3b369, 0xe1cc55,
    0xfee838,
];
const MAGMA: &[u32] = &[
    0x000004, 0x180f3d, 0x440f76, 0x721f81, 0x9e2f7f, 0xcd4071, 0xf1605d, 0xfd9668, 0xfeca8d,
    0xfcfdbf,
];
const PLASMA: &[u32] = &[
    0x0d0887, 0x46039f, 0x7201a8, 0x9c179e, 0xbd3786, 0xd8576b, 0xed7953, 0xfb9f3a, 0xfdca26,
    0xf0f921,
];
const INFERNO: &[u32] = &[
    0x000004, 0x1b0c41, 0x4a0c6b, 0x781c6d, 0xa52c60, 0xcf4446, 0xed6925, 0xfb9b06, 0xf7d13d,
    0xfcffa4,
];
const COOLWARM: &[u32] = &[
    0x3b4cc0, 0x6788ee, 0x9abbff, 0xc9d7f0, 0xedd1c2, 0xf7a889, 0xe26952, 0xb40426,
];
const BLUES: &[u32] = &[
    0xf7fbff, 0xdeebf7, 0xc6dbef, 0x9ecae1, 0x6baed6, 0x4292c6, 0x2171b5, 0x08519c, 0x08306b,
];
const REDS: &[u32] = &[
    0xfff5f0, 0xfee0d2, 0xfcbba1, 0xfc9272, 0xfb6a4a, 0xef3b2c, 0xcb181d, 0xa50f15, 0x67000d,
];
const GRAY: &[u32] = &[0x000000, 0xffffff];

impl Colormap {
    pub const DEFAULT: Colormap = Colormap(VIRIDIS);

    pub fn from_name(s: &str) -> Option<Colormap> {
        Some(Colormap(match s.to_ascii_lowercase().as_str() {
            "viridis" => VIRIDIS,
            "cividis" => CIVIDIS,
            "magma" => MAGMA,
            "plasma" => PLASMA,
            "inferno" => INFERNO,
            "coolwarm" => COOLWARM,
            "blues" => BLUES,
            "reds" => REDS,
            "gray" | "grey" | "greys" => GRAY,
            _ => return None,
        }))
    }

    /// The colour at `t` from 0 (low) to 1 (high); outside that range the end colours.
    pub fn at(self, t: f64) -> Color {
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let n = self.0.len() - 1;
        let pos = t * n as f64;
        let i = (pos.floor() as usize).min(n - 1);
        let f = pos - i as f64;
        let (a, b) = (rgb(self.0[i]), rgb(self.0[i + 1]));
        let mix = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * f).round() as u8;
        Color(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
    }
}

fn rgb(v: u32) -> Color {
    Color((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Black or white, whichever reads better on `c` (for numbers written on cells).
pub fn text_on(c: Color) -> Color {
    let lum = 0.2126 * f64::from(c.0) + 0.7152 * f64::from(c.1) + 0.0722 * f64::from(c.2);
    if lum > 140.0 {
        Color(0, 0, 0)
    } else {
        Color(255, 255, 255)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ends_and_middle() {
        let v = Colormap::from_name("Viridis").unwrap();
        assert_eq!(v.at(0.0), Color(0x44, 0x01, 0x54));
        assert_eq!(v.at(1.0), Color(0xfd, 0xe7, 0x25));
        assert_eq!(v.at(2.0), v.at(1.0));
        let g = Colormap::from_name("gray").unwrap();
        assert_eq!(g.at(0.5), Color(128, 128, 128));
        assert!(Colormap::from_name("jet").is_none());
        for n in NAMES {
            assert!(Colormap::from_name(n).is_some(), "{n}");
        }
        assert_eq!(text_on(Color(255, 255, 0)), Color(0, 0, 0));
    }
}
