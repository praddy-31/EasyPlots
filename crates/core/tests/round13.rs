//! Round 13: the owner's reports from testing round 12 (2026-11-15), each kept as a test
//! so it can't come back. Uses the fixtures in tests/fixtures.
use easyplots_core::search::{FileEntry, pattern_folders};
use easyplots_core::{Config, Engine};
use std::time::SystemTime;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");

fn engine() -> Engine {
    let mut config = Config::default();
    config.general.data_dir = FIXTURES.into();
    Engine::new(config)
}

fn refused(command: &str) -> String {
    match engine().render(command, None) {
        Ok(_) => panic!("{command} was drawn"),
        Err(e) => e.message,
    }
}

#[test]
fn a_folder_with_stat_averages_its_files_in_one_panel() {
    let r = engine()
        .render("line runs/ x=t y=temp stat=mean err=iqr", None)
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert!(
        r.svg.contains("mean of day1, day2, day3"),
        "one panel, three files"
    );
    // Without stat= a folder is still a grid, one panel per file.
    let grid = engine().render("line runs/ x=t y=temp", None).unwrap();
    assert!(grid.svg.contains(">day1<") && grid.svg.contains(">day3<"));
}

#[test]
fn stat_names_files_without_their_folders_and_keeps_numbers_in_order() {
    // run_a steps t by 1, simple.csv steps time by 0.5; one typed with its full path.
    let command = format!(
        "line run_a.csv x=t y=temp stat=mean + \"{FIXTURES}/simple.csv\" x=time y=temp stat=mean"
    );
    let svg = engine().render(&command, None).unwrap().svg;
    // (The <desc> keeps the command as typed; only the drawn text counts here.)
    let svg = svg.split("</desc>").nth(1).unwrap_or(&svg).to_owned();
    assert!(svg.contains("mean of run_a, simple"), "file names only");
    assert!(!svg.contains("fixtures/simple"), "no folders in the label");
    // A number axis: plain ticks in order, not 0…20 followed by 0.5, 5.5…
    let ticks: Vec<&str> = svg
        .split("</text>")
        .filter_map(|t| t.rsplit('>').next())
        .filter(|t| t.parse::<f64>().is_ok())
        .collect();
    assert!(
        !ticks.contains(&"0.5") && !ticks.contains(&"5.5"),
        "{ticks:?}"
    );
}

#[test]
fn error_bars_only_where_they_are_drawn() {
    for kind in ["hist", "kde", "area"] {
        let m = refused(&format!("{kind} garden.csv x=mass err=yield_sd"));
        assert!(m.contains(&format!("err= isn't used by {kind}")), "{m}");
    }
    assert!(
        refused("step garden.csv x=width y=yield xerr=mass").contains("xerr= isn't used by step")
    );
    assert!(
        engine()
            .render("step garden.csv x=width y=yield err=yield_sd", None)
            .is_ok()
    );
    // errorbar is gone: scatter … err= draws the same.
    assert!(
        refused("errorbar garden.csv x=width y=yield err=yield_sd").contains("unknown plot type")
    );
}

#[test]
fn error_bars_too_short_to_see_get_a_hint() {
    // mass (~2) as sideways bars on width (200–850): drawn, but too short to see.
    let r = engine()
        .render(
            "line garden.csv x=width y=yield err=yield_sd xerr=mass",
            None,
        )
        .unwrap();
    assert!(
        r.advice
            .iter()
            .any(|a| a.text.contains("xerr= bars of yield") && a.text.contains("too short to see")),
        "{:?}",
        r.advice.iter().map(|a| &a.text).collect::<Vec<_>>()
    );
    // yield_sd on yield is fine: no hint about it.
    assert!(!r.advice.iter().any(|a| a.text.starts_with("err= bars")));
}

#[test]
fn skip_is_now_skiplines() {
    let ok = engine().render("line messy.csv skiplines=3 x=t y=temp", None);
    assert!(ok.is_ok(), "{:?}", ok.err().map(|e| e.message));
    assert!(
        engine()
            .render("line messy.csv skip=3 x=t y=temp", None)
            .is_err()
    );
}

#[test]
fn a_pattern_is_offered_with_each_folder_that_has_such_files() {
    let entry = |display: &str| FileEntry {
        path: display.into(),
        display: display.into(),
        modified: SystemTime::UNIX_EPOCH,
        sheet: None,
    };
    let files = [
        entry("Projects/EasyPlots/tests/fixtures/run_a.csv"),
        entry("Projects/EasyPlots/tests/fixtures/run_b.csv"),
        entry("old/run_1.csv"),
        entry("Projects/EasyPlots/tests/fixtures/simple.csv"),
    ];
    let found = pattern_folders(&files, "run_*.csv", 10);
    assert_eq!(
        found,
        vec![
            ("Projects/EasyPlots/tests/fixtures/run_*.csv".to_owned(), 2),
            ("old/run_*.csv".to_owned(), 1)
        ]
    );
    assert_eq!(pattern_folders(&files, "fixtures/RUN_?.csv", 10).len(), 1);
    assert!(
        pattern_folders(&files, "run_a.csv", 10).is_empty(),
        "no wildcard"
    );
}
