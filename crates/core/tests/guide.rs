//! The Try-it guide can't go stale: every lesson's command is drawn here, using only the
//! built-in sample data (no data folder), exactly as the app and Terminal would.

use easyplots_core::{Config, Engine};

fn engine() -> Engine {
    let mut config = Config::default();
    // No data folder on purpose: lessons may only use the built-in demo/ files.
    config.general.data_dir = "/nonexistent-for-the-guide".into();
    Engine::new(config)
}

#[test]
fn every_lesson_draws() {
    let mut engine = engine();
    for lesson in easyplots_core::guide::lessons() {
        let drawn = engine.render(lesson.command, None);
        if let Err(e) = drawn {
            panic!(
                "lesson \"{}\": {}",
                lesson.title,
                e.display_with(lesson.command)
            );
        }
    }
}

#[test]
fn drawn_table_gives_the_numbers() {
    let mut engine = engine();
    // Two curves: long format with a series column; every point one row.
    let t = engine
        .drawn_table(
            "line demo/run_a.csv x=t y=temp + demo/run_b.csv x=t y=temp",
            None,
        )
        .unwrap();
    assert_eq!(t.lines().next(), Some("series\tx\ty"));
    assert!(t.lines().count() > 10, "{t}");
    // Summary bars: category names on x, the error bar in its own column.
    let t = engine
        .drawn_table("bar demo/categorical.csv x=bed y=yield stat=mean", None)
        .unwrap();
    assert_eq!(t.lines().next(), Some("series\tx\ty\terr"));
    assert!(t.contains("Shade"), "{t}");
}

#[test]
fn every_demo_file_opens_by_its_reserved_name() {
    let mut engine = engine();
    for (name, _) in easyplots_core::demo::FILES {
        // With and without .csv, as for real files.
        for typed in [name.to_string(), name.trim_end_matches(".csv").to_string()] {
            let table = engine
                .columns(&typed, None)
                .unwrap_or_else(|e| panic!("{typed}: {}", e.display_with(&typed)));
            assert!(!table.columns.is_empty(), "{typed} has no columns");
        }
    }
}

/// The lessons show good practice: none of them may set off the plot advisor.
#[test]
fn no_lesson_gets_a_hint() {
    let mut engine = engine();
    for lesson in easyplots_core::guide::lessons() {
        let r = engine.render(lesson.command, None).unwrap();
        assert!(
            r.advice.is_empty(),
            "lesson \"{}\": {:?}",
            lesson.title,
            r.advice
        );
    }
}
