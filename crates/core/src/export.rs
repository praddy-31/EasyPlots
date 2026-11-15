//! Turning the SVG into the requested file format.
//!
//! - PNG: `resvg` draws the SVG into pixels at the chosen DPI. The PNG records its DPI and
//!   the command that made it.
//! - PDF: `svg2pdf` converts the SVG with fonts embedded as real (searchable) text. The
//!   command goes into the PDF's document info.
//! - SVG: written as is (the command is already in its `<desc>`).

use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use resvg::usvg::{self, fontdb};

use crate::Error;
use crate::render::text::{self, Font};
use crate::spec::Format;

/// The font database with only the bundled fonts, so output never depends on what's
/// installed on the machine.
fn fonts() -> Arc<fontdb::Database> {
    static DB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        for data in text::all_font_data() {
            db.load_font_data(data.to_vec());
        }
        db.set_serif_family(Font::Stix.family());
        db.set_sans_serif_family(Font::SourceSans.family());
        Arc::new(db)
    })
    .clone()
}

fn parse_svg(svg: &str) -> Result<usvg::Tree, Error> {
    let options = usvg::Options {
        font_family: Font::Stix.family().to_owned(),
        fontdb: fonts(),
        ..usvg::Options::default()
    };
    usvg::Tree::from_str(svg, &options).map_err(|e| Error::new(format!("drawing failed: {e}")))
}

/// A small PNG of an SVG, `width` pixels wide (for thumbnails; no stored command).
pub fn svg_thumbnail(svg: &str, width: u32) -> Result<Vec<u8>, Error> {
    let tree = parse_svg(svg)?;
    let size = tree.size();
    let scale = width as f32 / size.width().max(1.0);
    let h = ((size.height() * scale).ceil() as u32).max(1);
    let mut pixmap =
        tiny_skia::Pixmap::new(width.max(1), h).ok_or_else(|| Error::new("thumbnail too large"))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| Error::new(format!("thumbnail: {e}")))
}

/// A PNG scaled down to `width` pixels wide (for thumbnails).
pub fn png_thumbnail(png: &[u8], width: u32) -> Result<Vec<u8>, Error> {
    let src =
        tiny_skia::Pixmap::decode_png(png).map_err(|e| Error::new(format!("thumbnail: {e}")))?;
    let scale = width as f32 / src.width().max(1) as f32;
    let h = ((src.height() as f32 * scale).ceil() as u32).max(1);
    let mut pixmap =
        tiny_skia::Pixmap::new(width.max(1), h).ok_or_else(|| Error::new("thumbnail too large"))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    pixmap.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bicubic,
            ..Default::default()
        },
        tiny_skia::Transform::from_scale(scale, scale),
        None,
    );
    pixmap
        .encode_png()
        .map_err(|e| Error::new(format!("thumbnail: {e}")))
}

/// Convert an SVG (sized in points) to the file format's bytes.
pub fn encode(
    svg: &str,
    format: Format,
    dpi: f64,
    command: &str,
    title: &str,
) -> Result<Vec<u8>, Error> {
    match format {
        Format::Svg => Ok(svg.as_bytes().to_vec()),
        Format::Png => png(&parse_svg(svg)?, dpi, command),
        Format::Pdf => pdf(&parse_svg(svg)?, command, title),
    }
}

fn png(tree: &usvg::Tree, dpi: f64, command: &str) -> Result<Vec<u8>, Error> {
    // usvg measures in CSS pixels (96 per inch).
    let scale = (dpi / 96.0) as f32;
    let size = tree.size();
    let w = (size.width() * scale).round().max(1.0) as u32;
    let h = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| Error::new(format!("image too large ({w}×{h} pixels)")))?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }

    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let per_metre = (dpi / 0.0254).round() as u32;
        enc.set_pixel_dims(Some(png::PixelDimensions {
            xppu: per_metre,
            yppu: per_metre,
            unit: png::Unit::Meter,
        }));
        let fail = |e: png::EncodingError| Error::new(format!("PNG encoding failed: {e}"));
        enc.add_itxt_chunk("Software".into(), software())
            .map_err(fail)?;
        enc.add_itxt_chunk("EasyPlots command".into(), command.to_owned())
            .map_err(fail)?;
        let mut writer = enc.write_header().map_err(fail)?;
        writer.write_image_data(&rgba).map_err(fail)?;
    }
    Ok(bytes)
}

fn pdf(tree: &usvg::Tree, command: &str, title: &str) -> Result<Vec<u8>, Error> {
    use pdf_writer::{Content, Name, Pdf, Rect, Ref, TextStr};
    use std::collections::HashMap;

    let (chunk, svg_ref) = svg2pdf::to_chunk(tree, svg2pdf::ConversionOptions::default())
        .map_err(|e| Error::new(format!("PDF conversion failed: {e}")))?;

    let mut alloc = Ref::new(1);
    let catalog = alloc.bump();
    let pages = alloc.bump();
    let page = alloc.bump();
    let content = alloc.bump();
    let info = alloc.bump();
    let mut map = HashMap::new();
    let chunk = chunk.renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));
    let svg_ref = map[&svg_ref];

    // Page size in points: usvg's pixels are 1/96 inch.
    let w = tree.size().width() * 0.75;
    let h = tree.size().height() * 0.75;
    let name = Name(b"S1");

    let mut out = Pdf::new();
    out.catalog(catalog).pages(pages);
    out.pages(pages).kids([page]).count(1);
    {
        let mut p = out.page(page);
        p.media_box(Rect::new(0.0, 0.0, w, h));
        p.parent(pages);
        p.contents(content);
        p.resources().x_objects().pair(name, svg_ref);
    }
    let mut c = Content::new();
    c.transform([w, 0.0, 0.0, h, 0.0, 0.0]).x_object(name);
    out.stream(content, &c.finish());
    out.extend(&chunk);
    out.document_info(info)
        .title(TextStr(title))
        .subject(TextStr(command))
        .creator(TextStr(&software()))
        .producer(TextStr(&software()));
    Ok(out.finish())
}

fn software() -> String {
    format!("EasyPlots {}", crate::VERSION)
}

/// The numbers actually drawn — after `where=`, calculations, `stat=`, baselines and
/// fits — as tab-separated text for the clipboard (⌘⇧C in the preview). Long format,
/// one row per point: panel, series, x, y; an `err` column when any series has error
/// bars (`err_lo`/`err_hi` when a bar is longer one way); a `fit` column with the fitted
/// value at each point's x when a series has a fit; a `z` column when a heatmap or
/// contour is drawn. Names are plain text (no maths markup), numbers have at most 6
/// significant figures, a date x is written as a date and a category x as its name; a
/// box or violin gives one row per group with y the median and err_lo/err_hi the
/// quartiles. Fitted curves themselves aren't listed (owner, 2026-11-15: 200 rows named
/// by the whole equation).
pub fn table_text(fig: &crate::figure::Figure) -> String {
    use crate::figure::AxisKind;
    use crate::render::mathtext::plain;
    use std::fmt::Write;

    struct Row {
        panel: usize,
        series: String,
        x: String,
        y: String,
        lo: String,
        hi: String,
        fit: String,
        z: String,
    }
    let num = |v: f64| {
        if v.is_finite() {
            sig6(v)
        } else {
            String::new()
        }
    };
    let mut rows: Vec<Row> = Vec::new();
    for (pi, panel) in fig.panels.iter().enumerate() {
        let x_text = |x: f64| match panel.xaxis {
            AxisKind::Category => panel
                .categories
                .get(x.round() as usize)
                .cloned()
                .unwrap_or_else(|| num(x)),
            AxisKind::Date => chrono::DateTime::from_timestamp(x.floor() as i64, 0)
                .map(|d| {
                    let d = d.naive_utc();
                    if d.format("%H:%M:%S").to_string() == "00:00:00" {
                        d.format("%Y-%m-%d").to_string()
                    } else {
                        d.format("%Y-%m-%d %H:%M:%S").to_string()
                    }
                })
                .unwrap_or_else(|| num(x)),
            AxisKind::Number => num(x),
        };
        let all: Vec<&crate::figure::Series> =
            panel.series.iter().chain(&panel.residuals).collect();
        for (k, s) in all.iter().enumerate() {
            if s.is_fit {
                continue;
            }
            let row = |x: String, y: String| Row {
                panel: pi,
                series: plain(&s.label),
                x,
                y,
                lo: String::new(),
                hi: String::new(),
                fit: String::new(),
                z: String::new(),
            };
            if let Some(grid) = &s.grid {
                for (j, y) in grid.ys.iter().enumerate() {
                    for (i, x) in grid.xs.iter().enumerate() {
                        rows.push(Row {
                            z: num(grid.at(i, j)),
                            ..row(num(*x), num(*y))
                        });
                    }
                }
                continue;
            }
            if !s.boxes.is_empty() {
                for b in &s.boxes {
                    rows.push(Row {
                        lo: num(b.q1),
                        hi: num(b.q3),
                        ..row(x_text(b.pos), num(b.median))
                    });
                }
                continue;
            }
            // Its fit is the curve drawn right after it.
            let fitted = all
                .get(k + 1)
                .filter(|f| f.is_fit)
                .and_then(|f| f.fitted.as_deref());
            for (i, (&x, &y)) in s.x.iter().zip(&s.y).enumerate() {
                let up = s.yerr.as_ref().and_then(|e| e.get(i)).copied();
                let down = s.yerr_lo.as_ref().and_then(|e| e.get(i)).copied();
                let (lo, hi) = match (up, down) {
                    (Some(u), Some(d)) => (num(d), num(u)),
                    (Some(u), None) => (num(u), num(u)),
                    _ => (String::new(), String::new()),
                };
                rows.push(Row {
                    lo,
                    hi,
                    fit: fitted.map_or(String::new(), |f| num(f.eval(x))),
                    ..row(x_text(x), num(y))
                });
            }
        }
    }
    let panels = fig.panels.len() > 1;
    let err = rows.iter().any(|r| !r.lo.is_empty());
    let two_sided = rows.iter().any(|r| r.lo != r.hi);
    let fit = rows.iter().any(|r| !r.fit.is_empty());
    let z = rows.iter().any(|r| !r.z.is_empty());
    let mut out = String::new();
    let mut head: Vec<&str> = Vec::new();
    if panels {
        head.push("panel");
    }
    head.extend(["series", "x", "y"]);
    if err && two_sided {
        head.extend(["err_lo", "err_hi"]);
    } else if err {
        head.push("err");
    }
    if fit {
        head.push("fit");
    }
    if z {
        head.push("z");
    }
    let _ = writeln!(out, "{}", head.join("\t"));
    for r in rows {
        let mut cells: Vec<String> = Vec::new();
        if panels {
            cells.push((r.panel + 1).to_string());
        }
        cells.extend([r.series, r.x, r.y]);
        if err && two_sided {
            cells.extend([r.lo, r.hi]);
        } else if err {
            cells.push(r.hi);
        }
        if fit {
            cells.push(r.fit);
        }
        if z {
            cells.push(r.z);
        }
        let _ = writeln!(out, "{}", cells.join("\t"));
    }
    out
}

/// At most 6 significant figures, without trailing zeros or float noise
/// (1.6400000000000001 → 1.64, 73.99099999999999 → 73.991).
fn sig6(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let exp = v.abs().log10().floor() as i32;
    if !(-6..15).contains(&exp) {
        return format!("{v:.5e}");
    }
    let decimals = (5 - exp).max(0) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="72pt" height="36pt" viewBox="0 0 72 36" font-family="STIX Two Text"><rect width="72" height="36" fill="#fff"/><text x="4" y="20" font-size="10">Temp −1</text></svg>"##;

    #[test]
    fn png_has_requested_size_and_metadata() {
        let bytes = encode(SVG, Format::Png, 144.0, "line a.csv", "a").unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let reader = decoder.read_info().unwrap();
        let info = reader.info();
        // 1 inch × 0.5 inch at 144 dpi.
        assert_eq!((info.width, info.height), (144, 72));
        assert!(
            info.utf8_text
                .iter()
                .any(|t| t.get_text().unwrap() == "line a.csv")
        );
    }

    #[test]
    fn pdf_embeds_font_and_command() {
        let bytes = encode(SVG, Format::Pdf, 300.0, "line a.csv", "a").unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-"));
        assert!(text.contains("STIXTwoText"), "font should be embedded");
        assert!(text.contains("line a.csv"));
        assert!(
            text.contains("/MediaBox [0 0 72 36]"),
            "page should be 1×0.5 inch"
        );
    }

    #[test]
    fn text_is_drawn_with_bundled_font() {
        fn count_text(g: &usvg::Group) -> usize {
            g.children()
                .iter()
                .map(|n| match n {
                    usvg::Node::Text(_) => 1,
                    usvg::Node::Group(g) => count_text(g),
                    _ => 0,
                })
                .sum()
        }
        // If the font were missing, usvg would drop the text.
        let tree = parse_svg(SVG).unwrap();
        assert_eq!(count_text(tree.root()), 1);
    }
}
