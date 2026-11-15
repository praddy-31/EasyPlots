//! Sample data built into the app for the "Try it" guide (round 8c).
//!
//! The files live in `tests/fixtures/` (made-up data, checked by the owner) and are
//! embedded in the binary at build time, so the guide works the same in the app and in
//! Terminal and nothing is ever written to disk. A command opens one by its reserved
//! name: `line demo/peak_onset.csv x=T y=storage` (the `.csv` may be left out, as for real files).

use std::path::Path;

/// Every sample file, by its reserved name.
pub const FILES: &[(&str, &str)] = &[
    (
        "demo/categorical.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/garden.csv"
        )),
    ),
    (
        "demo/trial.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/trial.csv"
        )),
    ),
    (
        "demo/spectrum.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/spectrum.csv"
        )),
    ),
    (
        "demo/peak_onset.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/heating.csv"
        )),
    ),
    (
        "demo/run_a.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/run_a.csv"
        )),
    ),
    (
        "demo/run_b.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/run_b.csv"
        )),
    ),
    (
        "demo/surface.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/surface.csv"
        )),
    ),
    (
        "demo/outlier.csv",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/outlier.csv"
        )),
    ),
];

/// The reserved name a typed one means (`demo/peak_onset` → `demo/peak_onset.csv`), or None when it
/// isn't a sample file. Case doesn't matter.
pub fn resolve(typed: &str) -> Option<&'static str> {
    let t = typed.to_ascii_lowercase();
    FILES
        .iter()
        .map(|(name, _)| *name)
        .find(|name| *name == t || name.trim_end_matches(".csv") == t)
}

/// The embedded text of a sample file, by the (resolved) path.
pub fn content(path: &Path) -> Option<&'static str> {
    let p = path.to_str()?;
    FILES.iter().find(|(name, _)| *name == p).map(|(_, c)| *c)
}
