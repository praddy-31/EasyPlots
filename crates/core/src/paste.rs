//! Tables pasted from the clipboard (cells copied in Excel, Numbers or a browser come as
//! tab-separated text). A pasted table is saved as a file in `paste_dir` before it is
//! plotted, so the saved plot's command names a real file and `again` can redraw it.

use std::path::{Path, PathBuf};

use crate::Error;
use crate::data::{self, Table};

/// What the clipboard holds, if it looks like a table: at least two rows, and a tab,
/// comma or semicolon in the first line.
pub fn table(text: &str) -> Option<Table> {
    let rows = text.lines().filter(|l| !l.trim().is_empty()).count();
    let first = text.lines().find(|l| !l.trim().is_empty())?;
    if rows < 2 || !first.contains(['\t', ',', ';']) {
        return None;
    }
    data::parse(text, Path::new("clipboard"))
        .ok()
        .filter(|t| !t.columns.is_empty())
}

/// [`save`] with the current local time in the name.
pub fn save_now(text: &str, dir: &Path) -> Result<PathBuf, Error> {
    save(text, dir, chrono::Local::now().naive_local())
}

/// Save pasted text as `pasted-YYYY-MM-DD-HHMMSS.tsv` (or `.csv` when it isn't
/// tab-separated) in `dir`, never replacing a file (`-2`, `-3`… instead).
pub fn save(text: &str, dir: &Path, now: chrono::NaiveDateTime) -> Result<PathBuf, Error> {
    if table(text).is_none() {
        return Err(Error::new(
            "the clipboard holds no table — copy some cells (with their header row) first",
        ));
    }
    let ext = if text.lines().next().is_some_and(|l| l.contains('\t')) {
        "tsv"
    } else {
        "csv"
    };
    std::fs::create_dir_all(dir)
        .map_err(|e| Error::new(format!("cannot create {}: {e}", dir.display())))?;
    let stem = format!("pasted-{}", now.format("%Y-%m-%d-%H%M%S"));
    let mut path = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    let mut body = text.replace("\r\n", "\n");
    if !body.ends_with('\n') {
        body.push('\n');
    }
    std::fs::write(&path, body)
        .map_err(|e| Error::new(format!("cannot write {}: {e}", path.display())))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_and_saves_tables() {
        assert!(table("hello world").is_none());
        assert!(table("t\ttemp").is_none()); // a header alone
        let t = table("t\ttemp\n1\t20.5\n2\t21\n").unwrap();
        assert_eq!(t.column_names().collect::<Vec<_>>(), ["t", "temp"]);
        let dir = std::env::temp_dir().join(format!("easyplots-paste-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 23)
            .unwrap()
            .and_hms_opt(14, 5, 0)
            .unwrap();
        let a = save("t\ttemp\r\n1\t20\r\n", &dir, now).unwrap();
        let b = save("t\ttemp\n1\t20\n", &dir, now).unwrap();
        assert_eq!(a.file_name().unwrap(), "pasted-2026-10-23-140500.tsv");
        assert_eq!(b.file_name().unwrap(), "pasted-2026-10-23-140500-2.tsv");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "t\ttemp\n1\t20\n");
        assert!(save("just words", &dir, now).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
