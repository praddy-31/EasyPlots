//! Golden-file tests: each example command must produce exactly the SVG stored in
//! `tests/golden/`. A change to how plots look then shows up as a failing test, so style
//! changes are always deliberate.
//!
//! After an intended change, refresh the files and look at the gallery:
//!   UPDATE_GOLDEN=1 cargo test -p easyplots-core --test golden
//!   GALLERY=1 cargo test -p easyplots-core --test golden   (writes docs/gallery/*.png)

use std::path::{Path, PathBuf};

use easyplots_core::{Config, Engine, Format, export};

const CASES: &[(&str, &str)] = &[
    ("line_basic", "line simple.csv x=time y=temp"),
    (
        "line_two_columns",
        r#"line simple.csv x=time y=temp,pressure title="Temperature and pressure""#,
    ),
    (
        "overlay_two_files",
        r#"line run_a.csv x=t y=temp label="Run A" + run_b.csv x=t y=temp label="Run B" style=dashed"#,
    ),
    (
        "scatter_overlay",
        "scatter run_a.csv x=t y=temp + run_b.csv x=t y=temp marker=s",
    ),
    (
        "bar_categories",
        "bar text_categories.csv x=month y=north,south",
    ),
    (
        "bar_dates",
        "bar dates.csv x=date y=sales_north color=green",
    ),
    ("line_dates", "line dates.csv"),
    ("line_times_markers", "line datetimes.csv markers grid=on"),
    (
        "missing_values",
        r#"line missing_and_comments.csv x=time y="Voltage (V)" markers ylim=1.15:1.35"#,
    ),
    (
        "decimal_comma",
        "line semicolon_decimal_comma.csv x=Zeit y=Temperatur,Druck legend=br",
    ),
    (
        "small_figure",
        "scatter tabs.csv x=temp y=pressure --size=8x6cm",
    ),
    // M3: log axes, legend outside, themes, fonts, maths, row filters.
    (
        "log_log",
        r#"line scaling.csv x=n y=quick,slow logx logy markers xlabel="Problem size $n$" ylabel="Time (s)""#,
    ),
    (
        "log_y",
        r#"scatter decay.csv x=t y=counts logy ylabel="Counts per second""#,
    ),
    (
        "legend_out",
        "line simple.csv x=time y=temp,pressure legend=out",
    ),
    (
        "maths_latin_modern",
        r#"line simple.csv x=time y=temp xlabel="$t$ (s)" ylabel="$T_{\mathrm{in}}$ (°C)" title="Heating at $\alpha = 0.5\,\mathrm{K/s}$, $\Delta T \approx 4$" --font="Latin Modern""#,
    ),
    (
        "theme_light",
        "bar text_categories.csv x=month y=north,south --theme=light",
    ),
    (
        "theme_dark",
        r#"line run_a.csv x=t y=temp label="Run A" + run_b.csv x=t y=temp label="Run B" --theme=dark"#,
    ),
    (
        "where_filter",
        r#"scatter run_a.csv x=t y=temp where="t < 12" + run_a.csv x=t y=temp where="t >= 12 and temp > 27" marker=s"#,
    ),
    ("hist", "hist samples.csv y=height"),
    // Owner's requests (round 3): a colour per column, smoothing, an error band,
    // reference lines and a shaded region.
    (
        "colour_lists",
        r#"line simple.csv x=time y=temp,"smooth(temp,9)" color=gray,red style=solid,dashed label=raw,smoothed"#,
    ),
    ("files_matching", "line run_*.csv x=t y=temp"),
    // Round 3: summary plots (one bar per file; groups of a column) and stats in the legend.
    (
        "summary_bars",
        r#"bar run_*.csv y=temp stat=mean title="one bar per file, mean ± sd" ; bar samples.csv x=site y=height,response stat=mean err=ci95 axis=left,right title="groups, 95% CI" ; scatter samples.csv x=dose y=response summary title=stats --size=10x3.4in"#,
    ),
    // Round 3: your own fit formula with its 95% band and a residuals strip.
    (
        "fit_formula",
        r#"scatter decay.csv x=t y=counts fit="a*exp(-x/b)+c" fitband residuals title="fit=\"a*exp(-x/b)+c\" fitband residuals""#,
    ),
    // Round 3: a y axis per column, a converted right axis, lines on the second axes.
    (
        "second_axes",
        r#"line simple.csv x=time y=temp,pressure axis=left,right hline2=101.2:"limit" title="axis=left,right" ; line simple.csv x=time y=temp ylabel="T (°C)" y2="y*1.8+32" y2label="T (°F)" hline2=80:"80 °F" title="y2=" --size=8x3.6in"#,
    ),
    // Round 5: numbers read off curves (a heating scan): peaks, onsets, area, slope, integral.
    (
        "curve_readings",
        r#"line heating.csv x=T y=storage,loss axis=left,right peaks onset title="peaks onset" ; line heating.csv x=T y=damping peaks area=80:130 title="peaks area=80:130" ; line heating.csv x=T y=storage smooth=5 slope + heating.csv x=T y=loss integral axis=right title="slope, integral" --size=8x7.5in --layout=2x2"#,
    ),
    // Round 6a: numbers on points and bars, corner notes, placed equations, correlation,
    // one-axis grids, twin axes coloured like their curves, `+` without a file.
    (
        "labels_and_notes",
        r#"line trial.csv x=dose y=growth err=growth_sd markers ylabel="growth (%)" + y=yield err=yield_sd axis=right y2label="yield (%)" xlabel="dose (%)" title="+ y=yield axis=right" ; scatter trial.csv x=growth y=yield err=yield_sd fit=linear fitlabel=tr corr=spearman grid=y note="made-up data"@bl title="fitlabel=tr corr=spearman" ; bar text_categories.csv x=month y=north values=%.0f grid=y title="values=%.0f" --size=10x3.4in --theme=light"#,
    ),
    // Owner's picks (half 1): messy files, workbooks and JSON.
    (
        "cleanup_messy",
        r#"line messy.csv skiplines=3 x=t y=temp title="as read" ; line messy.csv skiplines=3 x=t y=temp clip=0:100 dropna title="clip=0:100 dropna""#,
    ),
    (
        "workbook_sheets",
        r#"line workbook.xlsx x=t y=temp label="Run1" + workbook.xlsx sheet=Run2 skiplines=2 x=t y=temp label="Run2 (skiplines=2)""#,
    ),
    (
        "json_rows",
        r#"scatter readings.json x=t y=temp where="site == 'north'" fit=linear"#,
    ),
    (
        "band_and_lines",
        r#"line simple.csv x=time y=temp err="0.04*temp" band hline=20 vline=15 span=5:8"#,
    ),
    (
        "notes_and_labels",
        r#"line simple.csv x=time y=temp hline=20:"limit" span=5:8:"heating" vline=12:"switch" arrow="peak"@3.5,25.6 note="start"@0.5,20.3 arrow="low"@9.3,16.9"#,
    ),
    (
        "top_axis",
        r#"line scaling.csv x=n y=quick logx logy x2="1/n" x2label="Rate (1/n)" title="Top axis""#,
    ),
    (
        "top_axis_screen",
        r#"line simple.csv x=time y=temp x2="time*60" x2label="Minutes" yspan=22:24:"comfort" --theme=light"#,
    ),
    ("fft_spectrum", "fft signal.csv x=t y=voltage,current"),
    (
        "fft_power_journal",
        "fft signal.csv x=t y=voltage scale=power logy --size=aip",
    ),
    ("inset_zoom", "line simple.csv x=time y=temp inset=8:11"),
    (
        "point_shapes",
        "scatter series.csv x=x y=s1,s2,s3,s4,s5,s6,s7,s8,s9,s10 legend=out --size=acs-double",
    ),
    (
        "shapes",
        r#"line simple.csv x=time y=temp rect=8:11:16.5:18.5:"minimum" circle="peak"@16,28.2 segment=0,20:19.5,22.3 --theme=light"#,
    ),
    (
        "inset_log_double",
        "scatter scaling.csv x=n y=quick,slow logx logy inset=10:100 --size=acs-double",
    ),
    // Owner's request: the values of a histogram on x, and own tick steps.
    (
        "hist_x_steps",
        "hist samples.csv x=height xlim=155:185:5 ylim=::2",
    ),
    (
        "hist_overlap",
        r#"hist samples.csv y=height where="site == 'north'" label=north + samples.csv y=height where="site == 'south'" label=south bins=12"#,
    ),
    ("box_groups", "box samples.csv x=site y=height"),
    (
        "box_grouped_columns",
        r#"box samples.csv x=site y=response,"response*1.3" --theme=light"#,
    ),
    (
        "scatter_err_fit",
        r#"scatter samples.csv x=dose y=response err=sd fit=linear where="site == 'north'""#,
    ),
    ("step", "step run_a.csv x=t y=temp step=post"),
    (
        "area_stacked",
        "area text_categories.csv x=month y=north,south stacked",
    ),
    (
        "area_between",
        r#"area simple.csv x=time y="temp - 1","temp + 1" between label="±1 °C" + line simple.csv x=time y=temp color=black"#,
    ),
    (
        "bar_stacked",
        "bar text_categories.csv x=month y=north,south stacked",
    ),
    (
        "twin_axis",
        "line simple.csv x=time y=temp + simple.csv x=time y=pressure axis=right",
    ),
    (
        "calculated",
        r#"line simple.csv x=time/60 y="temp*9/5 + 32" xlabel="Time (min)" ylabel="Temperature (°F)""#,
    ),
    (
        "panels_grid",
        r#"line run_a.csv x=t y=temp ; hist samples.csv y=height ; box samples.csv x=site y=response ; scatter scaling.csv x=n y=quick logx logy fit=power --letters --suptitle="Four panels""#,
    ),
    (
        "panels_sharex",
        "line run_a.csv x=t y=temp ; line run_b.csv x=t y=temp color=red --sharex",
    ),
    (
        "heatmap_table",
        r#"heatmap matrix.csv values cmap=coolwarm zlim=-1:1 title="Correlations""#,
    ),
    ("heatmap_long", "heatmap surface.csv x=x y=y z=z"),
    (
        "contour_filled",
        r#"contour surface.csv x=x y=y z=z filled zlabel="Height (m)""#,
    ),
    ("contour_table", "contour grid.csv levels=8 cmap=magma"),
    (
        "kde_hist",
        "kde samples.csv y=height label=density + hist samples.csv y=height alpha=0.4 label=counts",
    ),
    ("violin_groups", "violin samples.csv x=site y=height"),
    (
        "polar",
        r#"polar pattern.csv x=angle y=gain,reference title="Antenna""#,
    ),
    // Round 6b: grouped data (like the owner's figures 5, 8, 9b and 11).
    (
        "strip_mean",
        r#"strip garden.csv x=bed y=depth mean hline=0 ylabel="Depth (cm)" title="Depth by bed" --theme=light"#,
    ),
    // Round 7b: a median with quartile bars and its numbers, alpha= as written, a median
    // line on a strip, the equation named from label=.
    (
        "median_bars",
        r#"bar garden.csv x=bed y=yield stat=median values=%.0f+- title="stat=median values=%.0f+-" ; bar garden.csv x=bed y=yield stat=mean values=%.0f+- title="stat=mean values=%.0f+-" --size=8x3.4in --theme=light"#,
    ),
    (
        "solid_boxes",
        r#"box garden.csv x=bed y=yield color=red alpha=1 + strip garden.csv x=bed y=yield color=black title="box alpha=1" ; strip garden.csv x=bed y=depth mean median=label title="mean (solid), median=label (dashed)" ; scatter trial.csv x=dose y=yield label="Trial plants" fit=linear title="label= names the points" --size=11x3.4in --theme=light"#,
    ),
    (
        "strip_horizontal",
        r#"strip garden.csv x=soil y=mass mean horizontal note="line = mean"@tr xlim=0:3.2 xlabel="Mass (kg), notched Izod" --theme=light"#,
    ),
    (
        "bar_highlight",
        r#"bar effects.csv x=factor y=height horizontal highlight="Fertiliser dose" values=%.0f% xlim=0:110 ylabel="" xlabel="Share of run-to-run variation (%)" title="Height: what explains it" --theme=light"#,
    ),
    (
        "scatter_by",
        "scatter garden.csv x=width y=yield err=yield_sd by=bed color=blue,gray,orange corr=spearman --theme=light",
    ),
    (
        "box_stats_highlight",
        r#"box garden.csv x=bed y=width summary highlight="Full sun""#,
    ),
    (
        "strip_over_box",
        "box garden.csv x=bed y=yield + strip garden.csv x=bed y=yield jitter",
    ),
    (
        "strip_dodged",
        "strip garden.csv x=bed y=mass by=soil mean=label",
    ),
    (
        "violin_horizontal",
        r#"violin garden.csv x=bed y=width horizontal summary groups="Full sun","Shade""#,
    ),
    (
        "bar_by_summary",
        "bar garden.csv x=bed y=yield stat=mean by=soil values=%.0f",
    ),
    // Round 7: baselines, crossings, scaling, fit checks.
    (
        "baseline_subtracted",
        "line spectrum.csv x=shift y=counts baseline=poly2,0:20,85:100 peaks=2",
    ),
    (
        "baseline_shown",
        "line spectrum.csv x=shift y=counts baseline=poly2,0:20,85:100 showbaseline",
    ),
    (
        "cross_curves",
        "line run_a.csv x=t y=temp + run_b.csv x=t y=temp cross=curves",
    ),
    (
        "cross_level",
        "line heating.csv x=T y=storage norm=max cross=0.5",
    ),
    ("kde_norm_max", "kde garden.csv x=mass by=soil norm=max"),
    (
        "fit_checks",
        "scatter trial.csv x=dose y=yield fit=linear fitstats fitlabel=off ; resid trial.csv x=dose y=yield fit=linear ; qq trial.csv x=dose y=yield fit=linear --layout=1x3 --size=7x2.6in",
    ),
    (
        "resid_overlay",
        "scatter trial.csv x=dose y=yield fit=linear + resid y=yield fit=linear axis=right",
    ),
    // Round 8a: colour by a number, residuals as a histogram, crossing labels.
    (
        "scatter_colour",
        r#"scatter garden.csv x=width y=yield c=mass zlabel="Mass (kg)""#,
    ),
    (
        "resid_hist",
        "scatter samples.csv x=dose y=response fit=linear ; resid samples.csv x=dose y=response fit=linear hist --layout=1x2 --size=7x3in",
    ),
    (
        "cross_labels",
        "line run_a.csv x=t y=temp + run_b.csv x=t y=temp cross=curves crosslabel=xy",
    ),
    // Round 8b: a broken axis, panel sizes, one legend for the figure, wide legends.
    (
        "broken_axis",
        "bar outlier.csv x=sample y=value ybreak=20:460 values",
    ),
    (
        "panel_sizes",
        "line run_a.csv x=t y=temp fit=linear ; resid run_a.csv x=t y=temp fit=linear --layout=2x1 --heights=3,1 --sharex",
    ),
    (
        "shared_legend",
        r#"line run_a.csv x=t y=temp label="Run A" + run_b.csv x=t y=temp label="Run B" ; scatter run_a.csv x=t y=temp label="Run A" + run_b.csv x=t y=temp label="Run B" ; step run_a.csv x=t y=temp label="Run A" --layout=1x3 --size=7x2.6in --legend=bottom --widths=2,1,1"#,
    ),
    (
        "shared_legend_right",
        r#"line run_a.csv x=t y=temp label="Run A" + run_b.csv x=t y=temp label="Run B" ; scatter run_a.csv x=t y=temp label="Run A" + run_b.csv x=t y=temp label="Run B" --legend=right --size=7x2.6in"#,
    ),
    // Round 10: a folder in place of a file gives one panel per file.
    ("folder_grid", "line runs/ x=t y=temp --size=7x2.6in"),
    (
        "legend_narrow_panel",
        "scatter trial.csv x=dose y=yield fit=linear fitstats ; resid trial.csv x=dose y=yield fit=linear ; qq trial.csv x=dose y=yield fit=linear --layout=1x3 --size=7x2.6in",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine() -> Engine {
    let mut config = Config::default();
    config.general.data_dir = root().join("tests/fixtures").display().to_string();
    // The stored pictures are in the journal look (the app's default is `light` since
    // 2026-11-12); cases that want another theme say so with --theme=.
    config.style.theme = "paper".into();
    Engine::new(config)
}

#[test]
fn golden_svgs() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let gallery = std::env::var_os("GALLERY").is_some();
    let golden_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut engine = engine();
    let mut failures = Vec::new();
    for (name, command) in CASES {
        let rendered = engine
            .render(command, None)
            .unwrap_or_else(|e| panic!("{name}: {}", e.display_with(command)));
        let path = golden_dir.join(format!("{name}.svg"));
        if update {
            std::fs::write(&path, &rendered.svg).unwrap();
        } else {
            let expected = std::fs::read_to_string(&path).unwrap_or_default();
            if expected != rendered.svg {
                failures.push(*name);
            }
        }
        if gallery {
            let png = export::encode(&rendered.svg, Format::Png, 110.0, command, name).unwrap();
            std::fs::write(root().join(format!("docs/gallery/{name}.png")), png).unwrap();
        }
    }
    assert!(
        failures.is_empty(),
        "these plots changed: {failures:?}\nIf the change is intended, run: UPDATE_GOLDEN=1 cargo test -p easyplots-core --test golden"
    );
}

#[test]
fn saves_files_without_overwriting() {
    let out = std::env::temp_dir().join(format!("easyplots-golden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let mut engine = engine();
    engine.config.output.dir = out.display().to_string();
    engine.config.output.name = "{file}_{type}".into();

    let a = engine.plot("line simple.csv x=time y=temp", None).unwrap();
    let b = engine.plot("line simple.csv x=time y=temp", None).unwrap();
    assert_eq!(a.path, out.join("simple_line.png"));
    assert_eq!(b.path, out.join("simple_line-2.png"));
    assert_eq!(&std::fs::read(&a.path).unwrap()[1..4], b"PNG");

    let pdf = engine
        .plot(
            "bar text_categories.csv x=month y=north --out=sales.pdf",
            None,
        )
        .unwrap();
    assert_eq!(pdf.format, Format::Pdf);
    assert!(std::fs::read(&pdf.path).unwrap().starts_with(b"%PDF"));

    engine.config.output.format = "svg".into();
    let svg = engine
        .plot("scatter simple.csv x=temp y=pressure", None)
        .unwrap();
    assert_eq!(svg.path.extension().unwrap(), "svg");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn preview_then_save_or_save_as() {
    let out = std::env::temp_dir().join(format!("easyplots-saveas-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut engine = engine();
    engine.config.output.dir = out.join("plots").display().to_string();
    engine.config.output.name = "{file}".into();

    // Rendering alone writes nothing.
    let rendered = engine
        .render("line simple.csv x=time y=temp", None)
        .unwrap();
    assert!(!out.join("plots").exists());

    let saved = engine.save(&rendered).unwrap();
    assert_eq!(saved.path, out.join("plots/simple.png"));

    // The same drawing in another format (launcher ⌘2), and the config's format.
    let saved = engine.save_in(&rendered, Some(Format::Pdf)).unwrap();
    assert_eq!(saved.path, out.join("plots/simple.pdf"));
    engine.config.output.format = "svg".into();
    let saved = engine.save(&rendered).unwrap();
    assert_eq!(saved.path, out.join("plots/simple.svg"));

    // Save As: any folder, format from the extension, replaces an existing file.
    let chosen = out.join("elsewhere.pdf");
    std::fs::write(&chosen, b"old").unwrap();
    let saved = engine.save_as(&rendered, &chosen).unwrap();
    assert_eq!(saved.format, Format::Pdf);
    assert!(std::fs::read(&chosen).unwrap().starts_with(b"%PDF"));
    assert!(engine.save_as(&rendered, &out.join("x.gif")).is_err());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn errors_point_at_the_problem() {
    let mut engine = engine();
    let cases = [
        ("line simple.csv x=tmie", "tmie", "did you mean \"time\""),
        (
            "line simpel.csv",
            "simpel.csv",
            "did you mean \"simple.csv\"",
        ),
        (
            "line text_categories.csv x=north y=month",
            "month",
            "contains text",
        ),
        (
            "line dates.csv x=sales_north y=date",
            "date",
            "dates can only go on the x axis",
        ),
        ("line simple.csv + dates.csv", "dates.csv", "can't mix"),
        (
            "line simple.csv x=time y=temp c=pressure",
            "pressure",
            "use it with scatter",
        ),
        (
            "scatter garden.csv x=width y=yield c=mass by=bed",
            "mass",
            "keep one of them",
        ),
        (
            "scatter garden.csv x=width y=yield c=bed",
            "bed",
            "contains text",
        ),
        (
            "line gallery_none/ x=t y=temp",
            "gallery_none/",
            "file not found",
        ),
        (
            "bar outlier.csv x=sample y=value ybreak=20:460 logy",
            "ybreak=20:460",
            "can't be combined with logy",
        ),
        (
            "bar outlier.csv x=sample y=value ybreak=600:700",
            "ybreak=600:700",
            "must lie inside the y axis",
        ),
        (
            "line simple.csv x=time y=temp crosslabel=y",
            "crosslabel=y",
            "add cross=curves",
        ),
        (
            r#"line simple.csv x=time y=temp x2="sin(x)""#,
            r#""sin(x)""#,
            "keep rising or keep falling",
        ),
        (
            "fft text_categories.csv x=month y=north",
            "month",
            "fft needs numbers or dates",
        ),
        (
            "line dates.csv x=date y=sales_north inset=1:2",
            "inset=1:2",
            "number x axis",
        ),
        (
            "line simple.csv x=time y=temp inset_at=tr",
            "inset_at=tr",
            "needs an inset= range",
        ),
        (
            "line simple.csv x=time y=temp --size=natur",
            "--size=natur",
            "did you mean \"nature\"",
        ),
        (
            r#"line simple.csv x=time y=temp x2="temp*2""#,
            r#""temp*2""#,
            "write the formula with x",
        ),
        (
            r#"line simple.csv x=time y=temp note="peak""#,
            r#"note="peak""#,
            "@",
        ),
        (
            r#"bar text_categories.csv x=month y=north x2="x*2""#,
            r#""x*2""#,
            "plain numbers",
        ),
        ("line nope_*.csv", "nope_*.csv", "no CSV files match"),
        (
            "line workbook.xlsx sheet=Run3",
            "workbook.xlsx",
            "did you mean \"Run1\"",
        ),
        (
            "line messy.csv every=0",
            "every=0",
            "every is a whole number",
        ),
        ("line simple.csv y=temp band", "simple.csv", "add err="),
        (
            "line simple.csv y=temp color=red,blue",
            "simple.csv",
            "2 values in color=",
        ),
        // A tick step needs plain numbers on that axis.
        (
            "line dates.csv xlim=::2",
            "xlim=::2",
            "only works when the x axis",
        ),
        ("line simple.csv ylim=0:40:0", "ylim=0:40:0", "above zero"),
        // Round 13: errorbar is gone (scatter … err= draws the same).
        (
            "errorbar samples.csv x=dose y=response err=sd",
            "errorbar",
            "plot type",
        ),
        // err= only where error bars are drawn.
        (
            "hist samples.csv x=response err=sd",
            "sd",
            "isn't used by hist",
        ),
        // Round 5: reading numbers off curves.
        (
            "bar heating.csv x=T y=storage peaks",
            "peaks",
            "reads numbers off curves",
        ),
        ("line dates.csv onset", "onset", "numbers on the x axis"),
        (
            "line heating.csv x=T y=storage area=200:300",
            "area=200:300",
            "outside the data",
        ),
        (
            "line heating.csv x=T y=storage area=5",
            "area=5",
            "x range to measure",
        ),
        (
            "line heating.csv x=T y=storage peaks=0",
            "peaks=0",
            "whole number",
        ),
        (
            "line heating.csv x=T y=storage onset=30:30.5",
            "onset=30:30.5",
            "baseline range",
        ),
        (
            "line heating.csv x=T y=storage slope integral",
            "integral",
            "can't both",
        ),
        (
            "bar heating.csv x=T y=storage slope",
            "heating.csv",
            "works with line",
        ),
        ("line dates.csv slope", "dates.csv", "needs numbers on x"),
    ];
    for (command, at, message) in cases {
        let err = match engine.render(command, None) {
            Ok(_) => panic!("{command} should fail"),
            Err(e) => e,
        };
        assert!(err.message.contains(message), "{command}: {}", err.message);
        assert_eq!(&command[err.span.clone().unwrap()], at, "{command}");
    }
}

#[test]
#[ignore = "speed check; run with --release --ignored"]
fn speed_50k_point_line() {
    let dir = std::env::temp_dir().join(format!("easyplots-speed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut csv = String::from("t,v\n");
    for i in 0..50_000 {
        let t = i as f64 * 0.001;
        csv.push_str(&format!(
            "{t:.3},{:.5}\n",
            (t * 6.0).sin() + 0.1 * (t * 200.0).sin()
        ));
    }
    std::fs::write(dir.join("big.csv"), csv).unwrap();
    let mut engine = engine();
    engine.config.general.data_dir = dir.display().to_string();
    engine.config.output.dir = dir.join("out").display().to_string();
    let saved = engine.plot("line big.csv x=t y=v", None).unwrap();
    let ms = saved.timings.total().as_secs_f64() * 1000.0;
    println!(
        "50k-point line → PNG at 300 dpi: {ms:.0} ms ({:?})",
        saved.timings
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        ms < 600.0,
        "took {ms:.0} ms (target 300 ms on an Apple Silicon Mac)"
    );
}

/// ⌘D keeps the look only; the style then draws another file in that look (owner's request).
#[test]
fn styles_keep_the_look_for_other_data() {
    let mut engine = engine();
    let dir = std::env::temp_dir().join(format!("easyplots-savestyle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    engine.styles_dir = dir.clone();
    let drawn = engine
        .render(
            r#"line simple.csv x=time y=temp,pressure color=black,red style=solid,dashed logy xlim=::5 legend=out title="Run 3" --theme=light --size=4x3in"#,
            None,
        )
        .unwrap();
    engine.save_style(&drawn, "mine").unwrap();
    let text = std::fs::read_to_string(dir.join("mine.toml")).unwrap();
    for wanted in [
        "theme = \"light\"",
        "size = \"4x3in\"",
        "legend = \"out\"",
        "logy = true",
        "xstep = 5.0",
        "colors = [\"black\", \"red\"]",
        "line_styles = [\"solid\", \"dashed\"]",
    ] {
        assert!(text.contains(wanted), "{wanted} missing in\n{text}");
    }
    // Never the data: no file, columns or title.
    for unwanted in ["simple", "temp", "Run 3"] {
        assert!(!text.contains(unwanted), "{unwanted} in\n{text}");
    }
    // Same name again is refused; a name with a space too.
    assert!(engine.save_style(&drawn, "mine").is_err());
    assert!(engine.save_style(&drawn, "two words").is_err());
    // Another file in that look: its two lines come out black and red, dashed second.
    let other = engine
        .render(
            "line run_a.csv x=t y=temp + run_b.csv x=t y=temp --style=mine",
            None,
        )
        .unwrap();
    assert!(other.svg.contains("stroke=\"#000000\"") && other.svg.contains("stroke-dasharray"));
    // A command with nothing about the look has no style to keep.
    let plain = engine
        .render("line simple.csv x=time y=temp", None)
        .unwrap();
    assert!(engine.save_style(&plain, "plain").is_err());
    assert_eq!(engine.style_names(), ["mine"]);
    engine.delete_style("mine").unwrap();
    assert!(engine.style_names().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The saved-plots browser lists EasyPlots' own plots, newest first, with thumbnails.
#[test]
fn recent_plots_and_thumbnails() {
    let mut engine = engine();
    let out = std::env::temp_dir().join(format!("easyplots-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    engine.config.output.dir = out.display().to_string();
    let a = engine
        .plot("line simple.csv x=time y=temp --out=first.png", None)
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let b = engine
        .plot("scatter run_a.csv x=t y=temp --out=second.svg", None)
        .unwrap();
    std::fs::write(out.join("not-ours.png"), b"not a plot").unwrap();
    let list = easyplots_core::recall::recent(&out, &out.join("pasted"), 10);
    let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["second.svg", "first.png"]);
    assert!(list[1].command.starts_with("line simple.csv"));
    // Pasted data saved in the plots folder itself: the plots are still listed.
    assert_eq!(easyplots_core::recall::recent(&out, &out, 10).len(), 2);
    for path in [&a.path, &b.path] {
        let png = easyplots_core::recall::thumbnail(path, 240).expect("a thumbnail");
        assert_eq!(&png[1..4], b"PNG");
    }
    let _ = std::fs::remove_dir_all(&out);
}

/// `run_*.csv` with one y column: lists go one entry per file.
#[test]
fn lists_go_per_file_for_patterns() {
    let mut engine = engine();
    let svg = engine
        .render("line run_*.csv x=t y=temp color=#123456,#abcdef", None)
        .unwrap()
        .svg;
    assert!(
        svg.contains("#123456") && svg.contains("#abcdef"),
        "both colours used"
    );
}

/// Second axes need what they refer to.
#[test]
fn second_axis_errors() {
    let mut engine = engine();
    for (command, wanted) in [
        ("line simple.csv x=time y=temp hline2=3", "right-axis units"),
        ("line simple.csv x=time y=temp vline2=3", "top-axis units"),
        (
            "line simple.csv x=time y=temp,pressure axis=left,right y2=y*2",
            "can't go with",
        ),
        (
            "line simple.csv x=time y=temp y2=(y-22)*(y-22)",
            "keep rising",
        ),
        (
            "line simple.csv x=time y=temp axis=left,right,left",
            "3 values in axis=",
        ),
    ] {
        let Err(e) = engine.render(command, None) else {
            panic!("{command} should fail");
        };
        assert!(e.message.contains(wanted), "{command}: {}", e.message);
    }
}

/// Summary plots: what works and what's pointed out.
#[test]
fn summaries() {
    let mut engine = engine();
    for ok in [
        "bar run_*.csv y=temp stat=median err=sem",
        "scatter samples.csv x=site y=height stat=mean err=none",
        "bar run_a.csv y=temp label=A stat=mean + run_b.csv y=temp label=B stat=mean",
    ] {
        if let Err(e) = engine.render(ok, None) {
            panic!("{ok}: {}", e.message);
        }
    }
    // A median's bars run from the lower to the upper quartile (the box plot's box: 53.7
    // to 60.4 round 55.8 for the shade bed); a mean's are ± sd unless err= says otherwise.
    let mut other = self::engine();
    let mut svg = |c: &str| other.render(c, None).unwrap().svg;
    let median = svg("bar garden.csv x=bed y=yield stat=median values=%.1f+-");
    assert!(
        median.contains("yield (median, Q1–Q3)"),
        "the axis says what the bars are"
    );
    assert!(
        median.contains("55.8 (53.7–60.4)"),
        "value with its quartiles"
    );
    assert!(svg("bar garden.csv x=bed y=yield stat=median err=sd values=%.0f+-").contains(" ± "));
    assert!(svg("bar garden.csv x=bed y=yield stat=mean err=iqr").contains("yield (mean, Q1–Q3)"));
    assert!(svg("bar garden.csv x=bed y=yield stat=mean values=%.0f+-").contains("55 ± 8"));
    // alpha= is the fill's real opacity: no built-in quarter.
    let solid = svg("box garden.csv x=bed y=yield alpha=1");
    assert!(!solid.contains("fill-opacity=\"0.25\""));
    assert!(svg("box garden.csv x=bed y=yield").contains("fill-opacity=\"0.25\""));
    assert!(svg("box garden.csv x=bed y=yield alpha=0.6").contains("fill-opacity=\"0.6\""));
    for (command, wanted) in [
        ("hist run_a.csv x=temp stat=mean", "bars or points"),
        (
            "bar run_*.csv y=temp stat=mean err=temp",
            "sd (standard deviation",
        ),
        ("bar run_*.csv stat=mean", "y=COLUMN"),
        ("bar run_a.csv y=temp stat=mode", "mean or median"),
        (
            "bar run_a.csv y=temp stat=mean + run_b.csv y=temp",
            "same stat=",
        ),
    ] {
        let Err(e) = engine.render(command, None) else {
            panic!("{command} should fail");
        };
        assert!(e.message.contains(wanted), "{command}: {}", e.message);
    }
}

/// Fit options point out what they need.
#[test]
fn fit_option_errors() {
    let mut engine = engine();
    for (command, wanted) in [
        ("scatter decay.csv x=t y=counts residuals", "add fit="),
        (
            "scatter decay.csv x=t y=counts fit=linear weighted",
            "err=COLUMN",
        ),
        ("scatter decay.csv x=t y=counts fitband", "go with a fit"),
        (
            r#"scatter decay.csv x=t y=counts fit="a*x+b" start=q:1"#,
            "isn't in the formula",
        ),
        (
            r#"scatter decay.csv x=t y=counts fit="a*t+b""#,
            "written with x",
        ),
        (
            "scatter decay.csv x=t y=counts fit=linear fitrange=100:200",
            "inside fitrange=",
        ),
    ] {
        let Err(e) = engine.render(command, None) else {
            panic!("{command} should fail");
        };
        assert!(e.message.contains(wanted), "{command}: {}", e.message);
    }
    let ok = engine
        .render("scatter samples.csv x=dose y=response err=sd fit=linear weighted through0 fitband fitrange=2:8", None);
    assert!(ok.is_ok(), "{:?}", ok.err().map(|e| e.message));
}

#[test]
fn curve_readings_for_the_terminal() {
    let mut engine = engine();
    let r = engine
        .render(
            "line heating.csv x=T y=storage,loss axis=left,right peaks onset area=80:120",
            None,
        )
        .unwrap();
    let text = r.fit_reports.join("\n");
    assert!(text.contains("storage: no peak found"), "{text}");
    assert!(text.contains("storage: onset at T = 97."), "{text}");
    assert!(text.contains("loss: peak at T = 99"), "{text}");
    assert!(text.contains("loss: area from T = 80 to 120 = "), "{text}");
}

/// Grouped data (round 6b): numbers checked by hand (Python) on `garden.csv`, and the
/// errors that point at the mistake.
#[test]
fn grouped_data() {
    let mut engine = engine();
    let r = engine
        .render("strip garden.csv x=bed y=depth mean", None)
        .unwrap();
    let text = r.fit_reports.join("\n");
    assert!(text.contains("Shade: mean = 7 ± 86 (sd, N = 9)"), "{text}");
    assert!(
        text.contains("Full sun: mean = -4 ± 76 (sd, N = 7)"),
        "{text}"
    );
    // The groups of one by= layer count together for corr; each group's own goes along.
    let r = engine
        .render(
            "scatter garden.csv x=width y=yield by=bed corr=spearman",
            None,
        )
        .unwrap();
    let text = r.fit_reports.join("\n");
    assert!(text.contains("r_s = 0.4957 (N = 24) (all)"), "{text}");
    assert!(text.contains("Full sun: r_s = 0.1970 (N = 7)"), "{text}");
    assert!(r.svg.contains("(all)"), "the plot says it's for all groups");
    // groups= keeps and orders; a quoted list works too.
    for cmd in [
        r#"box garden.csv x=bed y=width groups="Full sun","Shade""#,
        r#"box garden.csv x=bed y=width groups="full sun,shade""#,
    ] {
        let svg = engine.render(cmd, None).unwrap().svg;
        let (hot, cold) = (svg.find(">Full sun<"), svg.find(">Shade<"));
        assert!(hot.is_some() && cold.is_some() && hot < cold, "{cmd}");
        assert!(!svg.contains(">Part sun<"), "{cmd}: Part sun was left out");
    }
    // Lists follow the groups.
    let svg = engine
        .render(
            r#"scatter garden.csv x=width y=yield by=bed label="Cold","Middle","Hot" color=blue,gray,orange"#,
            None,
        )
        .unwrap()
        .svg;
    assert!(
        svg.contains(">Middle<") && svg.contains("#7f7f7f"),
        "labels and colours per group"
    );
    // stat=mean with by=: one bar series per group.
    let svg = engine
        .render(
            "bar garden.csv x=bed y=yield stat=mean by=soil values=%.0f",
            None,
        )
        .unwrap()
        .svg;
    for v in [">56<", ">38<", ">35<", ">55<", ">43<", ">34<"] {
        assert!(svg.contains(v), "bar value {v}");
    }
    for (command, at, wanted) in [
        ("bar garden.csv x=bed y=yield", "bed", "comes 9 times"),
        (
            "strip garden.csv x=bed y=yield groups=Shad",
            "Shad",
            "did you mean \"Shade\"",
        ),
        (
            "bar effects.csv x=factor y=flowers highlight=Compost",
            "Compost",
            "no bar or group",
        ),
        ("line simple.csv x=time y=temp mean", "mean", "strip plots"),
        (
            "line simple.csv x=time y=temp horizontal",
            "simple.csv",
            "not line",
        ),
        (
            "scatter garden.csv x=width y=yield by=bed color=red,blue,green,black",
            "garden.csv",
            "4 values in color= but 3 groups",
        ),
        (
            "line simple.csv x=time y=temp groups=a",
            "a",
            "groups= picks groups",
        ),
        ("barh effects.csv", "barh", "bar … horizontal"),
        (
            "scatter garden.csv x=width y=yield by=colour",
            "colour",
            "no column",
        ),
    ] {
        let Err(e) = engine.render(command, None) else {
            panic!("{command} should fail");
        };
        assert!(e.message.contains(wanted), "{command}: {}", e.message);
        if let Some(span) = &e.span {
            assert_eq!(&command[span.clone()], at, "{command}: {}", e.message);
        }
    }
}
