//! Round 12: the owner's reports from testing rounds 7–11 (2026-11-15), each kept as a
//! test so it can't come back. Uses the fixtures in tests/fixtures.
use easyplots_core::{Config, Engine};

fn engine() -> Engine {
    let mut config = Config::default();
    config.general.data_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures").into();
    Engine::new(config)
}

fn hints(command: &str) -> Vec<(String, Option<String>)> {
    let r = engine()
        .render(command, None)
        .unwrap_or_else(|e| panic!("{command}: {}", e.message));
    r.advice.into_iter().map(|a| (a.text, a.fix)).collect()
}

fn refused(command: &str) -> String {
    match engine().render(command, None) {
        Ok(_) => panic!("{command} was drawn"),
        Err(e) => e.message,
    }
}

#[test]
fn copied_numbers_have_plain_names_and_a_fit_column() {
    let t = engine()
        .drawn_table(
            "scatter trial.csv x=dose y=yield fit=linear where=\"dose > 1\"",
            None,
        )
        .unwrap();
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(lines[0], "series\tx\ty\tfit");
    // One row per point, no curve rows, no maths markup, no float noise.
    assert_eq!(lines.len(), 1 + 5, "{t}");
    assert!(!t.contains('$') && !t.contains("99999"), "{t}");
    let cells: Vec<&str> = lines[1].split('\t').collect();
    assert_eq!(&cells[..3], ["yield", "1.5", "72.1"]);
    assert!(cells[3].parse::<f64>().is_ok(), "{t}");
}

#[test]
fn stat_averages_files_only_when_there_is_something_to_average() {
    // One file, one row per t: nothing to average.
    assert!(refused("line run_a.csv x=t y=temp stat=mean").contains("one value"));
    // Two different measurements aren't averaged together.
    assert!(
        refused("line run_a.csv x=t y=temp stat=mean + simple.csv x=time y=pressure stat=mean")
            .contains("same y column")
    );
    // Two files at each t: drawn, the axis names them, a hint about 2-value error bars.
    let r = engine()
        .render("line run_*.csv x=t y=temp stat=mean err=iqr", None)
        .unwrap();
    assert!(
        r.svg.contains("mean of run_a, run_b"),
        "y label names the files"
    );
    assert!(r.advice.iter().any(|a| a.text.contains("only 2 values")));
}

#[test]
fn summary_describes_the_measurements_under_bars() {
    let r = engine()
        .render("bar samples.csv x=site y=response stat=mean summary", None)
        .unwrap();
    assert!(
        r.svg.contains("N = 20"),
        "N of the raw values, not of the 3 bars"
    );
    assert!(refused("heatmap surface.csv x=x y=y z=z summary").contains("heatmap"));
    // The old name is just an unknown word now (owner, round 14).
    assert!(refused("line run_a.csv x=t y=temp stats").contains("stats"));
}

#[test]
fn the_advisor_sees_cut_bars_and_unreachable_levels() {
    let h = hints("bar samples.csv x=site y=response stat=mean ylim=0:6");
    let cut = h
        .iter()
        .find(|h| h.0.contains("cut off"))
        .expect("cut-off hint");
    assert!(cut.1.as_deref().unwrap().contains("ylim=0:20"), "{cut:?}");
    let h = hints("line heating.csv x=T y=storage norm=max cross=1500");
    let level = h
        .iter()
        .find(|h| h.0.contains("cross=1500"))
        .expect("cross hint");
    assert!(
        level.1.as_deref().unwrap().ends_with("cross=0.51"),
        "{level:?}"
    );
}

#[test]
fn peaks_pick_curves_by_name_and_fitnames_uses_the_labels() {
    let r = engine()
        .render("line heating.csv x=T y=storage,loss peaks=loss", None)
        .unwrap();
    assert_eq!(r.fit_reports.len(), 1, "{:?}", r.fit_reports);
    assert!(r.fit_reports[0].starts_with("loss: peak"));
    assert!(
        refused("line heating.csv x=T y=storage,loss peaks=gain").contains("curves: storage, loss")
    );
    let r = engine()
        .render(
            "scatter trial.csv x=dose y=yield fit=linear xlabel=\"Dose (g)\" ylabel=Harvest fitnames=labels",
            None,
        )
        .unwrap();
    assert!(r.svg.contains("Harvest = 9"), "equation uses the labels");
    let r = engine()
        .render(
            "scatter trial.csv x=dose y=yield label=\"Trial yield\" fit=linear",
            None,
        )
        .unwrap();
    assert!(
        r.svg.contains(">yield = 98"),
        "label= stays in the legend only"
    );
}
