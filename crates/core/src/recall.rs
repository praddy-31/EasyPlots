//! `again PLOT`: read back the command a saved plot was made from, so it can be changed
//! and drawn again. Every plot stores it (see `export`): PNGs in a text chunk
//! (`EasyPlots command`), PDFs as the document's Subject, SVGs in `<desc>`.

use std::path::Path;

use crate::Error;

/// The command stored in the plot at `path`.
pub fn stored_command(path: &Path) -> Result<String, Error> {
    let bytes = std::fs::read(path)
        .map_err(|e| Error::new(format!("cannot read {}: {e}", path.display())))?;
    let found = match path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => from_png(&bytes),
        Some("pdf") => from_pdf(&bytes),
        Some("svg") => from_svg(&String::from_utf8_lossy(&bytes)),
        _ => {
            return Err(Error::new(format!(
                "{} is not a plot (again reads .png, .pdf and .svg files)",
                path.display()
            )));
        }
    };
    found.filter(|c| !c.trim().is_empty()).ok_or_else(|| {
        Error::new(format!(
            "{} has no EasyPlots command stored (was it made by EasyPlots?)",
            path.display()
        ))
    })
}

/// A saved plot for the browser: where it is, when it was saved, the command it stores.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SavedPlot {
    /// Relative to the plots folder, e.g. `simple_line_2026-10-23-140500.png`.
    pub name: String,
    pub path: std::path::PathBuf,
    /// Seconds since 1970, for "saved 5 min ago" and sorting.
    pub modified: u64,
    pub command: String,
}

/// The newest `limit` plots in `dir` that EasyPlots made (they store their command),
/// leaving out anything under `skip` (the pasted-data folder) when that is a folder of its
/// own inside `dir`. If `skip` is the plots folder itself (or holds it), nothing is left out.
pub fn recent(dir: &Path, skip: &Path, limit: usize) -> Vec<SavedPlot> {
    let skip_inside = !dir.starts_with(skip);
    saved_plots(dir)
        .into_iter()
        .map(|name| (dir.join(&name), name))
        .filter(|(path, _)| !(skip_inside && path.starts_with(skip)))
        .filter_map(|(path, name)| {
            let command = stored_command(&path).ok()?;
            let modified = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            Some(SavedPlot {
                name,
                path,
                modified,
                command,
            })
        })
        .take(limit)
        .collect()
}

/// A small PNG of a saved PNG or SVG plot (PDFs need their command drawn again).
pub fn thumbnail(path: &Path, width: u32) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    match path
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => crate::export::png_thumbnail(&bytes, width).ok(),
        "svg" => crate::export::svg_thumbnail(&String::from_utf8_lossy(&bytes), width).ok(),
        _ => None,
    }
}

/// Plots in `dir` (and one level of subfolders), newest first, as paths relative to it.
pub fn saved_plots(dir: &Path) -> Vec<String> {
    let mut found: Vec<(std::time::SystemTime, String)> = Vec::new();
    let visit = |folder: &Path, prefix: &str, deeper: bool, found: &mut Vec<_>| {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return Vec::new();
        };
        let mut subdirs = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = e.path();
            if path.is_dir() {
                if deeper {
                    subdirs.push((path, format!("{prefix}{name}/")));
                }
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if [".png", ".pdf", ".svg"].iter().any(|x| lower.ends_with(x)) {
                let modified = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                found.push((modified, format!("{prefix}{name}")));
            }
        }
        subdirs
    };
    for (sub, prefix) in visit(dir, "", true, &mut found) {
        visit(&sub, &prefix, false, &mut found);
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    found.into_iter().map(|(_, n)| n).collect()
}

fn from_png(bytes: &[u8]) -> Option<String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().ok()?;
    let info = reader.info();
    info.utf8_text
        .iter()
        .find(|t| t.keyword == "EasyPlots command")
        .and_then(|t| t.get_text().ok())
        .or_else(|| {
            info.uncompressed_latin1_text
                .iter()
                .find(|t| t.keyword == "EasyPlots command")
                .map(|t| t.text.clone())
        })
}

fn from_svg(text: &str) -> Option<String> {
    let start = text.find("<desc>")? + "<desc>".len();
    let end = start + text[start..].find("</desc>")?;
    Some(
        text[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&"),
    )
}

/// The `/Subject` string of the PDF's document info: a literal `(…)` string with escapes, or
/// a hex `<…>` string (UTF-16 with a byte-order mark for non-ASCII text).
fn from_pdf(bytes: &[u8]) -> Option<String> {
    let key = b"/Subject";
    let at = bytes.windows(key.len()).rposition(|w| w == key)? + key.len();
    let mut i = at;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    match bytes.get(i)? {
        b'(' => {
            let mut out = Vec::new();
            let mut depth = 1;
            i += 1;
            while i < bytes.len() {
                let b = bytes[i];
                match b {
                    b'\\' => {
                        i += 1;
                        let e = *bytes.get(i)?;
                        match e {
                            b'n' => out.push(b'\n'),
                            b'r' => out.push(b'\r'),
                            b't' => out.push(b'\t'),
                            b'b' => out.push(8),
                            b'f' => out.push(12),
                            b'0'..=b'7' => {
                                let mut v = 0u32;
                                let mut k = 0;
                                while k < 3
                                    && bytes.get(i).is_some_and(|d| (b'0'..=b'7').contains(d))
                                {
                                    v = v * 8 + u32::from(bytes[i] - b'0');
                                    i += 1;
                                    k += 1;
                                }
                                out.push(v as u8);
                                continue;
                            }
                            b'\r' | b'\n' => {}
                            other => out.push(other),
                        }
                    }
                    b'(' => {
                        depth += 1;
                        out.push(b);
                    }
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        out.push(b);
                    }
                    _ => out.push(b),
                }
                i += 1;
            }
            Some(decode(&out))
        }
        b'<' => {
            let end = i + bytes[i..].iter().position(|b| *b == b'>')?;
            let hex: Vec<u8> = bytes[i + 1..end]
                .iter()
                .copied()
                .filter(|b| b.is_ascii_hexdigit())
                .collect();
            let raw: Vec<u8> = hex
                .chunks(2)
                .map(|p| {
                    let s = std::str::from_utf8(p).unwrap_or("0");
                    u8::from_str_radix(&format!("{s:0<2}"), 16).unwrap_or(0)
                })
                .collect();
            Some(decode(&raw))
        }
        _ => None,
    }
}

/// PDF text: UTF-16BE after a byte-order mark, otherwise one byte per character.
fn decode(raw: &[u8]) -> String {
    if let Some(rest) = raw.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = rest
            .chunks(2)
            .map(|p| u16::from_be_bytes([p[0], *p.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        raw.iter().map(|b| char::from(*b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export;
    use crate::spec::Format;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="72pt" height="72pt" viewBox="0 0 72 72"><rect width="72" height="72" fill="white"/></svg>"#;

    #[test]
    fn reads_back_every_format() {
        let dir = std::env::temp_dir().join(format!("easyplots-recall-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for command in [
            r#"line a.csv x=t y=temp title="Run (3) \ ok""#,
            r#"line a.csv ylabel="$T_{\mathrm{in}}$ (°C)" title="α""#,
        ] {
            for format in [Format::Png, Format::Pdf] {
                let bytes = export::encode(SVG, format, 72.0, command, "t").unwrap();
                let path = dir.join(format!("p.{}", format.extension()));
                std::fs::write(&path, bytes).unwrap();
                assert_eq!(stored_command(&path).unwrap(), command, "{format:?}");
            }
            let svg = format!("<svg><desc>{}</desc></svg>", crate::render::escape(command));
            let path = dir.join("p.svg");
            std::fs::write(&path, svg).unwrap();
            assert_eq!(stored_command(&path).unwrap(), command);
        }
        std::fs::write(dir.join("x.csv"), "a\n1\n").unwrap();
        assert!(stored_command(&dir.join("x.csv")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
