//! The plot advisor: short hints under the preview when a known pitfall shows up (round
//! 11). Hints only look at the figure as built and drawn; they never change it. Where an
//! option fixes the pitfall and can be added cleanly, the hint carries the whole new
//! command (`fix`), which ⇥ in the preview puts in the box and draws.
//!
//! The checks, in the order hints are shown:
//! 1. the legend sits on data,
//! 2. one curve looks flat beside another on the same axis (ranges far apart),
//! 3. y (or x) spans four or more decades on a linear axis,
//! 4. bars that don't start at zero, or whose tops `ylim=` cuts off,
//! 5. more curves than the palette has colours,
//! 6. a `cross=` level no curve reaches,
//! 7. notes the panel builder left (`Panel::hints`, e.g. error bars from two values).

use serde::Serialize;

use crate::command;
use crate::figure::{AxisKind, Figure, Panel, Series};
use crate::render::Drawn;
use crate::spec::{Cross, LegendPos, PlotKind, PlotSpec, Spanned};

/// One hint: what was noticed and what to try, and the command with that option added
/// when it can be added cleanly.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Advice {
    pub text: String,
    /// The whole command with the fix in it (⇥ in the preview), or None: hint only.
    pub fix: Option<String>,
}

/// A curve this much flatter than its axis's whole range (2 %) looks like a flat line.
const FLAT: f64 = 0.02;
/// Four decades: 1 to 10 000.
const DECADES: f64 = 4.0;
/// Pieces of data under the legend before it counts as covering (one point can be a
/// near miss at the box's edge).
const COVERED: usize = 3;
/// Error bars shorter than this share of their axis's range (1 %) can't be seen.
const TINY: f64 = 0.01;

/// The hints for `command`, drawn as `fig` (`drawn` from `render_noting`, `colours` the
/// palette's length).
pub fn advise(
    command: &str,
    spec: &PlotSpec,
    fig: &Figure,
    drawn: &Drawn,
    colours: usize,
) -> Vec<Advice> {
    let mut out = Vec::new();
    // Residual strips belong to the panel above; folders can turn one written panel into
    // many. A fix is offered only where added words land on the right panel: the last
    // one, when the figure's panels are the written ones.
    let own: Vec<usize> = (0..fig.panels.len())
        .filter(|&i| !fig.panels[i].residual_of_prev)
        .collect();
    let written = own.len() == spec.panels.len();
    let several = own.len() > 1;
    for (n, &i) in own.iter().enumerate() {
        let panel = &fig.panels[i];
        let last = written && n + 1 == own.len();
        let fix = |more: &str| last.then(|| command::with_more(command, more));
        let at = |text: String| {
            if several {
                format!("panel {}: {text}", n + 1)
            } else {
                text
            }
        };

        // 1. The legend on top of data.
        if drawn.legend_covers.get(i).copied().unwrap_or(0) >= COVERED
            && panel.legend != LegendPos::Out
        {
            out.push(Advice {
                text: at("the legend covers data: legend=out puts it beside the plot".into()),
                fix: fix("legend=out"),
            });
        }

        // 2. Ranges far apart on one axis.
        if let Some(a) = flat_curve(panel) {
            let fix = if last {
                split_fix(command, spec, panel)
            } else {
                None
            };
            out.push(Advice {
                text: at(format!(
                    "{} looks flat beside the other curve: axis=right gives it its own axis",
                    a.label
                )),
                fix,
            });
        }

        // 3. Many decades on a linear axis.
        if !panel.logy
            && panel.ylim == (None, None)
            && panel.ybreak.is_none()
            && let Some(d) = decades(panel, |s| &s.y)
        {
            out.push(Advice {
                text: at(format!("y spans {d} decades: logy spreads them out")),
                fix: fix("logy"),
            });
        }
        if !panel.logx
            && panel.xlim == (None, None)
            && panel.xaxis == AxisKind::Number
            && let Some(d) = decades(panel, |s| &s.x)
        {
            out.push(Advice {
                text: at(format!("x spans {d} decades: logx spreads them out")),
                fix: fix("logx"),
            });
        }

        // 4. Bars that don't start at zero: their lengths no longer compare.
        let bars = panel
            .series
            .iter()
            .any(|s| matches!(s.kind, PlotKind::Bar | PlotKind::Hist));
        let cut = match panel.ylim {
            (Some(lo), _) if lo > 0.0 => true,
            (_, Some(hi)) if hi < 0.0 => true,
            _ => false,
        };
        if bars && (cut || panel.logy) {
            let why = if panel.logy { "logy" } else { "ylim=" };
            out.push(Advice {
                text: at(format!(
                    "bars don't start at 0 with {why}, so their lengths mislead: \
                     drop it, or show points (strip)"
                )),
                fix: None,
            });
        }
        // ... or whose tops (with their error bars) are cut off by the top of ylim=
        // (owner: ylim=0:6 hid the full bars without a word).
        if bars
            && !panel.horizontal
            && let (lo, Some(hi)) = panel.ylim
            && let Some(top) = bar_top(panel)
            && top > hi
        {
            let new = nice_above(top);
            let lo = lo.map_or(String::new(), crate::data::short);
            out.push(Advice {
                text: at(format!(
                    "the tallest bar reaches {}, above ylim's top ({}): its end is cut off",
                    crate::data::short(top),
                    crate::data::short(hi)
                )),
                fix: fix(&format!("ylim={lo}:{}", crate::data::short(new))),
            });
        }

        // 5. More curves than colours, and some of them drawn exactly alike (a marker
        // or dash of their own still tells them apart).
        let curves: Vec<&Series> = panel.series.iter().filter(|s| !s.is_fit).collect();
        let lines = curves
            .iter()
            .all(|s| matches!(s.kind, PlotKind::Line | PlotKind::Scatter | PlotKind::Step));
        let alike = curves.iter().enumerate().any(|(k, a)| {
            curves[..k]
                .iter()
                .any(|b| a.color == b.color && a.marker == b.marker && a.style == b.style)
        });
        if colours > 0 && curves.len() > colours && lines && alike {
            out.push(Advice {
                text: at(format!(
                    "{} curves but {colours} colours, so some look alike: \
                     split them into panels with ;",
                    curves.len()
                )),
                fix: None,
            });
        }

        // 6. cross=LEVEL that no curve reaches (often after norm=, which rescales the
        // curves but not the level).
        if let Some(sp) = written.then(|| &spec.panels[n])
            && let Some(Spanned {
                value: Cross::Levels(levels),
                ..
            }) = &sp.cross
            && let Some((lo, hi)) = finite_range(
                &curves
                    .iter()
                    .flat_map(|s| s.y.iter().copied())
                    .collect::<Vec<_>>(),
            )
            && levels.iter().all(|&l| l < lo || l > hi)
        {
            // The middle of the curves, to two significant figures (0.513 → 0.51).
            let m = (lo + hi) / 2.0;
            let step = 10f64.powf(m.abs().max(f64::MIN_POSITIVE).log10().floor() - 1.0);
            let mid = crate::data::short((m / step).round() * step);
            out.push(Advice {
                text: at(format!(
                    "no curve reaches cross={}: they run from {} to {} here",
                    levels
                        .iter()
                        .map(|l| crate::data::short(*l))
                        .collect::<Vec<_>>()
                        .join(","),
                    crate::data::short(lo),
                    crate::data::short(hi)
                )),
                fix: fix(&format!("cross={mid}")),
            });
        }

        // 7. Error bars too short to see beside their axis (owner, round 13:
        // `x=width xerr=mass`, mass ~2 against width 200–850, looked like no bars).
        for s in &curves {
            for (errs, values, which, opt) in
                [(&s.xerr, &s.x, "x", "xerr="), (&s.yerr, &s.y, "y", "err=")]
            {
                let Some(errs) = errs else { continue };
                let biggest = errs
                    .iter()
                    .chain(if which == "y" {
                        s.yerr_lo.iter().flatten()
                    } else {
                        s.xerr_lo.iter().flatten()
                    })
                    .copied()
                    .filter(|e| e.is_finite())
                    .fold(0.0, f64::max);
                if let Some((lo, hi)) = finite_range(values)
                    && hi > lo
                    && biggest > 0.0
                    && biggest < TINY * (hi - lo)
                {
                    out.push(Advice {
                        text: at(format!(
                            "{opt} bars of {} are at most {} long while {which} runs {} to {}: \
                             too short to see (is it the right column?)",
                            s.label,
                            crate::data::short(biggest),
                            crate::data::short(lo),
                            crate::data::short(hi)
                        )),
                        fix: None,
                    });
                }
            }
        }

        // 8. What the panel builder noticed.
        for h in &panel.hints {
            out.push(Advice {
                text: at(h.clone()),
                fix: None,
            });
        }
    }
    out
}

/// The top of the highest bar, error bar included.
fn bar_top(panel: &Panel) -> Option<f64> {
    panel
        .series
        .iter()
        .filter(|s| matches!(s.kind, PlotKind::Bar | PlotKind::Hist) && !s.right)
        .flat_map(|s| {
            s.y.iter().enumerate().map(move |(k, &y)| {
                // y is the bar's top (stacked bars carry their bottom in `base`).
                let err = s.yerr.as_ref().map_or(0.0, |e| e[k]);
                y + if err.is_finite() { err } else { 0.0 }
            })
        })
        .filter(|v| v.is_finite())
        .reduce(f64::max)
}

/// A round number a little above `v`: 6.3 → 7, 47 → 50, 0.81 → 0.9.
fn nice_above(v: f64) -> f64 {
    if v <= 0.0 {
        return 0.0;
    }
    let step = 10f64.powf(v.log10().floor());
    ((v / step).floor() + 1.0) * step
}

/// Curves drawn as lines or points, on the left axis, not fits.
fn plain_left(panel: &Panel) -> impl Iterator<Item = &Series> {
    panel.series.iter().filter(|s| {
        !s.is_fit
            && !s.right
            && matches!(s.kind, PlotKind::Line | PlotKind::Scatter | PlotKind::Step)
    })
}

fn finite_range(v: &[f64]) -> Option<(f64, f64)> {
    let (lo, hi) = v
        .iter()
        .filter(|v| v.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
            (a.min(v), b.max(v))
        });
    (lo <= hi).then_some((lo, hi))
}

/// The curve that looks flat beside the rest on the left axis (its own range under 2 %
/// of the axis's), when the panel doesn't already use a right axis.
fn flat_curve(panel: &Panel) -> Option<&Series> {
    if panel.has_right_axis() || panel.logy || panel.stacked || panel.ylim != (None, None) {
        return None;
    }
    let curves: Vec<(&Series, (f64, f64))> = plain_left(panel)
        .filter_map(|s| finite_range(&s.y).map(|r| (s, r)))
        .collect();
    if curves.len() < 2 {
        return None;
    }
    let lo = curves.iter().map(|c| c.1.0).fold(f64::INFINITY, f64::min);
    let hi = curves
        .iter()
        .map(|c| c.1.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let whole = hi - lo;
    if whole <= 0.0 {
        return None;
    }
    // A curve that is flat on its own (a constant) isn't helped by its own axis.
    let (s, r) = curves
        .iter()
        .filter(|(_, r)| r.1 > r.0)
        .min_by(|a, b| (a.1.1 - a.1.0).total_cmp(&(b.1.1 - b.1.0)))?;
    ((r.1 - r.0) / whole < FLAT).then_some(*s)
}

/// The command with the second of two curves moved to the right axis, when that is a
/// clean edit: the panel is the last one and has exactly two curves, either as two files
/// (`… + b.csv y=p` → add `axis=right` to the last) or as two columns of one
/// (`y=a,b` → `y=a + y=b axis=right`).
fn split_fix(command: &str, spec: &PlotSpec, panel: &Panel) -> Option<String> {
    if plain_left(panel).count() != 2 || panel.series.iter().any(|s| s.is_fit) {
        return None;
    }
    let layers = &spec.panels.last()?.layers;
    if layers.iter().any(|l| l.by.is_some() || l.fit.is_some()) {
        return None;
    }
    match layers.as_slice() {
        [a, b] if a.y.len() == 1 && b.y.len() == 1 => {
            Some(command::with_more(command, "axis=right"))
        }
        [l] if l.y.len() == 2
            && l.label.len() <= 1
            && l.color.len() <= 1
            && l.style.len() <= 1
            && l.marker.len() <= 1
            && l.err.is_empty() =>
        {
            let (first, second) = (&l.y[0], &l.y[1]);
            // Only a plain `a,b` list: nothing but the comma between the names.
            let between = command.get(first.span.end..second.span.start)?;
            if between.trim() != "," {
                return None;
            }
            let mut out = command.to_owned();
            out.replace_range(first.span.end..second.span.end, "");
            Some(format!(
                "{} + y={} axis=right",
                out.trim_end(),
                second.value
            ))
        }
        _ => None,
    }
}

/// Whole decades spanned by the curves' positive values, when it is four or more and
/// every value is above zero (a log axis would drop the rest).
fn decades(panel: &Panel, values: impl Fn(&Series) -> &Vec<f64>) -> Option<u32> {
    let curves: Vec<&Series> = plain_left(panel).collect();
    if curves.is_empty() || panel.has_right_axis() {
        return None;
    }
    let mut lo = f64::INFINITY;
    let mut hi = 0.0f64;
    for s in curves {
        for &v in values(s).iter().filter(|v| v.is_finite()) {
            if v <= 0.0 {
                return None;
            }
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    let d = (hi / lo).log10();
    (lo.is_finite() && d >= DECADES).then(|| d.floor() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;
    use crate::config::Config;
    use std::path::Path;

    fn engine(dir: &Path) -> Engine {
        let mut config = Config::default();
        config.general.data_dir = dir.display().to_string();
        Engine::new(config)
    }

    fn fixtures() -> Engine {
        engine(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures"))
    }

    fn hints(e: &mut Engine, command: &str) -> Vec<Advice> {
        e.render(command, None).unwrap().advice
    }

    #[test]
    fn a_plain_plot_gets_no_hints() {
        let mut e = fixtures();
        assert_eq!(hints(&mut e, "line run_a.csv x=t y=temp"), vec![]);
        assert_eq!(
            hints(
                &mut e,
                "line simple.csv x=time y=temp + y=pressure axis=right"
            ),
            vec![]
        );
    }

    #[test]
    fn a_flat_column_is_moved_to_the_right_axis() {
        let mut e = fixtures();
        let cmd = "line simple.csv x=time y=temp,pressure";
        let a = hints(&mut e, cmd);
        assert!(a[0].text.starts_with("pressure looks flat"), "{a:?}");
        let fix = a[0].fix.clone().unwrap();
        assert_eq!(fix, "line simple.csv x=time y=temp + y=pressure axis=right");
        // The fixed command draws and has nothing more to say.
        assert_eq!(hints(&mut e, &fix), vec![]);
        // Two files: axis=right on the last one.
        let a = hints(
            &mut e,
            "line simple.csv x=time y=temp + simple.csv y=pressure",
        );
        assert_eq!(
            a[0].fix.as_deref(),
            Some("line simple.csv x=time y=temp + simple.csv y=pressure axis=right")
        );
        // Three curves: a hint, but no edit that surely fixes it.
        let a = hints(
            &mut e,
            "line simple.csv x=time y=temp,pressure + y=temp label=again",
        );
        assert!(
            a[0].text.contains("looks flat") && a[0].fix.is_none(),
            "{a:?}"
        );
    }

    #[test]
    fn many_decades_suggest_a_log_axis() {
        let mut e = fixtures();
        let a = hints(&mut e, "line scaling.csv x=n y=slow");
        assert!(
            a.iter().any(|a| a.text.starts_with("y spans")
                && a.fix.as_deref() == Some("line scaling.csv x=n y=slow logy")),
            "{a:?}"
        );
        // Already logarithmic: quiet.
        let a = hints(&mut e, "line scaling.csv x=n y=slow logy logx");
        assert!(a.iter().all(|a| !a.text.contains("decades")), "{a:?}");
    }

    #[test]
    fn bars_cut_off_above_zero() {
        let mut e = fixtures();
        let a = hints(
            &mut e,
            "bar samples.csv x=site y=response stat=mean ylim=2:6",
        );
        assert!(
            a.iter()
                .any(|a| a.text.starts_with("bars don't start at 0")),
            "{a:?}"
        );
        assert!(hints(&mut e, "bar samples.csv x=site y=response stat=mean").is_empty());
    }

    #[test]
    fn a_covered_legend_and_too_many_curves() {
        let dir = std::env::temp_dir().join(format!("easyplots-advice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Points everywhere: no corner is free for the legend.
        let mut csv = String::from("x,y,g\n");
        for i in 0..40 {
            for j in 0..40 {
                csv.push_str(&format!("{i},{j},{}\n", (i * 40 + j) % 2));
            }
        }
        std::fs::write(dir.join("filled.csv"), csv).unwrap();
        // Ten groups of a line each.
        let mut csv = String::from("x,y,g\n");
        for g in 0..10 {
            for i in 0..5 {
                csv.push_str(&format!("{i},{},{g}\n", i + g));
            }
        }
        std::fs::write(dir.join("groups.csv"), csv).unwrap();
        let mut e = engine(&dir);
        let a = hints(&mut e, "scatter filled.csv x=x y=y by=g");
        assert_eq!(
            a[0].fix.as_deref(),
            Some("scatter filled.csv x=x y=y by=g legend=out"),
            "{a:?}"
        );
        assert!(hints(&mut e, "scatter filled.csv x=x y=y by=g legend=out").is_empty());
        let a = hints(&mut e, "line groups.csv x=x y=y by=g legend=out");
        assert!(
            a.iter().any(|a| a
                .text
                .starts_with("10 curves but 8 colours, so some look alike")),
            "{a:?}"
        );
        // Two panels: the lone legend goes above panel 1 by default, so nothing is
        // covered. With --legend=off it stays inside: the hint names the panel, and
        // offers no edit (words added at the end would land on panel 2).
        assert!(
            hints(
                &mut e,
                "scatter filled.csv x=x y=y by=g; line groups.csv x=x y=y"
            )
            .is_empty()
        );
        let a = hints(
            &mut e,
            "scatter filled.csv x=x y=y by=g; line groups.csv x=x y=y --legend=off",
        );
        assert!(
            a[0].text.starts_with("panel 1: the legend covers data"),
            "{a:?}"
        );
        assert!(a[0].fix.is_none(), "{a:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
