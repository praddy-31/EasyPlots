//! The "Try it" guide: short lessons shown in Settings, each a sample file, a command
//! and the picture it gives (round 8c). The commands use the built-in sample data
//! (`demo.rs`), and `tests/guide.rs` draws every one in `cargo test`, so a lesson that
//! stops working fails the build instead of going stale.

/// One lesson: a thing to try, in plain words, and the command that does it.
pub struct Lesson {
    /// The group it is shown under.
    pub section: &'static str,
    pub title: &'static str,
    /// One or two sentences: what the command shows and what to look for.
    pub text: &'static str,
    /// Ready to run; every file is a built-in `demo/` one.
    pub command: &'static str,
}

const L: fn(&'static str, &'static str, &'static str, &'static str) -> Lesson =
    |section, title, text, command| Lesson {
        section,
        title,
        text,
        command,
    };

pub fn lessons() -> Vec<Lesson> {
    vec![
        L(
            "First plots",
            "A line",
            "The simplest command: a plot type, a file, and which columns go on x and y. \
             This draws run A's temperature against time.",
            "line demo/run_a.csv x=t y=temp",
        ),
        L(
            "First plots",
            "Points",
            "Each row becomes a point with scatter — here yield against dose.",
            "scatter demo/trial.csv x=dose y=yield",
        ),
        L(
            "First plots",
            "Bars of means",
            "With stat=mean each group becomes one bar with an error bar (the standard \
             deviation); values writes the number on each bar.",
            "bar demo/categorical.csv x=bed y=yield stat=mean values",
        ),
        L(
            "First plots",
            "A histogram",
            "A histogram (hist) counts the values of one column into bins.",
            "hist demo/categorical.csv x=mass bins=10",
        ),
        L(
            "First plots",
            "A smooth density",
            "The smooth cousin of a histogram is kde: one curve per group with by=, each \
             top at 1 with norm=max.",
            "kde demo/categorical.csv x=mass by=soil norm=max",
        ),
        L(
            "First plots",
            "Boxes with the dots on top",
            "Layers go on top of each other with +: a box plot per bed, the single measurements as black \
             dots over it.",
            "box demo/categorical.csv x=bed y=yield + strip demo/categorical.csv x=bed y=yield color=black",
        ),
        L(
            "First plots",
            "A surface as colours",
            "Lines of equal z come from contour, from three columns (x, y and a value); \
             filled shades between them. cmap= picks the colour map.",
            "contour demo/surface.csv x=x y=y z=z filled",
        ),
        L(
            "Overlays and groups",
            "Two files on one plot",
            "Another file joins the plot with +; label= names each curve in the legend.",
            "line demo/run_a.csv x=t y=temp label=\"Run A\" + demo/run_b.csv x=t y=temp label=\"Run B\"",
        ),
        L(
            "Overlays and groups",
            "One series per group",
            "When a column names each row's group, by= splits the layer into one series \
             per group, each with its own colour and marker.",
            "scatter demo/categorical.csv x=width y=yield by=bed",
        ),
        L(
            "Overlays and groups",
            "Colour by a number",
            "Each scatter point takes its colour from a third column with c=, and a colour \
             bar on the right shows the scale; zlabel= names it.",
            "scatter demo/categorical.csv x=width y=yield c=mass zlabel=\"Mass (kg)\"",
        ),
        L(
            "Overlays and groups",
            "A second y axis",
            "A curve gets its own axis on the right with axis=right — here the much \
             smaller loss modulus beside the storage modulus. Each axis takes its \
             curve's colour.",
            "line demo/peak_onset.csv x=T y=storage + demo/peak_onset.csv x=T y=loss axis=right",
        ),
        L(
            "Fits and checking them",
            "A fitted line",
            "The least-squares line comes from fit=linear; its equation and R² go in the \
             legend. Also poly2…, exp, log, power, or your own formula in quotes.",
            "scatter demo/trial.csv x=dose y=yield fit=linear",
        ),
        L(
            "Fits and checking them",
            "The fit's numbers",
            "Each parameter ± its error, R², RMSE and N go in a corner with fitstats; \
             fitlabel=off keeps the equation out of the legend.",
            "scatter demo/trial.csv x=dose y=yield fit=linear fitstats fitlabel=off",
        ),
        L(
            "Fits and checking them",
            "Residuals as a histogram",
            "; starts a second panel. resid plots each point's distance from the fit; \
             the hist flag counts those distances into a histogram instead.",
            "scatter demo/trial.csv x=dose y=yield fit=linear ; resid demo/trial.csv x=dose y=yield fit=linear hist --layout=1x2 --size=7x3in",
        ),
        L(
            "Reading numbers off curves",
            "Peaks over a baseline",
            "A baseline (baseline=) is fitted to the quiet stretches (here two ranges) and \
             taken off, so the peaks sit on zero; peaks=2 rings the two that stand out. \
             Add showbaseline to see the fitted baseline before trusting it.",
            "line demo/spectrum.csv x=shift y=counts baseline=poly2,0:20,85:100 peaks=2",
        ),
        L(
            "Reading numbers off curves",
            "Onset of a step",
            "With onset, the baseline and the steepest tangent are drawn and where they \
             cross is ringed — the usual way to read where a drop or step begins.",
            "line demo/peak_onset.csv x=T y=storage onset",
        ),
        L(
            "Reading numbers off curves",
            "Where curves cross",
            "Every crossing of the curves is ringed with cross=curves; crosslabel=xy writes both \
             coordinates at each ring.",
            "line demo/run_a.csv x=t y=temp + demo/run_b.csv x=t y=temp cross=curves crosslabel=xy",
        ),
        L(
            "Several panels",
            "Panels of different sizes",
            "--layout=2x1 stacks two panels; --heights=3,1 makes the top one three \
             times as tall — a fit over its short residuals strip; --sharex gives both \
             the same x axis.",
            "line demo/run_a.csv x=t y=temp fit=linear ; resid demo/run_a.csv x=t y=temp fit=linear --layout=2x1 --heights=3,1 --sharex",
        ),
        L(
            "Several panels",
            "One legend for the figure",
            "--legend=bottom collects the panels' entries into one legend under the \
             figure instead of one per panel (also right and top).",
            "line demo/run_a.csv x=t y=temp label=\"Run A\" + demo/run_b.csv x=t y=temp label=\"Run B\" ; scatter demo/run_a.csv x=t y=temp label=\"Run A\" + demo/run_b.csv x=t y=temp label=\"Run B\" --legend=bottom --size=7x2.6in",
        ),
        L(
            "Several panels",
            "A broken axis",
            "An empty range is left out of the y axis with ybreak=, slanted strokes marking \
             the gap, so two tall bars don't squash the small ones.",
            "bar demo/outlier.csv x=sample y=value ybreak=20:460 values",
        ),
    ]
}
