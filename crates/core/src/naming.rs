//! Output file names: the `{file}_{type}_{date}-{time}` template, `--out=NAME`, safe
//! characters, and never overwriting an existing plot.

use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};

use chrono::NaiveDateTime;

use crate::Error;
use crate::spec::{Format, Spanned};

/// What the template tokens are filled with.
pub struct NameParts {
    pub files: Vec<String>,
    pub types: Vec<String>,
    pub ys: Vec<String>,
    pub now: NaiveDateTime,
}

/// Fill in `{file}`, `{files}`, `{type}`, `{y}`, `{date}`, `{time}` and `{n}`.
pub fn expand(template: &str, parts: &NameParts, n: usize) -> String {
    let mut types = parts.types.clone();
    types.dedup();
    template
        .replace("{file}", parts.files.first().map_or("plot", String::as_str))
        .replace("{files}", &parts.files.join("+"))
        .replace("{type}", &types.join("+"))
        .replace("{y}", &parts.ys.join("-"))
        .replace("{date}", &parts.now.format("%Y-%m-%d").to_string())
        .replace("{time}", &parts.now.format("%H%M%S").to_string())
        .replace("{n}", &n.to_string())
}

/// Replace characters that are unsafe or awkward in file names.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    let short: String = trimmed.chars().take(150).collect();
    if short.is_empty() {
        "plot".to_owned()
    } else {
        short
    }
}

/// Decide the output path (inside `dir`) and format.
///
/// Format: the `--out` extension wins, then `--format`, then the config.
pub fn plan(
    dir: &Path,
    out: Option<&Spanned<String>>,
    format_opt: Option<Format>,
    config_format: Format,
    template: &str,
    parts: &NameParts,
) -> Result<(PathBuf, Format), Error> {
    if let Some(out) = out {
        let rel = Path::new(&out.value);
        let outside = rel.is_absolute()
            || out.value.starts_with('~')
            || rel.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            });
        if outside {
            return Err(Error::at(
                format!(
                    "--out takes a name inside the output folder ({}), not a path outside it — change the folder in config.toml",
                    dir.display()
                ),
                out.span.clone(),
            ));
        }
        let ext_format = rel
            .extension()
            .and_then(|e| Format::from_name(&e.to_string_lossy()));
        if let (Some(e), Some(f)) = (ext_format, format_opt)
            && e != f
        {
            return Err(Error::at(
                format!(
                    "--out ends in .{} but --format is {}",
                    e.extension(),
                    f.extension()
                ),
                out.span.clone(),
            ));
        }
        let format = ext_format.or(format_opt).unwrap_or(config_format);
        let mut path = dir.join(rel);
        if ext_format.is_none() {
            let file = path
                .file_name()
                .map(|f| sanitize(&f.to_string_lossy()))
                .unwrap_or_else(|| "plot".into());
            path.set_file_name(format!("{file}.{}", format.extension()));
        }
        return Ok((path, format));
    }
    let format = format_opt.unwrap_or(config_format);
    let n = std::fs::read_dir(dir).map_or(0, |d| d.count()) + 1;
    let name = sanitize(&expand(template, parts, n));
    Ok((dir.join(format!("{name}.{}", format.extension())), format))
}

/// Create `path`, or `name-2.ext`, `name-3.ext`, … if it already exists. Never overwrites.
pub fn create_unique(path: &Path) -> Result<(File, PathBuf), Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::new(format!("cannot create {}: {e}", parent.display())))?;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "plot".into());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for i in 1..10_000 {
        let candidate = if i == 1 {
            path.to_path_buf()
        } else {
            path.with_file_name(format!("{stem}-{i}{ext}"))
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(f) => return Ok((f, candidate)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(Error::new(format!(
                    "cannot write {}: {e}",
                    candidate.display()
                )));
            }
        }
    }
    Err(Error::new(format!("too many files named {stem}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts() -> NameParts {
        NameParts {
            files: vec!["data".into(), "other".into()],
            types: vec!["line".into(), "line".into()],
            ys: vec!["temp".into(), "pressure".into()],
            now: chrono::NaiveDate::from_ymd_opt(2026, 10, 21)
                .unwrap()
                .and_hms_opt(14, 30, 5)
                .unwrap(),
        }
    }

    #[test]
    fn default_template() {
        assert_eq!(
            expand("{file}_{type}_{date}-{time}", &parts(), 1),
            "data_line_2026-10-21-143005"
        );
        assert_eq!(
            expand("{files}_{y}_{n}", &parts(), 7),
            "data+other_temp-pressure_7"
        );
    }

    #[test]
    fn unsafe_characters() {
        assert_eq!(sanitize("a/b:c?"), "a_b_c_");
        assert_eq!(sanitize("  "), "plot");
    }

    #[test]
    fn out_option() {
        let dir = Path::new("/plots");
        let out = |s: &str| Spanned {
            value: s.to_owned(),
            span: 0..1,
        };
        let (p, f) = plan(dir, Some(&out("run3.pdf")), None, Format::Png, "", &parts()).unwrap();
        assert_eq!((p, f), (PathBuf::from("/plots/run3.pdf"), Format::Pdf));
        let (p, f) = plan(
            dir,
            Some(&out("sub/run3")),
            Some(Format::Svg),
            Format::Png,
            "",
            &parts(),
        )
        .unwrap();
        assert_eq!((p, f), (PathBuf::from("/plots/sub/run3.svg"), Format::Svg));
        assert!(plan(dir, Some(&out("../x.png")), None, Format::Png, "", &parts()).is_err());
        assert!(
            plan(
                dir,
                Some(&out("/tmp/x.png")),
                None,
                Format::Png,
                "",
                &parts()
            )
            .is_err()
        );
        assert!(
            plan(
                dir,
                Some(&out("x.png")),
                Some(Format::Pdf),
                Format::Png,
                "",
                &parts()
            )
            .is_err()
        );
    }

    #[test]
    fn never_overwrites() {
        let dir = std::env::temp_dir().join(format!("easyplots-naming-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let target = dir.join("plot.png");
        let (_, a) = create_unique(&target).unwrap();
        let (_, b) = create_unique(&target).unwrap();
        let (_, c) = create_unique(&target).unwrap();
        assert_eq!(a, target);
        assert_eq!(b, dir.join("plot-2.png"));
        assert_eq!(c, dir.join("plot-3.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
