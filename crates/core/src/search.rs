//! Finding data files for the launcher: a list of every CSV/TSV, Excel workbook and JSON
//! file under the data folder (a workbook with several sheets gives one entry per sheet),
//! and fuzzy matching against it (so `tmpcsv` finds `temperature_2024.csv`).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatchConfig, Matcher};

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Path relative to the data folder, e.g. `lab/run_3.csv` (a sheet is shown after it:
    /// `results.xlsx › Run2`).
    pub display: String,
    pub modified: SystemTime,
    /// The sheet of a workbook, when it has several (`sheet=` in the command).
    pub sheet: Option<String>,
}

impl FileEntry {
    /// The file name as typed in a command (relative to the data folder, or with `~/`).
    pub fn file(&self) -> &str {
        self.display
            .split(SHEET_MARK)
            .next()
            .unwrap_or(&self.display)
    }
}

/// Between a workbook's name and a sheet in the list: `results.xlsx › Run2`.
pub const SHEET_MARK: &str = " › ";

/// Folders never worth searching for data.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "Library",
    "target",
    "__pycache__",
    "venv",
    "site-packages",
];
const EXTENSIONS: &[&str] = &["csv", "tsv", "txt", "xlsx", "xlsm", "xls", "ods", "json"];
const WORKBOOKS: &[&str] = &["xlsx", "xlsm", "xls", "ods"];
/// Which JSON files the list shows (`general.json_files`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonFiles {
    /// Every `.json` file.
    #[default]
    All,
    /// Only those that hold a table: the file starts with `[` and then `{` (rows).
    Tables,
    /// All but well-known settings files (`package.json`, `tsconfig.json`…).
    Known,
}

impl JsonFiles {
    pub const NAMES: [&'static str; 3] = ["all", "tables", "known"];

    pub fn from_name(s: &str) -> Option<JsonFiles> {
        Some(match s.to_ascii_lowercase().as_str() {
            "all" => JsonFiles::All,
            "tables" => JsonFiles::Tables,
            "known" => JsonFiles::Known,
            _ => return None,
        })
    }

    /// Whether the JSON file at `path` is listed.
    fn lists(self, path: &Path) -> bool {
        match self {
            JsonFiles::All => true,
            JsonFiles::Tables => json_holds_rows(path),
            JsonFiles::Known => !path.file_name().is_some_and(|n| {
                let n = n.to_string_lossy().to_ascii_lowercase();
                SETTINGS_JSON.contains(&n.as_str())
                    || n.starts_with("tsconfig.")
                    || n.ends_with(".config.json")
            }),
        }
    }
}

/// JSON files that are settings of some tool, never data.
const SETTINGS_JSON: &[&str] = &[
    "package.json",
    "package-lock.json",
    "tsconfig.json",
    "jsconfig.json",
    "settings.json",
    "composer.json",
    "manifest.json",
    "tauri.conf.json",
];

/// Does the file start with `[` and then `{` (an array of rows)? Reads a few bytes only.
fn json_holds_rows(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 256];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let mut chars = String::from_utf8_lossy(&head[..n])
        .trim_start_matches('\u{feff}')
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .into_bytes()
        .into_iter();
    chars.next() == Some(b'[') && chars.next() == Some(b'{')
}

fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .is_some_and(|e| list.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn is_json(path: &Path) -> bool {
    has_extension(path, &["json"])
}

/// A file a folder grid draws: CSV, TSV or a workbook (not JSON or notes).
pub fn is_table_file(path: &Path) -> bool {
    has_extension(path, &["csv", "tsv", "xlsx", "xlsm", "xls", "ods"])
}

const MAX_DEPTH: usize = 8;
/// Stop after looking at this many folder entries, so a huge data folder can't stall us.
const MAX_VISITED: usize = 200_000;

/// List every data file under `root` (hidden folders and the ones above are skipped), and
/// every folder that holds two or more of them directly (shown as `runs/`: picked, it
/// gives one panel per file). `json` says which JSON files count.
pub fn scan(root: &Path, json: JsonFiles) -> Vec<FileEntry> {
    let mut out = Vec::new();
    // Data files directly inside each folder, for the folder entries.
    let mut tables_in: std::collections::HashMap<PathBuf, (usize, SystemTime)> =
        std::collections::HashMap::new();
    let mut visited = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_VISITED {
                return out;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if depth < MAX_DEPTH && !SKIP_DIRS.contains(&name.as_ref()) {
                    stack.push((path, depth + 1));
                }
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
            {
                let modified = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                let display = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                if is_json(&path) && !json.lists(&path) {
                    continue;
                }
                if is_table_file(&path) && dir != root {
                    let e = tables_in
                        .entry(dir.clone())
                        .or_insert((0, SystemTime::UNIX_EPOCH));
                    e.0 += 1;
                    e.1 = e.1.max(modified);
                }
                // .txt only when it looks like a table (a separator in the first line).
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
                    && !looks_like_table(&path)
                {
                    continue;
                }
                // A workbook in the owner's data folder: one entry per sheet if several.
                let sheets = if path
                    .extension()
                    .is_some_and(|e| WORKBOOKS.iter().any(|x| e.eq_ignore_ascii_case(x)))
                {
                    crate::sheets::sheet_names(&path)
                } else {
                    Vec::new()
                };
                if sheets.len() > 1 {
                    for sheet in sheets {
                        out.push(FileEntry {
                            path: path.clone(),
                            display: format!("{display}{SHEET_MARK}{sheet}"),
                            modified,
                            sheet: Some(sheet),
                        });
                    }
                } else {
                    out.push(FileEntry {
                        path,
                        display,
                        modified,
                        sheet: None,
                    });
                }
            }
        }
    }
    for (dir, (count, modified)) in tables_in {
        if count >= 2 {
            let display = dir.strip_prefix(root).unwrap_or(&dir).to_string_lossy();
            out.push(FileEntry {
                display: format!("{display}/"),
                path: dir,
                modified,
                sheet: None,
            });
        }
    }
    out
}

/// Does the first line of a .txt file hold a tab, comma or semicolon (a table, not notes)?
fn looks_like_table(path: &Path) -> bool {
    use std::io::BufRead;
    std::fs::File::open(path)
        .ok()
        .and_then(|f| std::io::BufReader::new(f).lines().next()?.ok())
        .is_some_and(|l| l.contains(['\t', ',', ';']))
}

/// CSV and TSV files and workbooks anywhere, from the Spotlight index (`mdfind`, macOS only; elsewhere
/// nothing). Only names are read, never the files. Paths in hidden folders and the folders
/// skipped by `scan` are left out; `display` is the path with `~` for the home folder.
pub fn spotlight(limit: usize) -> Vec<FileEntry> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let output = std::process::Command::new("/usr/bin/mdfind")
        .arg("kMDItemFSName == '*.csv'c || kMDItemFSName == '*.tsv'c || kMDItemFSName == '*.xlsx'c || kMDItemFSName == '*.xls'c || kMDItemFSName == '*.ods'c")
        .output();
    match output {
        Ok(o) if o.status.success() => from_mdfind(
            &String::from_utf8_lossy(&o.stdout),
            &crate::config::home(),
            limit,
        ),
        _ => Vec::new(),
    }
}

/// Turn `mdfind`'s list of paths into file entries.
pub fn from_mdfind(text: &str, home: &Path, limit: usize) -> Vec<FileEntry> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| {
            p.components().all(|c| {
                let c = c.as_os_str().to_string_lossy();
                !c.starts_with('.') && !SKIP_DIRS.contains(&c.as_ref())
            })
        })
        .take(limit)
        .map(|path| {
            let modified = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let display = match path.strip_prefix(home) {
                Ok(rest) => format!("~/{}", rest.to_string_lossy()),
                Err(_) => path.to_string_lossy().into_owned(),
            };
            FileEntry {
                path,
                display,
                modified,
                sheet: None,
            }
        })
        .collect()
}

/// Add `more` to `entries`, skipping files already listed.
pub fn merge(entries: &mut Vec<FileEntry>, more: Vec<FileEntry>) {
    let known: std::collections::HashSet<PathBuf> =
        entries.iter().map(|e| e.path.clone()).collect();
    // (A workbook found again by Spotlight is already listed, sheets and all.)
    entries.extend(more.into_iter().filter(|e| !known.contains(&e.path)));
}

/// The best matches for `query`, best first. An empty query lists the files of the
/// latest commands first (`recent`: names as typed, newest first), then the most recently
/// changed ones; JSON files come after CSV files and workbooks.
pub fn search<'a>(
    entries: &'a [FileEntry],
    query: &str,
    limit: usize,
    recent: &[String],
) -> Vec<&'a FileEntry> {
    let query = query.trim();
    // Where a file stands among the recently used ones (lower = more recent).
    let used = |e: &FileEntry| {
        recent
            .iter()
            .position(|r| r == e.file() || r == &e.display)
            .unwrap_or(usize::MAX)
    };
    if query.is_empty() {
        let mut list: Vec<&FileEntry> = entries.iter().collect();
        list.sort_by(|a, b| {
            used(a)
                .cmp(&used(b))
                .then(is_json(&a.path).cmp(&is_json(&b.path)))
                .then(b.modified.cmp(&a.modified))
                .then(a.display.cmp(&b.display))
        });
        list.truncate(limit);
        return list;
    }
    let mut matcher = Matcher::new(MatchConfig::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut scored: Vec<(&FileEntry, u32)> = entries
        .iter()
        .filter_map(|e| {
            let mut buf = Vec::new();
            pattern
                .score(
                    nucleo_matcher::Utf32Str::new(&e.display, &mut buf),
                    &mut matcher,
                )
                .map(|s| (e, s))
        })
        .collect();
    // Higher score first; ties go to a recently used file, then to CSV and workbooks
    // before JSON, then to the shorter (usually more specific) path.
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(used(a.0).cmp(&used(b.0)))
            .then(is_json(&a.0.path).cmp(&is_json(&b.0.path)))
            .then(a.0.display.len().cmp(&b.0.display.len()))
    });
    scored.into_iter().take(limit).map(|(e, _)| e).collect()
}

/// A pattern like `run_*.csv` typed without its folders: the pattern in each folder
/// whose files it matches (most matches first), written as it should be typed, e.g.
/// `lab/tests/run_*.csv`. A folder in the pattern (`tests/run_*`) must end the file's
/// folder. Empty for a query without `*` or `?` (owner, round 13: `run_*.csv` listed
/// nothing, so the path had to be found through `run_a.csv` first).
pub fn pattern_folders(entries: &[FileEntry], query: &str, limit: usize) -> Vec<(String, usize)> {
    let query = query.trim().trim_matches('"');
    if !query.contains(['*', '?']) {
        return Vec::new();
    }
    let (want_dir, name) = match query.rsplit_once('/') {
        Some((d, n)) => (d.to_lowercase(), n.to_lowercase()),
        None => (String::new(), query.to_lowercase()),
    };
    let mut found: Vec<(String, usize)> = Vec::new();
    for e in entries.iter().filter(|e| e.sheet.is_none()) {
        let file = e.file();
        let (dir, base) = file.rsplit_once('/').unwrap_or(("", file));
        if !crate::figure::wildcard(&name, &base.to_lowercase())
            || !(want_dir.is_empty() || dir.to_lowercase().ends_with(&want_dir))
        {
            continue;
        }
        let typed = if dir.is_empty() {
            query.rsplit('/').next().unwrap_or(query).to_owned()
        } else {
            format!("{dir}/{}", query.rsplit('/').next().unwrap_or(query))
        };
        match found.iter_mut().find(|(t, _)| *t == typed) {
            Some((_, n)) => *n += 1,
            None => found.push((typed, 1)),
        }
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.len().cmp(&b.0.len())));
    found.truncate(limit);
    found
}

/// For a query with no fuzzy match (a typo): the few files whose names are closest in
/// spelling, closest first.
pub fn closest<'a>(entries: &'a [FileEntry], query: &str, limit: usize) -> Vec<&'a FileEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let allowed = (q.chars().count() / 2).max(2);
    let mut scored: Vec<(usize, &FileEntry)> = entries
        .iter()
        .filter_map(|e| {
            let stem = Path::new(&e.display)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let d = crate::error::edit_distance(&q, &stem)
                .min(crate::error::edit_distance(&q, &e.display.to_lowercase()));
            (d <= allowed).then_some((d, e))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.display.cmp(&b.1.display)));
    scored.into_iter().take(limit).map(|(_, e)| e).collect()
}

/// Fuzzy-filter plain strings (plot types, column names, options), best first.
pub fn filter<'a>(items: &[&'a str], query: &str) -> Vec<&'a str> {
    let query = query.trim();
    if query.is_empty() {
        return items.to_vec();
    }
    let mut matcher = Matcher::new(MatchConfig::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut scored: Vec<(usize, &'a str, u32)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let mut buf = Vec::new();
            pattern
                .score(nucleo_matcher::Utf32Str::new(s, &mut buf), &mut matcher)
                .map(|score| (i, *s, score))
        })
        .collect();
    scored.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(_, s, _)| s).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    #[test]
    fn scans_fixture_folder() {
        let files = scan(&fixtures(), JsonFiles::All);
        assert!(files.iter().any(|f| f.display == "simple.csv"));
        assert!(!files.iter().any(|f| f.display.ends_with(".md")));
    }

    #[test]
    fn fuzzy_finds_files() {
        let files = scan(&fixtures(), JsonFiles::All);
        let hits = search(&files, "smpl", 5, &[]);
        assert_eq!(hits[0].display, "simple.csv");
        let hits = search(&files, "datetime", 5, &[]);
        assert_eq!(hits[0].display, "datetimes.csv");
        assert!(search(&files, "zzzzqqq", 5, &[]).is_empty());
        assert_eq!(search(&files, "", 3, &[]).len(), 3);
    }

    #[test]
    fn skips_hidden_and_heavy_folders() {
        let root = std::env::temp_dir().join(format!("easyplots-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["a/b", ".hidden", "node_modules/x"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "a/b/deep.csv",
            ".hidden/no.csv",
            "node_modules/x/no.csv",
            "top.TSV",
            "notes.txt",
        ] {
            std::fs::write(root.join(file), "x\n1\n").unwrap();
        }
        let mut names: Vec<String> = scan(&root, JsonFiles::All)
            .into_iter()
            .map(|f| f.display)
            .collect();
        names.sort();
        assert_eq!(names, ["a/b/deep.csv", "top.TSV"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn closest_catches_typos() {
        let files = scan(&fixtures(), JsonFiles::All);
        assert!(
            search(&files, "smiple", 5, &[]).is_empty()
                || search(&files, "smiple", 5, &[])[0].display != "simple.csv"
        );
        assert_eq!(closest(&files, "smiple", 3)[0].display, "simple.csv");
        assert!(closest(&files, "qqqqqqqq", 3).is_empty());
    }

    #[test]
    fn spotlight_list_is_filtered() {
        let text = "/Users/me/data/a.csv\n/Users/me/Library/x.csv\n/Users/me/.hidden/b.csv\n/Volumes/USB/c.tsv\n";
        let got = from_mdfind(text, Path::new("/Users/me"), 10);
        let names: Vec<&str> = got.iter().map(|e| e.display.as_str()).collect();
        assert_eq!(names, ["~/data/a.csv", "/Volumes/USB/c.tsv"]);
        let mut all = got.clone();
        merge(&mut all, got);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn filter_strings() {
        assert_eq!(filter(&["line", "scatter", "bar"], "sct"), ["scatter"]);
        assert_eq!(filter(&["time", "temp"], ""), ["time", "temp"]);
    }

    #[test]
    fn recent_files_json_last_and_folders() {
        let files = scan(&fixtures(), JsonFiles::All);
        // A folder holding two or more data files is listed, with a slash.
        assert!(files.iter().any(|f| f.display == "runs/"));
        // The files of the latest commands lead an empty search, in that order.
        let recent = vec!["garden.csv".to_owned(), "heating.csv".to_owned()];
        let hits = search(&files, "", 5, &recent);
        assert_eq!(hits[0].display, "garden.csv");
        assert_eq!(hits[1].display, "heating.csv");
        // JSON comes after everything else.
        let all = search(&files, "", 1000, &[]);
        let first_json = all.iter().position(|f| is_json(&f.path)).unwrap();
        assert!(all[first_json..].iter().all(|f| is_json(&f.path)));
    }

    #[test]
    fn json_modes() {
        let root = std::env::temp_dir().join(format!("easyplots-json-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("rows.json"), " [\n {\"a\": 1}]").unwrap();
        std::fs::write(root.join("package.json"), "{\"name\": \"x\"}").unwrap();
        std::fs::write(root.join("notes.json"), "{\"a\": [1, 2]}").unwrap();
        let names = |mode| {
            let mut n: Vec<String> = scan(&root, mode).into_iter().map(|f| f.display).collect();
            n.sort();
            n
        };
        assert_eq!(names(JsonFiles::All).len(), 3);
        assert_eq!(names(JsonFiles::Tables), ["rows.json"]);
        assert_eq!(names(JsonFiles::Known), ["notes.json", "rows.json"]);
        // The test's own folder in the system's temp place.
        let _ = std::fs::remove_dir_all(&root);
    }
}
