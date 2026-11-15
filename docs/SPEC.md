# Spec: commands, config and file names

The full description of the command language (§1), `config.toml` (§2) and output file
names (§3). The config example in §2 is checked by a test, so it can't drift from the
code.

## 1. The command language

The launcher's typed mode, its picker mode and the `easyplots` terminal command all use
this one language.

### Shape

```
TYPE  FILE [layer options]  [+ FILE [layer options]]…  [; TYPE FILE …]…  [--figure options]
```

- **`+`** adds another *layer* to the same axes (an overlay).
- **`;`** starts a new *panel* (a subplot). Each panel can have its own plot type.
- **`--key=value`** options apply to the whole figure and can go anywhere.
- A plain **`key=value`** applies to the layer it follows if it's a layer option, otherwise
  to the current panel.
- If `TYPE` is left out after `+`, the layer uses the panel's type. A different type is
  allowed: `line a.csv y=temp + scatter b.csv y=temp`.
- If the **file** is left out after `+` (the next word is an option such as `y=`), the
  layer reads the previous layer's file with its `x=`, `sheet=` and `skiplines=`:
  `line a.csv x=t y=temp + y=pressure axis=right fit=linear`. Everything else (cleaning,
  `err=`, `fit=`, colours) starts fresh for that layer. A bare flag right after `+` still
  needs a file.
- Several y columns in one layer (`y=a,b`) and one layer per column (`y=a + y=b`) draw the
  same plot; the list is shorter, `+` lets each curve have its own options.
- Panels are laid out in a grid (`--layout=ROWSxCOLS`); without it, two or three panels sit
  side by side, four make 2×2, more a near-square grid, and with `--sharex` they stack in
  one column. Without a `--size`, each extra row or column makes the figure ¾ of the
  default bigger in that direction.

### Examples

```sh
line data.csv x=time y=temp
line data.csv x=time y=temp,pressure title="Run 3" legend=tr
line a.csv x=t y=temp label="Run A" + b.csv x=t y=temp label="Run B"
scatter data.csv x=1 y=3 color=orange             # columns by number (1 = first)
line data.csv x=time y=temp markers dash=dashed ylim=0:40
bar sales.csv x=month y=north,south --size=15x10cm --out=sales.pdf
line data                                         # ".csv" may be left out
scatter data.csv x=1 y=3 logy                     # logarithmic y axis
line data.csv x=time y=temp where="temp > 20 and time < 60" --theme=dark
line data.csv ylabel="$T_{\mathrm{in}}$ (°C)" --font="Latin Modern"
line data.csv x=time/60 y="power/area"            # calculated columns
hist data.csv y=temp bins=40                      # histogram
scatter data.csv x=dose y=resp err=sd fit=linear   # error bars and a fitted line
line a.csv x=t y=temp ; hist a.csv y=temp --layout=1x2 --letters   # two panels
line data.csv x=t y=temp + data.csv x=t y=pressure axis=right    # second y axis
line data.csv --style=thesis                      # a style file
again ~/Plots/data_line_2026-10-22-101500.png     # edit a saved plot
```

### Files and columns

- `FILE` is a path (`~/` works), or a name looked up first in the terminal's current
  folder and then in `data_dir`. `.csv` may be left out. Names with spaces need quotes. In
  picker mode the full path is filled in for you.
- A pattern draws every matching file: `line run_*.csv x=t y=temp` gives one line per file
  (sorted by name), each labelled by its file name when there's one y column. `*` is any
  text and `?` one character; case doesn't matter.
- **A folder in place of a file** gives one panel per data file in it (CSV, TSV and
  workbooks; sorted by name), each titled with the file's name (after `title=` if
  given: `Run: run_a`): `line runs/ x=t y=temp` is a grid of every run, laid out
  automatically (or with `--layout=`). Other panels of the command stay as they are. Up
  to 36 files; more is an error that suggests a pattern instead. A name that is both a
  file and a folder means the file. The picker lists a folder that holds two or more data
  files as `runs/`. With `stat=` on its layer the folder's files are averaged instead:
  one panel, one layer per file, as with a pattern (`line runs/ x=t y=temp stat=mean`).
- `stat=` over files with x numbers puts the x values in numeric order and keeps them as
  numbers (files with different x steps share one true number line); bars, boxes and
  strips keep one slot per value. The axis label names the files by name, without folders.
- Typing a pattern without its folders in the launcher (`run_*.csv`) suggests it with
  each folder that has matching files, most matches first.
- Columns are named by header (any case), or by number starting at 1, or calculated (below).
  Names with spaces or commas need quotes: `y="Temp (°C)"`. Inside quotes, `\"` is a
  quote and `\\` a backslash; any other backslash is kept as typed (for maths: `$\alpha$`).
- `y=a,b,c` draws several columns from the same file; each becomes a legend entry.
  `x=t1,t2 y=a,b` pairs them up in order: a against t1, b against t2 (the counts must match).
- **An option is written once.** The same option twice for one file, one panel or the
  figure is an error (`mean mean=label` → "mean is given twice: keep one"), not "the last
  one wins". Other spellings count as the same (`color=` and `colour=`). Marks that add up
  may repeat: `hline=`, `vline=`, `span=`, `yspan=` (and their `2` forms), `note=`,
  `arrow=`, `rect=`, `circle=`, `segment=`. A panel option is written **once per panel,
  wherever it stands** (like `--legend=` once per figure): `line a.csv … peaks + b.csv …
  peaks` is "peaks is given twice", since `peaks` already reads every curve in the panel.
- **Two options that set one thing are refused together:** `legend=` (a panel's legend)
  with `--legend=` (one legend for all panels); `markers` with `marker=` on one file
  ("marker= already turns markers on").
- **One name, one meaning:** `summary` (N, mean ± sd) and `stat=` (one value per group)
  are different things; `cross=curves` and `cross=50` say which crossings; `--layout=2x3`
  arranges panels while `grid=` (without `--`) is the grid lines; `skiplines=` and
  `skiprows=` say which lines they skip; points with error bars are `scatter … err=`.
- Defaults: no `x` and no `y` → the first column is x and every other number column is y.
  Only `y` given → x is the row number (1, 2, 3…). Only `x` given → every other number
  column is y.
- An x column can hold numbers, dates (a date axis) or text (categories, e.g. month names).
  A y column must hold numbers.
- Legend labels are the column names (or `label=`). If two series would get the same label,
  the file name is added (`run_a: temp`, `run_b: temp`), or for the same file the row
  filter (`temp (t < 12)`, `temp (t >= 12)`).

### Calculated columns

Anywhere a column goes (`x=`, `y=`, `err=`, `xerr=`, and the left side of a `where=`
comparison), a calculation works too: `y=power/area`, `x=t/60`, `y="temp*9/5 + 32"`,
`y=log10(counts)`. A name that is a real column always means that column.

- `+ - * / ^` and brackets; numbers like `2.5` or `1e-3`; `pi` and `e`.
- Functions: `ln` (also `log`, as in Python), `log10`, `log2`, `exp`, `sqrt`, `abs`, `sin`,
  `cos`, `tan` (radians), `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `floor`, `ceil`,
  `round`, and four that look along the column: `cumsum` (running total), `diff` (change
  from the row before), `norm` (scaled to 0–1) and `smooth(col, n)` (the average of `n`
  rows around each row; `n` is 5 if left out). A comma inside brackets stays in the
  calculation: `y=smooth(temp,9),temp` is two columns.
- Columns with spaces or symbols in their name go in backticks: `` y="`Voltage (V)`*1000" ``.
  Calculations with spaces need quotes around the whole value.
- A missing value or an impossible result (`log` of a negative number, division by zero)
  leaves a gap, like an empty cell. Dates and text can't be calculated with.
- The axis label and legend show the calculation as typed.

### Other file types: Excel and JSON

- **Excel and LibreOffice workbooks** (`.xlsx`, `.xlsm`, `.xls`, `.ods`): the first sheet,
  or `sheet=Run2` (a name, any case, or a number from 1). Cells are read as values
  (formulas as their last result); Excel dates become dates. In the picker, a workbook in
  the data folder with several sheets is listed once per sheet (`results.xlsx › Run2`).
- **JSON**: rows (`[{"t": 1, "temp": 20}, …]`), columns (`{"t": [1, 2], "temp": [20, 21]}`),
  an array of arrays (the first row may be the header), or an object holding one of these
  (`{"data": […]}`). Listed in the data folder, never in whole-disk search, below the CSV
  and Excel files. `general.json_files` chooses which: `all` (default), `tables` (only
  files that start with `[` then `{`: rows; the file's first bytes are read, in the data
  folder only) or `known` (all but well-known settings files: `package.json`,
  `tsconfig.json`, `settings.json`, anything inside a folder whose name starts with `.`).
- **Text files** (`.txt`) are listed when their first line has a tab, comma or semicolon.
- Everything after reading works the same as for CSV files.

### Cleaning up messy data

Layer options, so each file (or each layer of the same file) can be cleaned differently:

| Option | Does |
|---|---|
| `skiplines=3` | ignore the first 3 lines of the file (instrument notes above the header); also for workbooks |
| `skiprows=10` | leave out the first 10 data rows (e.g. a warm-up period) |
| `every=5` | keep every 5th row (after `where=` and `skiprows=`) |
| `clip=0:100` | y values outside 0–100 count as missing (sensor glitches); `clip=:100` has no lower end |
| `dropna` | leave out rows with a blank in any drawn column, so lines don't break there |

### How CSV files are read

- The separator (`,` `;` tab `|`) is detected from the first lines.
- The first row is a header unless every cell in it is a number or a date.
- Lines starting with `#` and empty lines are skipped.
- With a `;`, tab or `|` separator, `1,5` is read as 1.5 (decimal comma).
- Numbers need no separators (`1234.5`). Thousands marks are also read: `1,234.5` and
  `12,345` (or `1.234,5` and `1.234.567` in a decimal-comma file). Only strict groups of
  three count, so `1,23` or `12.34.5` stay text; `1.234` in a decimal-comma file stays
  1.234 (it can't be told apart).
- Empty cells and `NaN`, `NA`, `N/A`, `null`, `none`, `-`, `--`, `?` are missing values:
  lines break at them and markers are left out.
- Dates: `2026-10-21`, `2026/10/21`, `21.10.2026`, with optional time `14:30[:05[.123]]`
  after a space or `T`. A time zone at the end (`Z`, `+02`, `+0200`, `+02:00`, `-0500`)
  turns the time into the **Mac's own time zone** (summer time by date): `10:00+02:00` is
  09:00 on a Mac in London in winter, so files from different zones line up. A time without a zone is plotted as written. The same applies to dates in
  `where=` and in options such as `vline=`.
- A column becomes text if any cell isn't a number or date; the error message then names
  the first such cell and its row.

### Plot types

| Type | Draws | Notes |
|---|---|---|
| `line` | lines through the points | several lines take solid, dashed, dotted, dash-dot in turn (one line stays solid), so they differ without colour; `dash=` chooses; `markers` adds markers (a different shape per line) |
| `scatter` | points only | each series gets its own shape (circle, square, triangle, diamond, triangle down, then the same outlined), so they differ without colour; `marker=` chooses |
| `bar` | bars from zero | several y columns side by side; `stacked` on top of each other. A text x column gives one bar per name; a name that comes more than once is an error (the bars would hide each other) that suggests `stat=mean`, `strip` or `box` |
| `step` | a line that changes in steps | `step=post` (default: each value holds until the next), `pre`, `mid` |
| `area` | a line filled down to zero | `stacked` piles several y columns up; `y=low,high between` fills between two columns |
| `hist` | a histogram of the column(s) in `x=` (`x=a,b` gives one each; `y=` works too) | `bins=30` (default: chosen from the data), `density`, `stacked`; several columns or layers share the bins and overlap see-through |
| `box` | box plots: median, the middle half of the values (box), whiskers to the furthest values within 1.5 box heights, the rest as open circles | `box f y=a,b,c` compares columns; `box f x=site y=height` makes one box per group in a text column; with several y columns the boxes stand side by side in each group |
| `kde` | a smooth density curve of the column(s) in `x=` (or `y=`), lightly filled | `bw=` sets the smoothing width (default: Scott's rule); next to a `hist`, the histogram switches to density so they share a scale |
| `violin` | box plots shaped by the density of their values, with the middle half as a thick bar and the median as a white dot | grouped like `box`; `bw=` |
| `strip` | every value as a dot, per group (a text `x=` column, or one group per y column) | dots that would touch move sideways (a beeswarm); `jitter` spreads them at random instead (the same every time); `mean` adds a line at each group's mean (`mean=label` writes it); each group gets its own colour; can go over `box` or `violin` of the same groups (the boxes turn grey) |
| `heatmap` | a table of numbers as coloured cells, with a colour bar | a table, or `x= y= z=` (below); `values` writes each number in its cell; `cmap=`, `zlim=`, `zlabel=` |
| `contour` | lines of equal value over a grid, coloured by value | `filled` fills between the lines; `levels=10` (about this many); can share a panel with `line`/`scatter` |
| `polar` | angle (`x`, degrees; `angle=rad` for radians) and radius (`y`) on a round plot | 0° to the right, counter-clockwise; `markers` adds markers; `ylim=` sets the radius range |
| `fft` | the spectrum of each y column: how strong each frequency is, from samples over x (times; a date column gives Hz) | `scale=amplitude` (default: a sine of size A peaks at about A), `power` (power per unit of frequency; add `logy`) or `db`; `window=hann` (default) or `none`. The mean is taken away first; samples that aren't evenly spaced are read off at even steps; frequencies run up to half the sampling rate |

| `resid` | each point's distance from its fit (data − fit), around a dashed zero line, against the same x | needs `fit=` (any fit and its options); the fit curve isn't drawn; points outside `fitrange=` have none; the label and axis read `yield − fit`. Beside the plot: `scatter f x=a y=b fit=linear ; resid f x=a y=b fit=linear`; on it: `… + resid y=b fit=linear axis=right`. The flag `hist` counts the residuals into a histogram instead (x axis `yield − fit`, y axis `Count`; `bins=` and `density` work; `hist` on another type points to `resid`) |
| `qq` | normal Q–Q plot of a fit's residuals: sorted residuals against the normal quantiles of their ranks (Blom: (i − 3/8)/(n + 1/4)), with a dashed line through both sets' first and third quartiles | needs `fit=` and at least 3 points; residuals that come from a normal distribution lie on the line, an S-shape means heavy or light tails, a bend means skew; x axis `Normal quantile` |

A histogram's x axis holds the values and its y axis the count (`Count`, or `Density`).
`box`, `violin`, `heatmap`, `polar` and `qq` need their panel to themselves (a `strip` may join
a `box` or `violin` of the same `x=` groups). `stacked` applies to the whole panel;
`grouped` (the default) undoes it.

### Heatmaps and contours

Two shapes of data work:

- **A table** (`heatmap matrix.csv`): every number column is a grid column, every row a grid
  row. The first column names the rows if it holds text, if its header cell is empty, or if
  the other headers are numbers (`y, 0, 0.5, 1…` with y values down the first column).
  `y=a,b,c` picks columns and `x=` the column naming the rows. In a heatmap the first row is
  at the top, as in the file; a contour uses the numbers in the header and first column as
  coordinates when there are any.
- **Long form** (`contour surface.csv x=x y=y z=height`): one row per point; rows with the
  same x and y are averaged; missing points stay empty. A heatmap can use text or dates for
  x and y; a contour needs numbers.

Colour maps (`cmap=`): `viridis` (default), `cividis`, `magma`, `plasma`, `inferno` (dark to
light, readable in grey), `coolwarm` (blue–white–red, for values around a middle, e.g.
correlations with `zlim=-1:1`), `blues`, `reds`, `gray`. The colour bar on the right shows
the scale, with `zlabel=` as its label.

### Layer options

`x`, `y`, `label`, `color`, `dash` (`solid`, `dashed`, `dotted`, `dashdot`; the old name
`style=` still works),
`width` (points), `marker` (point shapes: `o` circle, `s` square, `^` triangle, `v`
triangle pointing down, `d` diamond, the outlined `open-circle`, `open-square`,
`open-triangle`, `open-triangle-down`, `open-diamond`, and `x`, `+`, `none`), `alpha`
(0–1: how solid; on shapes that are see-through by default, such as boxes, violins, areas,
bands and overlapping histograms, it is the fill's real value, so `alpha=1` is solid and
a box without `alpha=` stays at 0.25), the flag `markers` (line with markers; each series gets the next shape), `where="…"` (keep
only matching rows, below), `err=` (also `yerr=`; one column, or one per y column) and
`xerr=` (error bars: `err=` on `line`, `scatter`, `bar` and `step`, `xerr=` on `line` and
`scatter`; other plot types refuse them), the flag `band` (draw `err=` as a shaded band around the line
instead of bars), `smooth=5` (each y column as a moving average over 5 rows; `rolling=`
points here), `baseline=` and `norm=` (below),
`axis=right` (measure on a second y axis), `fit=` (below), `step=` and the flag `between`
(see the plot types), `z=` (heatmaps and contours), and `c=` (scatter: colour each point
by a number column, below).

**One per column:** `label=`, `color=`, `dash=` and `marker=` take one value for every
y column, or a list in the order of the y columns: `y=temp,pressure color=orange,blue
dash=solid,dashed`. Missing entries get the usual defaults; more entries than columns is
an error. A label with a comma needs quotes: `label="Run A, warm"`.

**Colours** (`color=`) look like their name: `blue`, `orange`, `green`, `red`, `purple`,
`brown`, `pink`, `gray`, `olive`, `cyan`, `black`, `navy`, `sky`, `teal`, `dark-green`,
`lime`, `yellow`, `gold`, `dark-red`, `magenta`, `violet`, `light-gray`, `dark-gray`,
`white`, plus the colour-blind-safe Okabe–Ito colours the automatic colours come from:
`vermillion`, `bluish-green`, `reddish-purple`, `okabe-blue`. Spaces and dashes don't
matter (`dark green`); `#rrggbb` and `#rgb` work too.

**Fitted curves** (`fit=`): `linear`, `poly2` … `poly5` (also `quadratic`, `cubic`), `exp`
(y = a·e^(bx)), `log` (y = a + b·ln x) and `power` (y = a·x^b). The curve is drawn dashed
in the series' colour over the range of the data, and its legend entry shows the equation
with three significant figures and R² (1 means a perfect fit). Polynomials and `log` are
solved exactly; `exp` and `power` start from the straight line through the logarithms and
are then refined to the true least-squares curve (Levenberg–Marquardt); on a log y axis
they stay fitted to the logarithms, which is what looks straight there. Points where a
fit is impossible (`log` and `power` at x ≤ 0) are skipped. Works on `line`, `scatter`
and `step`.

**Your own formula:** `fit="a*exp(-x/b)+c"`: written with `x` and the same functions as
calculated columns; every other name is a parameter to fit (`pi` and `e` are the
constants). The legend shows the parameters and R². Starting guesses are tried
automatically; for a hard formula give rough ones: `start=a:5000,b:2`.

**Options:** `fitband` shades the 95% confidence band (where the true curve lies, given
the scatter); `through0` fits a polynomial through (0, 0); `weighted` counts each point
by 1/error² from `err=`; `fitrange=2:8` fits and draws only over that x range;
`residuals` adds a short strip under the plot with each point's distance from its fit
(on the same x axis; panels with a strip are stacked in one column; the `resid` plot type
is the flexible way). In Terminal the
saved plot's path is followed by each fit's numbers: R², N and every parameter ± its
standard error (from the covariance of the fit, scaled by the scatter left over).
`fitstats` (panel; `fitstats=tl|tr|bl|br`, default the emptiest corner) writes the same
numbers on the plot as a small table, one line each: every parameter ± its standard error,
R², RMSE (√(mean of the squared residuals), in y's units) and N; with several fits each
block starts with its series' name.

**Baselines:** `baseline=0:5,90:100` fits a polynomial to the points whose x lies in any of
the ranges and takes it off each y column, after `clip=`, `smooth=` and `dropna`, before
`slope`/`integral`. The shape comes first when it isn't a straight line:
`baseline=poly2,0:5,90:100` (`const`, `linear`, `poly2`, `poly3`); with no ranges
(`baseline=linear`) the first and last tenth of the x range are used. `showbaseline`
leaves the data as it is and draws the fitted baseline instead (dashed, in the curve's
colour) with the ranges shaded, to check them. Terminal gets the baseline's equation and
ranges. Works on `line`, `scatter`, `step` and `area` with numbers on x.

**Scaling (`norm=`):** each curve divided so that its largest value is 1 (`max`), its first
value is 1 (`first`), or its area (trapezoids, in order of x) is 1 (`area`); `range` maps
its smallest value to 0 and largest to 1. Applied after `kde`, `fft`, `slope` and
baselines, before fits; error bars are scaled with it; the y label becomes `Normalised …`
(`Relative density` for kde). Works on `line`, `scatter`, `step`, `area`,
`kde` and `fft` (`hist` has `density`). `norm(col)` in a calculation is the 0–1 scaling of
one column.

**Colour by a number (`c=`):** on `scatter` only, `c=mass` colours each point by that
column's value through the heatmap colour maps (`cmap=`, default viridis), with a colour
bar on the right; `zlabel=` names it and `zlim=1:3` fixes the colour range (values beyond
its ends keep the end colours). `c=` takes a calculation too (`c=log10(dose)`). It can't
go with `by=` (one colouring at a time), `color=` (`cmap=` picks the look) or `stat=`,
`slope`, `integral` (no longer one value per point); each error says so. `c=` on another
plot type points to `scatter`.

### Row filters: `where=`

`where="temp > 20 and site == 'north'"` keeps only the rows that match, per layer, so the
same file can be drawn twice with different filters.

- Comparisons: `>` `<` `>=` `<=` `==` (or `=`) `!=`, joined with `and`, `or`, `not` and
  brackets. `and` binds tighter than `or`.
- Columns by name (any case); names with spaces in backticks: `` `Voltage (V)` > 1.2 ``.
- Number columns compare with numbers, date columns with dates (`day >= 2026-03-01`), text
  columns only with `==` / `!=` and a quoted value (`site == 'north'`).
- Missing values never match. An index x axis keeps the original row numbers.

### Grouped data: `by=`, `groups=`, `horizontal`, `highlight=`

For data where the groups are in one column (a row per measurement, a column saying
which bed, sample or soil it is):

- **`by=bed`** splits a layer into one series per value of `bed`, in the order they
  first appear, each with its own colour and marker and a legend entry named after its
  group. `label=`, `color=`, `marker=` and `dash=` lists follow the groups
  (`by=bed color=blue,gray,orange`); one `label=` goes in front of each group's name.
  Works with every type but heatmaps, contours and polar plots; with `stat=mean` it gives
  grouped bars, one series per group (`bar f x=bed y=yield stat=mean by=soil`). `corr`
  takes a `by=` layer's groups together (`r_s = 0.78 (all)`); each group's own value
  goes to the Terminal. Up to 40 groups.
- **`groups=Cold,Hot`** keeps only those groups (of `by=`, or of a text x column), in that
  order: the order of the boxes, strips, bars and colours. Names match in any case; a
  quoted list works too (`groups="Full sun,Shade"`).
- **`strip`** with a text x column is grouped by it, one colour per group; with `by=` as
  well (`strip f x=bed y=mass by=soil`) each group's dots sit side by side, like
  grouped bars.
- **`horizontal`** lays `bar`, `box`, `violin` and `strip` plots on their side: values
  along x, groups down y, the first at the top. Options are as seen on the page:
  `xlabel=`, `xlim=`, `logx` and `vline=` are about the values. Not with `axis=right`,
  `x2=`, `y2=`, `inset=`, shapes, `residuals` or `--sharex`/`--sharey`.
- **`highlight="Bed temperature"`** keeps the named bars or groups in colour (a list for
  several) and turns the rest grey; a highlighted group's name is written in bold. It
  names a group of `by=` (a series), or a bar, box or strip on a text x axis.
- **`summary`** on `box`, `violin` and `strip` (and `bar … stat=`) writes N and mean ± sd under each group's name
  (two small lines) instead of in a legend.

`mean` also prints each group's mean ± sd and N in Terminal. `median` / `median=label`
do the same at each group's median, as a dashed line (so both can be shown), and print
the median with its quartiles.

### Panel options

`title`, `xlabel`, `ylabel` (`xlabel=""` removes the label), `xlim=0:10`, `ylim=:5`
(a third number is the tick step: `xlim=0:100:20` puts ticks at 0, 20 … 100, `ylim=::5`
only sets the step; ticks start at the lower limit, or at multiples of the step; for
plain number axes, not dates, names, log or polar),
`legend=best|tr|tl|br|bl|out|off` (`best` keeps clear of points, line pieces, error bars,
an inset and corner notes), `grid=on|off|x|y` (`x`: vertical lines only, `y`: horizontal
only), the flags `logx`, `logy`, `logy2` (the right axis), `stacked`,
`grouped`, `density`, `bins=` (histograms), `y2label=`, `y2lim=` for the right axis,
reference lines `hline=0` and `vline=12.5,20` (thin dashed lines across the plot) and
shaded regions `span=2:5` (across x) and `yspan=20:25` (across y; several with commas),
which are always kept in view; each can carry a label after another colon
(`hline=20:"limit"`, `vline=12:"switch on"`, `span=5:8:"heating"`), written in the tick
font just inside the plot. Text at a point: `note="start"@0.5,20` puts the text just
above-right of the point (x, y in data units; a date works for x); text in a corner:
`note="n = 6 each"@tr` (`tl`, `tr`, `bl`, `br`), several in one corner stacked inwards and
placed past a legend fixed in that corner; and
`arrow="peak"@3.5,25.6` puts it a little way off, towards the middle of the plot, with an
arrow to the point (repeat either for more). **Shapes** in data units, outlined in the
text colour (repeat for more; they're kept in view and appear in an inset too):
`rect=2:5:20:25` (x from 2 to 5, y from 20 to 25; `rect=2:5:20:25:"zone A"` labels it
inside its top left), `circle=@3.5,25.6` (a ring of fixed size around a point, so it
stays round; `circle="peak"@3.5,25.6` labels it) and `segment=0,20:10,30` (a straight
line between two points). A **top x axis** in other units:
`x2="1240/x"` gives the top axis's values as a formula of the bottom's (`x`, or the x
column's name), with `x2label=` for its label; ticks are round numbers of the top axis
placed where they belong, so a curved conversion (1/x) is fine. The formula must keep
rising or keep falling across the x range, and the bottom axis must be plain numbers (on
a log bottom axis the top one is log too). An **inset**: `inset=5:8` draws a zoomed copy of
x from 5 to 8 (y fitted to the data there; `inset=5:8:20:25` sets y too) in the emptiest
corner of the plot, with a thin box on the zoomed region and two guide lines to it;
`inset_at=tr|tl|br|bl` picks the corner (no guide lines if it then covers the region). The
legend keeps clear of it. Number x axes only. And
`cmap=`, `zlim=`, `zlabel=`, `levels=`, `filled` (heatmaps and contours), `bw=`
(kde and violin), `angle=deg|rad` (polar).

**Numbers on the plot:** the flag `values` writes each value above its bar (below a
negative one, in the middle of a stacked segment, above the error bar when there is
one), above its point on `line`, `scatter` and `step` (under it when there's no
room), and in each heatmap cell; the y range gets a little more room at the top for
them. Three significant figures by default; `values=%.1f` (or `values=1`) sets the
decimals and keeps text around them (`values=%.0f%` → `41%`, `values="±%.2f"`). `+-`
right after the number adds the error bar's size: `values=%.0f+-` → `41 ± 3`, or
`41 (36–47)` when the bar is longer one way (a median's quartiles); `±` works too. A value
that would round to zero but isn't zero is written `<1` (`<1%`, `<0.1`…; `>−1` when
negative), as in tables.

**Second y axis:** layers with `axis=right` are measured on their own axis on the right,
with its own ticks and label (`y2label=`, default the column name); `logy2` makes it
logarithmic. When all the curves on one side share a colour, that side's numbers and
label take it, so it's clear which axis is whose (`axiscolor=off` keeps both black).
`axis=` takes a list like `color=`, one per y column (`y=storage,loss axis=left,right`, e.g.
storage modulus left, the much smaller loss modulus right); never chosen automatically.
**A converted right axis**: `y2="y*1.8+32"` labels the right side in other units of the
left (`y` in the formula), with `y2label=`; like `x2=`, ticks are round numbers placed
where they belong and the formula must keep rising or falling. It can't go with
`axis=right` columns (those have their own right axis).
**Lines on the second axes:** `hline2=` and `yspan2=` take right-axis values (with labels,
like `hline=`), `vline2=` top-axis (`x2=`) values. Each needs that axis.

**Fitted equations:** with short column names (a word of up to 8 letters and digits),
the equation uses them, upright, with `·` for times: `yield = −1.39 · growth + 85.8`; a single
letter stays italic (`T = 0.4 · t + 19`), and longer names keep `y` and `x`. A `label=`
names the legend entry only. `fitnames=labels` writes the axis labels instead, whatever
their length, with units in brackets left out (`xlabel="Dose (g)" ylabel="Harvest (kg)"`
→ `Harvest = 98 − 18.4 · Dose`; a label with maths of its own falls back to y and x);
`fitnames=columns` is the default. Three
significant figures; `fitdigits=4` sets them (1–8). `fitlabel=` says where it goes:
`legend` (default), a corner `tl|tr|bl|br` (as a corner note; the fitted line then has no
legend entry), a point `@20,50` in data units, or `off` (Terminal only).

**Correlation:** `corr` writes Pearson's r of each curve's x and y in the emptiest corner
(away from a fixed legend), two decimals; `corr=spearman` writes Spearman's r_s (Pearson's
r of the ranks, ties sharing their mean rank), for curves that rise or fall without being
straight. Several curves are named. Terminal prints four decimals and N. Points with a
blank in x or y are left out; fewer than 3 points or no spread give nothing.

**Ticks:** about one per 40 pt of height (60 pt of width), at nice steps (1, 2, 2.5, 5 ×
10ⁿ), but never fewer than two on a number axis: a narrow axis steps down
(20 → 10 → 5 → 2.5 → 2…) until two fit.

**Statistics:** the flag `summary` adds a second, smaller line under each series' legend
entry: mean ± standard deviation, median and N of its y values (as drawn: after `where=`,
`clip=` and `smooth=`; for `hist`/`kde` the values counted). Numbers are given to the
precision the spread supports (101.04 ± 0.31). The legend shows even for one series.
On `bar … stat=` it writes N and mean ± sd of the measurements under each bar (not of the
bar heights); `heatmap`, `contour` and `polar` refuse it.
`easyplots --stats FILE…` prints N, mean, sd, median, min and max of each number column.
No statistical tests: choosing one needs judgement a plotting app can't give.

**Reading numbers off curves:** two layer flags replace each y column before drawing:
`slope` (dy/dx from the neighbouring rows in file order: central differences, one-sided at
the ends) and `integral` (the running area by trapezoids from the first row; a blank row
stays blank and the total carries on). They work on `line`, `scatter`, `step` and `area`
with numbers on x, after `clip=`, `smooth=` and `dropna`; the column's name becomes
`dE1/dT` or `∫storage dT`. Not both on one file (give it twice with `+`), and not with `err=`.
Four panel options read every data curve in the panel (not fits; `line`, `scatter`, `step`,
`area`, `kde`, `fft`; numbers on x; right-axis curves too), draw what they find
in the curve's colour, and print one line each in Terminal after the path:
Each reads every curve unless names pick some: `peaks=loss`, `dips=storage,loss`, `onset=storage`, or
with a count or range too (`peaks=loss,3`, `onset=storage,20:40`); names are the legend's, and a
name that isn't there is refused with the ones that are.
- `peaks` / `peaks=N` (1–50) and `dips` / `dips=N`: the most prominent high (or low)
  points. Prominence is how far a peak stands above the higher of the lowest points on
  either side before higher ground; a peak needs at least 5 % of the curve's height range
  and can't be an end point. Ringed, labelled with x.
- `onset` / `onset=A:B`: the two-tangent onset. The steepest point is searched up to the
  point farthest from the curve's first value (the whole curve for a step, the rising
  side for a peak), on a slope smoothed over about 2 % of the points; the tangent is a
  straight line through the points around it. The baseline is a straight line through the
  points in `A:B`, or by default the first third of the way to the steepest point. Both
  lines are drawn dashed, kept within the curve's height; the crossing is ringed and
  labelled `onset X`.
- `area=A:B`: the area under the curve down to y = 0 between x = A and B (the ends read off
  the line between neighbouring points; clipped to the data), shaded lightly and shown
  under the curve's legend entry (`area 3.95`).
- `cross=curves`: where curves cross. Every pair of data curves on the same y axis is compared
  over the x range they share: both are read off their straight lines at every x either
  has, and a change of sign of the difference is a crossing (touching without crossing
  isn't). Ringed in the text colour, labelled with x; Terminal gives x and y.
- `cross=50` (or a list, `cross=20,80`): where each curve crosses y = 50, in row order (a
  heating-then-cooling scan gives both), read off the line between rows; a dashed line
  marks each level. Half height: `norm=max cross=0.5`. When nothing crosses, Terminal
  says the range the curve covers.
  At most 12 crossings are ringed per pair or level (noisy data); Terminal gives the count.
- `crosslabel=x|y|xy|off` says what each crossing's label reads: the x value (the
  default), the y value, both as `(4.59, 21.4)` (each to three significant figures), or
  nothing (rings only). It needs `cross=` in the panel ("needs cross=" in the
  launcher); Terminal always gets both values.
Labels try above, below, right and left of their ring and take the first place clear of
the plot's edge, the rings and the labels already placed; a thin halo in the background
colour keeps them readable over lines.

**Summary plots:** `stat=mean` (or `median`) on `bar`, `scatter` or `line`
turns each file into one value: `bar sample_*.csv y=modulus stat=mean` gives one bar per
matching file, named after the file (or its `label=`). With `x=COLUMN` the rows are
grouped by that column's values instead (`bar data.csv x=sample y=modulus stat=mean`;
several files add to the same groups; `by=` splits each group into one bar per value of
another column). Error bars show the sample standard deviation for a mean, and for a
median the quartiles: the bar runs from the lower (Q1) to the upper quartile (Q3), so it
can be longer on one side. `err=` changes that: `sd`, `sem` (standard error, sd/√N),
`ci95` (95% confidence interval, Student's t), `iqr` (Q1 to Q3, also round a mean) or
`none`. Several y columns give bars side by side, each with its error bar.
`where=`, `clip=`, `skiprows=` and `every=` apply to the values first. Every layer of the
panel needs the same `stat=` and the same y column (they are averaged together). With a
number `x=` and several files (`line run_*.csv x=t y=temp stat=mean`) the files are
averaged at each x into one curve, named "mean of 2 files", the y axis naming the files.
Refused when nothing is averaged (one value per group: one file with one row per x). The y axis is labelled `modulus (mean ± sd)` or
`modulus (median, Q1–Q3)`.

`legend=out` puts the legend to the right of the plot, without a box, and narrows the plot
to make room.

**Broken y axis:** `ybreak=20:460` leaves that range out of the left y axis: the axis
runs normally below and above the gap, which is drawn as a narrow band with a slanted
stroke on each axis at its ends; bars and lines crossing the gap are cut there. The range
must lie inside the axis (otherwise the error says how far the axis runs), and the broken
axis can't be logarithmic (`logy`) or shared with a right axis (`axis=right`, `y2=`);
each refusal says so. Ticks are placed on each part separately.

**Log axes** (`logx`, `logy`) need numbers. Values at or below zero can't be shown and are
left out (a line breaks there); if nothing is left, the error says so. Whole decades get
labels: plain numbers from 0.001 to 1000, `10ⁿ` beyond; 2–9 × each decade are minor
ticks. A range shorter than one decade gets ordinary steps. Bars on a log axis start at the
bottom of the axis.

The legend appears when there is more than one series (or when `legend=` is given).
`best` tries the corners, then the middle of each side, then the centre, and picks the spot
covering the least data.

**A legend wider than its panel** (a fit's equation in a narrow panel): each equation's
`(R² = …)` moves to a second, smaller line first; if that still doesn't fit, the legend's
text shrinks, down to 70 %.

### Figure options

**Journal sizes** (`--size=NAME`, also in a style file or `output.size`): the name alone is
one column, `-double` the full page width (`-mid` the in-between width where there is one).
Narrow figures are ¾ as tall as wide, full-width ones 0.42. The font size becomes the
journal's usual one (8 pt; 7 pt for Nature and Science) unless a `--style` file sets
`font_size`.

| Name | One column | Full width | Journals |
|---|---|---|---|
| `column` / `double` | 3.5 in | 7 in | any (IOP, Wiley, *Quantum* have no fixed width) |
| `acs` | 3.25 in | 7 in | JCTC, J. Phys. Chem. A/B/C, JACS |
| `aip` | 8.5 cm | 17 cm | J. Chem. Phys., APL |
| `aps` | 3 3/8 in | 6 3/4 in | Phys. Rev. Lett., PRA, PRB |
| `rsc` | 8.3 cm | 17.1 cm | PCCP, Chem. Sci. |
| `nature` | 89 mm | 183 mm | Nature, npj Quantum Information, Nature Comms |
| `science` | 5.5 cm (`-mid` 12 cm) | 18.3 cm | Science |
| `elsevier` | 90 mm (`-mid` 140 mm) | 190 mm | Comput. Phys. Commun., Chem. Phys. |

`--out=NAME` (a name inside the output folder, subfolders allowed; the extension
picks the format), `--format=png|pdf|svg`, `--size=6x4in` (also `cm`, `mm`, `pt`; no unit
means inches; 1–40 in per side; or a journal size, below), `--dpi=300` (30–2400, PNG only),
`--theme=paper|light|dark|palette`, `--font=auto|"STIX Two"|"Latin Modern"|"Source Sans"`,
`--style=NAME` (below), and for several panels `--layout=ROWSxCOLS`, `--suptitle="…"` (a
title over all panels), `--letters` ((a), (b)… at each panel's top left), `--sharex` and
`--sharey`.

**Panel sizes:** `--widths=2,1` sets the relative widths of the panel columns and
`--heights=3,1` the relative heights of the rows (any positive numbers; `3,1` makes the
top row three times as tall). The count must match the layout's columns or rows; a wrong
count says how many there are.

**One legend for the figure:** `--legend=bottom|right|top` (or `off`, below) collects every panel's entries
into one legend outside the panels (a row underneath, a column on the right, or a row on
top) and draws none inside them. Entries with the same label and look appear once; an
entry drawn differently in different panels (a line in one, points in another) looks like
its first appearance.

**The default for several panels:** when the panels that would
show a legend all list the same entries, they get one legend on top of the figure instead
(as `--legend=top`, holding only those panels' entries), since a legend inside a small
panel covers data and one copy is enough. A single panel with a legend gets it above that
panel when the panel is in the top row; lower down it stays inside. Panels whose entries
differ each keep their own (a merged legend couldn't say which entry belongs where).
`--legend=off`, or `legend=` written on any panel, keeps every legend inside its panel;
figures with `residuals` strips keep them inside too.

**Shared axes:** `--sharex` gives every panel the same x range and shows x tick labels
and the x label only on the bottom panel of each column; `--sharey` does the same for y on
the left-hand panels. Panels in a row or column always line up their plot areas.

### Style files: `--style=NAME`

`--style=thesis` reads `~/.config/easyplots/styles/thesis.toml` (next to `config.toml`),
so one word sets a whole look. Every key is optional; the command wins over the file and
the file over `config.toml`:

```toml
theme      = "paper"            # paper | light | dark | palette
font       = "Latin Modern"
font_size  = 9
line_width = 1.0
grid       = false
legend     = "best"
size       = "3.5x2.6in"
dpi        = 600
format     = "pdf"
colors     = ["black", "red", "#1f77b4"]   # the series colours, in order
line_styles = ["solid", "dashed"]          # the series line styles, in order
markers    = ["o", "s"]                    # markers, for scatter or with `markers`
logx       = false
logy       = true
xstep      = 5                             # tick steps, as in xlim=::5
ystep      = 0.5
```

An unknown name lists the styles there; a misspelt key is an error.

**Styles and presets** are two different things. A *style* is a look with no data in it;
`⌘D` on a launcher preview writes one from the command (theme, font, size, colours, line
styles, markers, line width, legend, grid, log axes, tick steps — never files, columns,
labels or titles), refusing a name that already exists. A *preset* (`⌘⇧D`) is the whole
command, for files with the same columns (see the launcher).

### Editing a saved plot: `again`

`again PLOT` puts the command a saved plot was made from back in the launcher (`↵` in
command mode), ready to change and draw again. `again` on its own (↵), or `⌘O` anywhere in
the launcher, opens the **saved-plots browser**: the newest 60 plots EasyPlots saved in
the plots folder, as a grid of pictures (newest first, with how long ago). Arrow keys
move, typing filters by name or command, `↵` puts the plot's command back in the box,
`⌘↵` opens the file. Pictures are kept in the cache folder; PDFs are drawn again from
their command. `PLOT` is a path or a name in the output
folder; the launcher suggests recent plots. In the terminal, `easyplots again PLOT` prints
the command, and `easyplots again PLOT --theme=dark` draws it again with the options
added (an added option replaces the stored one of the same name; the same for
`preset NAME FILE more`). Works for PNG, PDF and SVG files made by EasyPlots.

### Themes and fonts

| Theme | Look | Font (`auto`) |
|---|---|---|
| `paper` | journal: black on white, full frame, inward ticks on all sides, minor ticks, no grid | STIX Two |
| `light` (default) | screen and slides: white, left and bottom axes only, outward ticks, light grid, lines 20 % thicker | Source Sans 3 |
| `dark` | as `light` on a dark grey background, with a brighter blue and light grey instead of black | Source Sans 3 |
| `palette` | as `light`/`dark`, in the colours of the current Palette theme (§1b), light or dark: background, foreground, grid in `selection`, series in blue, orange, green, red, magenta, cyan, yellow. Without Palette's colour file, a built-in dark theme | Source Sans 3 |

`--font` (or `style.font`) overrides the theme's font: STIX Two (Times-like), Latin Modern
(the LaTeX look) or Source Sans 3 (clean sans-serif). All are bundled, so plots look the
same on every machine. `grid=on|off` on a plot always wins; `style.grid` sets the paper
default.

### Look (the `paper` theme)

STIX Two Text at 10 pt (ticks 9 pt, title 11 pt), a frame with inward major and minor ticks
on all four sides, no grid, the Okabe–Ito colour-blind-safe palette, lines 1.25 pt. Numbers
use a real minus sign (−). Very large or small numbers get a shared `×10ⁿ` above the y axis
or at the right of the x axis. Date axes step by seconds up to years and show the shared
part (e.g. `2026`) once at the right. Line plots fit the x data exactly; scatter and markers
get 5% space at the sides; bars start at zero.

### Maths in labels

Text between `$…$` in a title, axis label or legend label is set as maths, using a small
subset of LaTeX: superscripts and subscripts (`x^2`, `T_{max}`, one level deeper:
`e^{x_1}`), Greek letters (`\alpha`, `\Delta`; lower-case italic as in LaTeX), symbols
(`\pm`, `\times`, `\cdot`, `\degree`, `\infty`, `\leq`, `\geq`, `\neq`, `\approx`, `\sim`,
`\propto`, `\partial`, `\nabla`, `\to`, `\ell`, `\hbar`, `\AA`…), `\sqrt{}` (written √),
`\frac{a}{b}` (written a/b), `\mathrm{}` / `\text{}` for upright text, and the spaces `\,`
`\;` `\quad`. Single letters are italic; `-` becomes a real minus; `=` `<` `≈`… get space
around them and `+` `−` `×`… a little, as in LaTeX. `\$` is a plain dollar sign, and a
lone `$` stays a dollar sign.

A character the chosen font lacks (Latin Modern has no Greek) comes from STIX Two, and
maths symbols from a small bundled cut of STIX Two Math. Full LaTeX is not
supported.

### Errors

Every error says where in the command it is and suggests a fix. The terminal shows:

```
easyplots: no column "tmie" in simple.csv — did you mean "time"?
  line simple.csv x=tmie y=temp
                    ^^^^
```

A misspelt word, option, colour, colour map or style suggests the closest correct one.

## 1b. The launcher

Press the hotkey (`general.hotkey`, default `Ctrl+Option+P`) from any app. The launcher
opens on the display with the mouse pointer, without taking the app you were in out of
front (on macOS it is a non-activating panel, like Spotlight's). It sits centred across
and a little above the middle: its top edge is where a centred preview's would be, so a
preview opens centred on the screen and the command line doesn't move. Press the hotkey again,
press `Esc`, or click elsewhere to close. Clicking elsewhere or the hotkey keeps the draft:
opened again within 10 minutes, the launcher is where you left it (a preview comes back as
its command). `Esc`, saving and opening a plot start fresh next time. It opens in picker
mode, or in command mode if `launcher.mode = "command"`.

**Picker mode** builds the command step by step and shows it above the prompt as it grows:

| Step | List shows | Keys |
|---|---|---|
| file | CSV/TSV, workbook (one entry per sheet) and JSON files under `data_dir` (fuzzy search). With an empty box: the files of your latest commands first, then the newest; JSON files last (which ones: `general.json_files`). Folders holding two or more data files are listed as `runs/` (a grid, §1). First of all, **Clipboard** when the clipboard holds a table | `↵` choose; the clipboard's table is saved as a file in `paste_dir` first |
| type | all plot types, and at the end your presets (`preset: NAME`) | `↵` choose; a preset fills in its command with the file just picked |
| columns | only the steps the type needs (below), each listing the fitting columns with an icon for their kind and what's in them (`12.1 – 31.4 · mean 22 · 3 blanks`, or the text values) | `↵` choose; in y, `⇥` ticks several (`↵` alone takes the highlighted one) |
| style | only if you have saved styles: `(no style)` and each style | `↵` choose; adds `--style=NAME` |

| Type | Column steps |
|---|---|
| `line` `scatter` `bar` `step` `area` `polar` `fft` | x (or `(row number)`), y |
| `hist` `kde` | x (the values to count; `⇥` ticks several) |
| `box` `violin` | y, then a group (a text column, or `(no groups)`; asked only if the file has text columns) |
| `heatmap` `contour` | x (or `(whole table)`, which ends here), y (one), z |

After the last step the finished command moves into the box (command mode), ready for
options or `↵`.

**Pasting a table:** copy cells (with their header row) in Excel, Numbers or a web page,
open the launcher: the file step starts with "Clipboard (12 rows × 3 columns)". Choosing it
saves the table as `pasted-YYYY-MM-DD-HHMMSS.tsv` in the pasted-data folder
(`output.paste_dir`, Settings → Saving; never overwritten), so the plot's command names a
real file and `again` works later.

**Pinning and dropping files** (`⌘P`): pins the launcher so it stays open when another
app is clicked (a pin shows next to the box; `⌘P` again or `Esc` unpins, and closing
always unpins). Drag one or more CSV, workbook or JSON files from Finder onto it (a dashed
outline shows while dragging): it starts afresh with the first file picked (next step:
the plot type); the others are overlaid with the same columns (`line a.csv x=t y=temp +
b.csv x=t y=temp`). This replaced dropping on the menu-bar icon, which macOS didn't pass
on reliably.

**Junk above the header** (an instrument's name, a date) is found by itself: lines that
don't split into as many fields as the rest of the file are left out, as long as the
line after them looks like a header (not numbers). `skiplines=N` still sets it by hand, and
the launcher's column list and suggestions read the file with that layer's `skiplines=` and
`sheet=`.

**File rows** show the file name first and its folder after it in grey; a long folder is
shortened from its start (`…/thesis/raw data/`), so the name always shows.

**Peek at rows:** beside the file list, the highlighted file's first 8 rows (header and
cells, numbers shortened), so you can tell similar files apart before picking. Files in
the data folder are read for this; files found by whole-disk search are not opened until
picked (their peek stays empty).

When nothing matches what's typed, the closest spellings are listed as "did you mean?".
`⌫` in an empty box goes back a step (forgetting that step's choice). Typing a plot type and a space (`line `) in the file
step switches to command mode. `↑` and `↓` only move through the list; recent commands
are listed in command mode (`⌘E`) while its box is empty.

**Command mode** takes the language above, coloured as you type (plot type, file, option
names). A long command folds onto up to three lines in the box, then scrolls inside it
(a pasted line break becomes a space). The list suggests what fits the word under the cursor: plot types, then files, then
option names (only those that do something for the plot type being typed, so `hist`
offers `bins=` but not `marker=`; any option can still be typed), columns inside `x=`/`y=`/`err=`, and option values (colours with swatches,
style files after `--style=`, saved plots after `again`, presets after `preset`, and
"Pick a colour…" after `color=`, which opens the macOS colour panel). After a whole column name in
`y=` or `err=`, `name, …` adds a comma for another column. After `peaks=`, `dips=` or
`onset=` the panel's curves are offered; after `cross=`, `curves` (a level is typed as a
number). An option the plot type doesn't use says so (`err= isn't used by kde`) instead of
listing values, and an option that clashes with one already written (`marker=` after
`markers`, `legend=` with `--legend=`) isn't offered. The highlighted row folds a long
description onto more lines. Typing an option (`c=`, `title=`) during the picker's column
steps carries on in command mode with the command built so far. `⇥` puts
the highlighted suggestion in. With an empty box the list shows recent commands. An error is
shown under the box, the border turns red, and the problem text is highlighted and selected.
`⌘E` switches between picker and command mode. `⌘Z` / `⌘⇧Z` undo and redo: typing (a burst
within a second counts as one step) and picker steps alike.

`again PLOT` (↵) puts a saved plot's command back in the box; `preset NAME FILE` (↵) draws
a preset with another file. With `launcher.live_preview = true`, a small drawing of the
command appears beside the list after a short pause in typing, and in the picker a drawing
of the highlighted choice (each plot type in the type step, each column in the column
steps). An unfinished command keeps the last good drawing, faded; errors only show on ↵.
`⇥` on a long command scrolls the box so the inserted text stays in view.

**Preview and saving.** `↵` draws the plot in a larger window (`launcher.preview_size`)
without saving anything. Then:

| Key | Does |
|---|---|
| `↵` or `⌘S` | save in the output folder in the `output.format` format (the hint says which), named by `output.name`, never overwriting |
| `⌘1` `⌘2` `⌘3` | save this plot as PNG, PDF or SVG, whatever `output.format` says (after saving: another copy in that format) |
| `⌘⇧S` | Save As…: the macOS Save dialog; the extension picks PNG, PDF or SVG |
| `⌘C` | copy the image at `output.dpi`: a PNG, or a TIFF with `output.copy_format = "tiff"` |
| `⌘⇧C` | copy the numbers actually drawn (after `where=`, calculations, `stat=`, baselines, fits) as tab-separated text, one row per point (panel, series, x, y; `err` when there are error bars, `fit` with the fitted value at that x when the series has a fit, `z` for heatmaps); names are plain text and numbers have at most 6 significant figures; a box or violin gives its median and quartiles |
| `⌘R` | draw again, reading the file afresh (after you changed it in another app); nothing watches files |
| `⇥` | apply the plot advisor's hint (below), when it offers a fix; otherwise nothing |
| `⌘D` | keep the look as a style (`styles/NAME.toml`): type a one-word name, `↵` |
| `⌘⇧D` | keep the whole command as a preset: type a one-word name, `↵` |
| typing | edit the command again; `↵` draws again |

**Plot advisor** (`launcher.advice`, on by default): one dim line under the preview when
the plot has a known pitfall, the most useful first:

| Noticed | Hint suggests | `⇥` adds |
|---|---|---|
| the legend sits on 3 or more pieces of data (points, bits of line, bars) | `legend=out` | `legend=out` |
| one curve's range is under 2 % of its axis's (it looks flat beside another) | `axis=right` | with exactly two curves: `axis=right` on the second file, or `y=a,b` → `y=a + y=b axis=right` |
| all y (or x) values above zero and spanning 4 or more decades, no limits set | `logy` (`logx`) | `logy` (`logx`) |
| bars or a histogram with `ylim=` above (or below) zero, or `logy` | drop it, or points (`strip`) | nothing: hint only |
| the tallest bar (with its error bar) above `ylim=`'s top | a higher top | `ylim=LOW:TOP`, TOP a round number above the bar |
| a `cross=` level that no curve of the panel reaches (e.g. after `norm=`) | a level inside the curves | `cross=MID`, the curves' middle to two significant figures |
| `stat=` error bars resting on only 2 values | read with care, `err=none` | nothing: hint only |
| more curves than palette colours, and some drawn exactly alike | panels with `;` | nothing: hint only |

In a figure of several panels the hint names the panel (`panel 2: …`); `⇥` is offered
only for the last panel, where added words land. The fix is plain text in the box, so `⌘Z`
gives back the command as it was. Hints are never part of the saved file; the terminal
command prints them on stderr (`hint: …`). No Try-it lesson sets one off (a test checks).

After saving, `output.after_save` decides what happens: `thumbnail` keeps the launcher open
with a green border and the path (`↵` opens the file, `⌘↵` shows it in Finder, `⌘⇧S` saves
another copy); `notify` closes and shows a notification; `open` closes and opens the file;
`reveal` closes and shows the file selected in Finder; `none` just closes.

**Look.** Colour names are Palette's (`background`, `foreground`, `accent`, `selection`,
`red` … `magenta`). `launcher.theme`:

- `"palette"` (default): Palette's current theme, from `~/.config/palette/current/easyplots.json`,
  or until Palette writes that, `habit-tracker.json` beside it (same colours). Rechecked every
  2 s while the launcher is open. If neither exists, as `"auto"`.
- `"auto"`: Catppuccin Mocha in dark mode, Rosé Pine Dawn in light mode.
- a built-in name: `catppuccin-mocha`, `rose-pine-dawn`, `gruvbox-dark`, `tokyo-night`,
  `everforest`, `kanagawa`.

`opacity` (0–1, no lower limit; Settings warns below 0.4) and `blur` (0 off to 100, the
blur radius macOS gives Ghostty; `true`/`false` from older files read as 20/0) set the
glass; `font` (JetBrains Mono is bundled, any installed font works) and `font_size` the
text; `hints` the key hints; `size` and `preview_size` the window.

**Palette's glass:** when `easyplots.json` in Palette's folder has a `glass` block
(`palette glass easyplots`: `{"kind": "blurred", "opacity": 0.7, "blur": 40}`), it replaces
`opacity` and `blur` whatever the launcher's colour setting is: `solid` is opacity 1 and no
blur, `clear` Palette's opacity with no blur, `blurred` both. Kind `own` or no block hands
them back to config.toml. Palette's values are used as they are (no cap); Settings →
Launcher shows a banner, dims the two controls, and warns below 0.4 as for your own value.
The launcher picks a change up when it opens and within 2 s while open. If macOS lacks the
private blur call, the launcher falls back to Apple's frosted popover material and says so
in its warnings.

**Presets** are commands kept under a name for reuse with another file ("same plot,
different file"), for files with the same columns. Keep one with `⌘⇧D` in the preview; use it by picking a file and then the
preset in the type step, by typing `preset NAME FILE`, or in Terminal with
`easyplots preset NAME FILE`. Its first file (and any later layer using that same file) is
replaced; other files stay. They're stored in `history.toml` and listed (and deleted) in
Settings.

## 1d. The Settings window

Menu-bar icon → **Settings…** (or opening EasyPlots.app again). The app shows a Dock icon
while it's open. Tabs: **General** (hotkey, data folder, whole-disk search, launch at
login), **Saving** (plots folder, format, file name, dpi, size, after saving), **Plot
look** (theme, font, font size, line width, grid, legend), **Launcher** (mode, colours,
opacity, blur, font (every installed font, monospaced first), size, hints, live preview,
history, window sizes, plot advisor), **Presets & styles**, **Try it** and **Help**.

- **Try it**: short lessons in groups (first plots, overlays and groups, fits and checking
  them, reading numbers off curves, panels…), each a sentence, its command and the picture
  the engine draws for it, using the sample data built into the app (`demo/categorical.csv`,
  `demo/peak_onset.csv`, `demo/trial.csv`, `demo/spectrum.csv`, `demo/run_a.csv`,
  `demo/run_b.csv`, `demo/surface.csv`, `demo/outlier.csv`: made-up data, read-only,
  nothing written to disk). These names work in the launcher and the terminal too. **Open
  in launcher** puts a lesson's command in the launcher's box and draws it, in one click. Every lesson is drawn in
  `cargo test`, so the guide can't go stale.
- **Help**: `docs/FEATURES.md` with a search box, and the command map as a picture.

Every change is written into `config.toml` at once, keeping its comments and layout, and
applies straight away (the launcher repaints, a new hotkey works). A value that would make
the file invalid is refused with the reason under the option, and nothing is written.
The hotkey box records keys: click it and press the combination (it needs Ctrl, Option or
Cmd). **Open config.toml** opens the file in a text editor; edits there show up in the
form within a second.

- **Search the whole disk** adds every CSV/TSV in the Spotlight index to the file list
  (names only; files are opened only when you choose them). Hidden folders and Library,
  node_modules and similar are skipped.
- **Launch at login** adds or removes EasyPlots as a login item (a Launch Agent in
  `~/Library/LaunchAgents`).

## 1c. The terminal command

The tray menu's **Install Terminal Command…** links `/usr/local/bin/easyplots` to the program
inside EasyPlots.app (asking for the password if that folder isn't writable). Then
`easyplots …` works in any Terminal window and is updated with the app. It takes the same
commands, plus `--columns FILE`, `--stats FILE`, `--svg`, `--time`, `again PLOT`,
`preset` (list), `preset NAME FILE`, `--version` and `--help`. Files are
looked up in Terminal's current folder first, then in `data_dir`.

`--each "PATTERN" TYPE options…` saves one plot per matching file, each file put after
the plot type: `easyplots --each "runs/*.csv" line x=t y=temp` saves `line runs/a.csv
x=t y=temp`, `line runs/b.csv x=t y=temp`… and prints each path. The plot advisor's hints
go to stderr as `hint: …` (stdout stays the paths and fit numbers, for scripts).

## 2. `config.toml`

Location: `~/.config/easyplots/config.toml`, or the path in the `EASYPLOTS_CONFIG`
environment variable. Anything missing falls back to the defaults below. The app creates
the file with comments on first run; the terminal command only reads it. The app checks
for changes (macOS tells it when the file is saved; nothing runs in between) and applies
hand edits at once: a new hotkey, the launcher's look
(theme, opacity, blur, font) and plot settings. Problems such as a
bad value or a hotkey another app already uses are shown at the bottom of the launcher.
Command history and presets are kept next to it in `history.toml`. Unknown keys are reported
as errors, so typos don't go unnoticed.

```toml
[general]
hotkey       = "Ctrl+Option+P"
data_dir     = "~/Documents"        # where the file picker looks first
search_whole_disk = false           # also list CSVs from everywhere (Spotlight; names only)
launch_at_login   = true            # start EasyPlots when you log in
json_files   = "all"                # JSON files in the list: all | tables (only those with rows) | known (not package.json etc.)

[output]
dir      = "~/Plots"
paste_dir = "~/Plots/pasted"        # where tables pasted from the clipboard are kept
format   = "png"                    # png | pdf | svg
copy_format = "png"                 # what ⌘C copies: png | tiff
name     = "{file}_{type}_{date}-{time}"
dpi      = 300                      # PNG only
size     = "6x4in"
after_save = "thumbnail"            # thumbnail | notify | open | reveal (in Finder) | none

[style]
theme    = "light"                  # light (screen, slides) | paper (journal) | dark | palette
font     = "auto"                   # auto (the theme's own) | STIX Two | Latin Modern | Source Sans
font_size = 10                      # points
palette  = "okabe-ito"              # colour-blind-safe default
line_width = 1.25                   # points
grid     = false
legend   = "best"

[launcher]
mode     = "picker"                 # picker | command: which mode opens first
history_size = 200
theme    = "palette"                # palette | auto | a built-in name (§1b)
opacity  = 0.9                      # background opacity, 0–1 (below 0.4 text gets hard to read)
blur     = 20                       # blur what's behind the window, 0 (off) to 100
font     = "JetBrains Mono"         # any installed font; JetBrains Mono is bundled
font_size = 15
hints    = true                     # key hints at the bottom
size     = "640x420"                # window size
preview_size = "900x680"            # window size while showing a plot
live_preview = false                # draw a small preview while typing a command
advice   = true                     # a hint under the preview when a plot has a known pitfall
```

## 3. Output file names

The `name` template can use these tokens:

| Token | Becomes |
|---|---|
| `{file}` | first CSV's name without `.csv` (`data`) |
| `{files}` | all CSV names joined by `+` |
| `{type}` | plot type(s), e.g. `line` or `line+hist` |
| `{y}` | y column names joined by `-` |
| `{date}` / `{time}` | `2026-10-21` / `143005` |
| `{n}` | number of files already in the output folder, plus one |

Unsafe characters (`/ \ : * ? " < > |`) are replaced with `_`; names are cut at 150
characters. If the file already exists, `-2`, `-3`… is added; nothing is ever overwritten.
`--out=` on a command overrides the template for that plot. It must stay inside the output
folder (no `/…`, `~/…` or `..`); to save elsewhere, change `output.dir`.

Every plot records the command that made it: in the PNG's text metadata
(`EasyPlots command`), in the PDF's *Subject*, and in the SVG's `<desc>`.
