# Architecture

How EasyPlots is put together: the plotting engine, the terminal command, the macOS app
and its two windows. Technical words are explained in the [Glossary](#glossary) at the
end.

## The big picture

EasyPlots is **one program** with three ways to use it:

```
            ┌───────────────────────────── EasyPlots.app (one process) ─────────────────────────────┐
 hotkey ──▶ │  Launcher window  ──┐                                                                   │
            │  (search bar, TS)   │   Tauri commands    ┌──────────── Rust side ──────────────┐       │
 click  ──▶ │  Tray menu ─────────┼──────────────────▶  │ app/src-tauri: tray, hotkey,        │       │
 menu bar   │  Settings window ───┘   (messages)        │ windows, file search, config watch  │       │
            │  (Dock icon, TS)                          │            │ calls                   │       │
            │                                           │            ▼                         │       │
            │                                           │ crates/core: parse → load → draw →   │──▶ PNG/PDF/SVG
            │                                           │ export                               │   in output folder
            │                                           └──────────────────────────────────────┘       │
            └────────────────────────────────────────────────────────────────────────────────────────┘
 terminal ──▶ easyplots (crates/cli) ─── calls the same crates/core ──────────────────────────────────▶ same files
```

- The **windows** (launcher, settings) are small web pages written in TypeScript. They only
  draw the interface and send requests.
- The **Rust side** does all the real work: finding files, reading CSVs, drawing plots,
  writing files.
- **`crates/core`** is the plotting engine. It knows nothing about windows, so it can be
  tested on its own and reused by the terminal command.

## Layers and modules

### `crates/core` (easyplots-core), the engine

The pipeline, entered through `Engine::render` / `Engine::plot` in `crates/core/src/lib.rs`:

```
command text ─ command::parse ─▶ PlotSpec ─ figure::build ─▶ Figure ─ render ─▶ SVG ─ export ─▶ bytes ─ naming ─▶ file
                                 (names as typed) (+ CSV data)  (numbers to draw)
```

| Module | Job |
|---|---|
| `lib.rs` | `Engine`: holds the config and a cache of loaded CSVs; `render` (no saving), `plot` (save), `columns` (list a file's columns), `recall` (`again`). Applies a `--style` file between config and command. Records how long each step took |
| `command` | Turns a command string (see [SPEC](SPEC.md)) into a `PlotSpec`. Two steps: split into words respecting quotes, then build panels/layers. Every error points at the word it's about and suggests fixes; an option is written once per file, panel or figure, and options that set one thing two ways are refused together |
| `spec` | `PlotSpec` and its parts (plot kinds, colours, markers, line styles, legend positions). Still refers to files and columns by name. Both input modes end here |
| `history` | Recent commands and presets in `history.toml`; `with_file` puts another file into a preset's command |
| `figure` (`mod.rs`, `columns.rs`, `univariate.rs`, `multivariate.rs`, `fitting.rs`) | Finds the files (a pattern such as `run_*.csv` gives one layer per file; a folder, one panel per file, or with `stat=` one layer per file) (current folder, then `data_dir`), loads them through `DataCache` (reloads only when a file changes), splits `by=` layers into one layer per group (`split_groups`, before `stat=` summaries, which then make one series per group), picks or calculates the columns, applies defaults (columns, labels, colours, markers) and does the counting some types need: histogram bins, box-plot quartiles, violin and kde densities, heatmap/contour grids (from a table or x/y/z
columns), stacking, fitted curves, `slope`/`integral` and the marks from `analysis`. Produces a `Figure` of numbers, with the panel grid |
| `advice` | The plot advisor: checks on the built and drawn figure (legend over data, a flat curve beside another, many decades on a linear axis, bars cut by `ylim=`, an unreachable `cross=` level, error bars too short to see…), each with an optional fixed command that ⇥ applies in the preview |
| `sheets` / `paste` | Excel and LibreOffice workbooks (`calamine`, `sheet=`) and JSON files as tables; a table copied to the clipboard recognised and saved in the pasted-data folder |
| `spectrum` | `fft`: evenly spaced samples, a Hann window and a radix-2 transform of our own |
| `guide` / `demo` | The Try it lessons in Settings, and the sample files built into the app (`demo/…`, read from `tests/fixtures` at build time) |
| `expr` | Calculated columns (`power/area`, `log10(counts)`): a small parser for `+ − × ÷ ^`, brackets and functions, evaluated row by row on a table |
| `colormap` | Colour maps for heatmaps and contours (`viridis`, `magma`, `coolwarm`…) and black-or-white text on a colour |
| `settings` | The Settings form's option list (label, hint, kind of control) and `set`, which changes one value in `config.toml`'s text keeping comments (`toml_edit`) and refuses anything that wouldn't load |
| `fit` | Fitted curves (`fit=`): polynomials, exp/log/power and your own formula (`fit="a*exp(-x/b)+c"`, solved numerically), true least squares with standard errors, the 95% band, weights, `fitrange=`, R², and the equation as a maths label |
| `analysis` | Numbers read off a curve: `slope`, `integral`, `area=`, `peaks`/`dips` (by prominence) and the two-tangent `onset`. Plain maths on x/y lists; `figure::read_curves` turns the results into `Mark`s (rings, dashed tangents, a shaded area) and Terminal lines |
| `styles` | Style files (`--style=NAME`, `styles/NAME.toml` next to the config): read, check, and lay over the config |
| `recall` | `again`: reads the stored command back out of a PNG, PDF or SVG, and lists saved plots for suggestions |
| `data` | Reads CSVs: separator and header detection, comment lines, missing values, decimal commas, number/date/text columns (rules in SPEC §1) |
| `style` | Plot themes (`paper`, `light`, `dark`, `palette`): font, sizes in points, tick lengths and direction, frame, grid, colours, the Okabe–Ito palette |
| `filter` | Row filters (`where="temp > 20 and site == 'north'"`): parses the condition and marks which rows of a table match; the left side can be a calculation (`expr`) |
| `render/mod.rs` (with `axes.rs`, `legend.rs`, `marks.rs`, `univariate.rs`, `multivariate.rs`) | Writes the SVG directly (no plotting library). Splits the figure into a grid of panels; measures each panel's labels to size its margins (`measure`), lines up plot areas across rows and columns, then draws each (`draw_panel`): background, grid, data (clipped: areas, bars, histograms, boxes, then lines, steps, error bars and markers), frame or axes, ticks (in or out), labels, offsets, a second y axis, title, panel letter and legend. Linear and log scales; shared axes. Very long lines are thinned to what the page can show; overlapping markers are skipped |
| `render/groups.rs` | Grouped data: `strip` dots spread as a beeswarm (each dot as near its group's middle as it can go without touching one already placed, from the lowest value up; over 1500 in a group, or with `jitter`, a fixed-seed random spread), `mean` and `median` lines and their labels, boxes in their own colours (`highlight=`), and the **sideways view** for `horizontal`: the panel's data stays upright and is drawn through a mirror across the diagonal (`matrix(0 1 1 0 0 0)` swaps x and y), so bars, boxes, violins and error bars need no second drawing code; text and markers are drawn after the turn so they aren't mirrored. A second panel (`Sideways::axes`) gives the page its axes: values along the bottom, group names down the side, the first at the top |
| `render/ticks.rs` | Where ticks go: "nice" number steps, minor ticks, `×10ⁿ` factors, log decades, calendar-aware date ticks, category labels |
| `render/text.rs` | The bundled plot fonts (STIX Two, Latin Modern, Source Sans 3, and a STIX Two Math cut for symbols; embedded in the program), which font has which character, and text measurement |
| `render/mathtext.rs` | Maths in labels (`$\alpha_1^2$`): turns it into runs of text with a size, style and raise, measures them, and writes them as `<tspan>`s |
| `export` | SVG → PNG (`resvg`, with DPI and the command stored) or PDF (`svg2pdf` + `pdf-writer`, fonts embedded, command in the document info). Only the bundled fonts are used, so output is the same on every machine |
| `naming` | Output names from the template, `--out`, safe characters, and `-2`, `-3`… instead of overwriting |
| `config` | Reads `config.toml`; defaults for anything missing; unknown keys are errors; writes the commented default file (`DEFAULT_TOML`) on first run |
| `search` | The list of CSV/TSV files under `data_dir` (skips hidden folders, `Library`, `node_modules`…; at most 8 levels, 200k entries), Spotlight's list for whole-disk search (`mdfind`, names only), and fuzzy matching (`nucleo-matcher`) |
| `complete` | Command-mode suggestions: what fits the word under the cursor (options of this plot type, column names read from the file, values, curve names, a typed pattern with its folders), and which text it replaces |
| `ui_theme` | Launcher colours by Palette's names: six built-in themes, `auto`, and reading Palette's colour file (`easyplots.json`, else `habit-tracker.json`; only `#rrggbb` accepted). Also the launcher's `Look` (opacity, blur): Palette's `glass` block in `easyplots.json` when it sets one, else `[launcher]`; never capped, `faint` below 0.4 for the Settings warning |
| `error` | One error type with an optional position in the command, and the "did you mean" helper |

Tests: unit tests sit at the bottom of each module. `crates/core/tests/golden.rs` renders
example commands from `tests/fixtures/` and compares them with the stored SVGs in
`crates/core/tests/golden/`, so any change in the look is deliberate. The same run can
write the PNGs in `docs/gallery/`.

### `crates/cli` (`easyplots`), the terminal command

A library (`run(args) -> exit code`) plus a tiny `main.rs`. The app links it in, so the app
program itself acts as the terminal command when started as `easyplots`.


`easyplots line data.csv x=time y=temp` does exactly what the launcher does with the same
text, and prints where the plot was saved (and the advisor's hints on stderr). Also
`--columns FILE` (list columns and their types), `--svg` (print the SVG instead of
saving), `--time` (show how long each step took), `--each "runs/*.csv" …` (one plot per
file), `again PLOT`, `preset NAME FILE` and `--complete "<text>"` (the launcher's
suggestions as JSON). It's useful for scripts and for testing the engine without the GUI.
Releases carry it as `easyplots-cli-macos-arm64.tar.gz`.

### `app/src-tauri`, the macOS app (Rust)

| Module | Job |
|---|---|
| `main` | If started as `easyplots`, run the terminal command and exit. Otherwise start the app with no Dock icon (*accessory* mode), the hotkey and notification plugins, the tray and the shared state |
| `state` | `AppState`: the `Engine` (config + CSV cache), the file list, history, warnings, current hotkey. Creates `config.toml` on first run and reloads it when it changes |
| `hotkey` | Registers `general.hotkey` (re-registers when it changes); a failure becomes a launcher warning |
| `commands` | What the launcher may call: `launcher_info` (incl. look, theme, save format, presets), `current_theme` (polled for Palette switches), `search_files` (with "did you mean" guesses), `list_columns`, `complete_command`, `preview` (draws, keeps the drawing, enlarges the window), `preview_thumb` (live preview), `end_preview`, `save_preview` (optionally in another format: ⌘1–3), `save_preview_as` (Save dialog), `copy_preview` (clipboard: the PNG file's bytes through AppKit's pasteboard, or with `copy_format = "tiff"` the plugin's raw picture), `open_plot`, `recall_plot` (`again`), `save_preset`/`apply_preset`, `hold_open` (colour panel). Converts text positions to JavaScript's units (UTF-16). Rescans the data folder in the background (at most every 30 s), adding Spotlight's list when whole-disk search is on |
| `settings` | What the Settings window may call: `settings_info` (the option list from core and the current values), `set_setting` (writes one value into `config.toml` via core, then applies it), `pick_folder`, `open_config_file`, `delete_preset`. Applies what the app owns: hotkey, blur, login item (`tauri-plugin-autostart`) |
| `tray` | Menu-bar icon. **Click** opens a menu: *Open Launcher*, *Settings…*, *Install Terminal Command…*, *Quit*. No hover behaviour |
| `install` | Links `/usr/local/bin/easyplots` to the app's own program (password prompt via `osascript` if needed) |
| `windows` | Watches the config folder (`notify`, which uses macOS FSEvents: the system reports a saved `config.toml`; nothing runs in between) and applies hand edits at once. Launcher: borderless, transparent with rounded corners and an optional blur, sized from config (larger while previewing); opens on the display with the mouse pointer, centred across, its top edge kept when a preview makes it taller (`place_in`); floats above other windows; shown with the hotkey (which also refreshes config and files), hidden on `Esc`, the hotkey again, or focus loss (not while pinned with `⌘P`). Files dragged onto the launcher (Tauri's `DragDrop` window event) start it afresh with them picked (`files_dragged`). Settings: a normal window; while open, the app switches to *regular* mode so it gets a Dock icon |
| `glass` | The launcher's blur strength through macOS's private `CGSSetWindowBackgroundBlurRadius` (as Ghostty and Dispatch), looked up at run time with `dlsym` so a macOS without it still starts; `windows::apply_blur` falls back to the popover material then. Re-applied when the look changes (on show, and from the `current_theme` poll) |
| `panel` (macOS) | Turns the launcher into a non-activating panel (`tauri-nspanel`) at start-up, so it takes the keyboard without making EasyPlots the active app (window managers like AeroSpace then leave workspaces and Settings alone). Places it with AppKit (`objc2-app-kit`) on the screen under the mouse. Makes the app active only for the Save dialog and the colour panel, and hides it again afterwards so the previous app gets the keyboard back |

### `app/src`, the windows (TypeScript)

- `launcher/main.ts`: a prompt line, a list and a footer, in four modes: picker (file →
  type → only the column steps that type needs, `stepsFor`), command (coloured text over a
  transparent input, suggestions from Rust), preview and saved, plus naming a preset (⌘D)
  and the live preview beside the list (of the typed command, or of the picker's
  highlighted choice). Clicking away keeps the draft for 10 minutes; `Esc`, saving and
  opening a plot start fresh (`close`). A `session` counter drops answers that arrive after
  a fresh start (a slow save). The command box is a `<textarea>` with invisible text over
  a coloured copy (`#mirror`); after every change the box is made to lay its text out
  afresh (`refold`: WebKit doesn't always fold a line back up when a word gets shorter)
  and the copy checks it folds in the same places. Colours are CSS variables named after Palette's keys, filled from the
  theme Rust sends. `launcher/api.ts` holds typed wrappers for the Rust commands. Keys are
  listed in SPEC §1b.
- `app/tests/launcher-ui.mjs`: drives the built launcher in Chromium with stand-in Rust
  answers (`npm --prefix app run test:ui`). `app/tests/e2e-linux.sh`: runs the real app in a
  virtual Linux screen, presses the hotkey and plots, for testing without a Mac.
  `app/tests/check-macos.sh`: type-checks and lints the macOS-only Rust from Linux.
- `settings/`: tabs of options drawn from the list the Rust side sends (text, numbers with
  sliders, switches, menus, folder pickers, a "press the new hotkey" recorder) and the
  presets. Each change is sent at once; errors appear under the option. It takes the
  launcher's colours (`current_theme`, Palette's theme or a built-in one) and follows a
  Palette switch while open. `help.ts` draws the Help tab from `docs/FEATURES.md` with a
  search box; `commandmap.ts` draws the command map in it three ways (cards, tree,
  terminal); `tryit.ts` is the Try it tab (lessons with pictures and "Open in launcher").
  `app/tests/settings-ui.mjs` tests it in Chromium against the engine's real option list
  and checks every option on the map against the engine's suggestions.

## Data on disk

| What | Where | Notes |
|---|---|---|
| Config | `~/.config/easyplots/config.toml` | Created with defaults on first run. Hand edits are picked up automatically |
| History and presets | `~/.config/easyplots/history.toml` | Plain command strings, newest first |
| Cache | `~/Library/Caches/EasyPlots/` | File-search index and thumbnails; safe to delete |
| Plots | Output folder from config (default `~/Plots`) | Never overwritten; a `-2`, `-3`… suffix is added instead |
| Style files | `~/.config/easyplots/styles/NAME.toml` | Written with ⌘D on a preview (or by hand); read by `--style=NAME` |
| Pasted tables | `output.paste_dir` (default `~/Plots/pasted`) | A table copied to the clipboard, kept as a file so its plots can be redrawn |

Each saved plot also stores the command that made it (in PNG text metadata, the PDF
document info and the SVG `<desc>`). `again PLOT` reads it back to edit and redraw.

## Main flows

**Plotting from the launcher**
1. The hotkey fires. The Rust side reloads `config.toml` if it changed (a background thread
   is told by macOS when the file is saved and applies the hotkey, blur and look at once,
   telling the window with `config-changed`), starts a background rescan of the data folder
   if the list is older than 30 s, and shows the launcher (already loaded, so it appears at
   once) on the display with the mouse. On `launcher-shown` the window starts fresh or
   carries on with the draft.
2. Typing calls `search_files` (picker) or `complete_command` (command mode).
3. Picking a file calls `list_columns`, which loads the CSV into the cache, so the plot
   that follows doesn't read it again.
4. Return calls `preview(command)`: core parses, loads (or reuses) and draws; the drawing is
   kept in the app's state, a PNG goes to the window, and the window grows. Nothing is saved.
5. Return or ⌘S calls `save_preview` (output folder), ⌘⇧S `save_preview_as` (Save dialog),
   ⌘C `copy_preview`. These reuse the kept drawing. Then `after_save` applies.

**Changing settings**
Settings sends `set_setting(section, key, value)`. Core's `settings::set` changes that one
value in the file's text with `toml_edit` (comments and layout kept) and checks that the
whole file still loads; the app writes it (temporary file, then rename), reloads the
config, re-registers the hotkey, sets blur and the login item, and sends `config-changed`
so both windows redraw. Hand edits take the same path through the file watcher.

## Weak spots to watch

- **Mac-only code is type-checked, not run, on Linux.** `app/tests/check-macos.sh` lints
  it for `aarch64-apple-darwin` with a stand-in C compiler (nothing is linked); the CI
  macOS job builds it, and only a real Mac runs it.
- **The launcher panel is a class swap.** `tauri-nspanel` changes the launcher's window
  class to an `NSPanel` subclass at start-up, behind Tauri's back. Tauri's own window
  calls (hide, focus events) keep working, but a Tauri update that changes how windows
  are created is the first suspect if the launcher misbehaves after one.
- **Notifications can't be clicked to open the plot** (the Tauri plugin has no click
  actions on desktop). `after_save = "open"` or the thumbnail cover that.
- **Window managers** (AeroSpace, yabai, Rectangle) watch windows through the
  Accessibility API and may still try to manage the panel; if one does, a floating rule
  for `com.praddy.easyplots` in its config is the fix.
- **Returning focus** to the previous app: the panel never activates the app, so focus
  stays; only after the Save dialog or the colour panel is the app hidden to hand it back.
- **Hotkey clashes** with other apps: registration can fail. Settings must show the error.
- **Maths in labels:** SVG text has no maths layout, so `mathtext` places superscripts,
  subscripts and symbols itself. Keep it to a subset (see SPEC). Labels that need a
  fallback font are placed piece by piece with our own measurement, so a small gap or
  overlap between pieces would show up there first.
- **Text measurement vs drawing:** margins are sized with our own measurement
  (`render/text.rs`, no kerning), while `resvg`/`svg2pdf` lay out the text themselves. They
  agree to within a fraction of a point; if labels ever look clipped, start here.
- **Panel layout:** each panel is measured on its own and the plot areas are then lined
  up; a very wide tick label in one panel narrows its whole column. Long legends inside
  small panels can cover data; `legend=out` or `--size` helps.
- **Fits on log axes:** on a log y axis `exp` and `power` stay fitted to the logarithms
  (a true least-squares fit there visibly misses the small values); elsewhere they are
  true least squares.
- **Peaks and onsets on noisy data:** peaks need 5 % prominence and the onset's steepest
  point is found on a lightly smoothed slope, but very noisy curves still want `smooth=`
  first. Mark labels avoid each other and the rings, not the curves (a halo keeps them
  readable).
- **Huge scatter plots:** 50k random points take about 0.5 s (every dot is drawn). Lines
  are thinned, so they stay fast.
- **Whole-disk search:** some folders need macOS *Full Disk Access*. Using the Spotlight
  index avoids crawling.
- **Private blur call:** the blur strength uses an undocumented macOS function (Ghostty,
  kitty, WezTerm and Dispatch rely on it too). If an update removes it, the launcher falls
  back to the frosted material and lists a warning.
- **Not notarised:** builds are signed ad hoc (or, when the repository has the secrets,
  with a self-made certificate, so macOS keeps its permissions between builds), but
  without a paid Apple account Gatekeeper blocks the first launch of a new install:
  System Settings → Privacy & Security → Open Anyway.
- **Linux vs Mac:** the engine is tested everywhere; the windows run in Chromium with
  stand-in answers (and the launcher's text box also in WebKit), but the real app only
  on a Mac.

## Glossary

- **Rust:** the programming language for the engine and app logic. It produces fast,
  standalone programs with no interpreter (unlike Python).
- **Cargo:** Rust's build tool and package manager. `Cargo.toml` lists dependencies;
  `Cargo.lock` pins their exact versions (committed, so builds are repeatable). Build output
  goes in `target/`, which can grow to a few GB but is ignored by git and can be deleted any
  time with `cargo clean`.
- **Crate:** a Rust package, either a library (like `csv` or `resvg`) or one of our own parts
  (`crates/core`).
- **Workspace:** several crates in one repo that share one `target/` folder and one lock file.
- **Tauri:** a toolkit for building desktop apps whose windows are web pages, using the
  web view macOS already has (so the app stays small, unlike Electron, which ships a
  whole browser).
- **Web view:** the built-in Safari engine that displays the app's windows.
- **TypeScript (TS):** JavaScript with type checking. It runs the windows.
- **npm / Node:** JavaScript tooling used only while building the windows. `node_modules/`
  is its download folder, ignored by git like `target/`.
- **Tauri command:** a Rust function the windows are allowed to call, like a doorbell from
  the interface to the engine.
- **Tray / menu-bar icon:** the small icon at the top right of the screen.
- **Accessory vs regular app:** macOS terms. An accessory app has no Dock icon; a regular
  app does. EasyPlots switches between them when Settings opens and closes.
- **Global hotkey:** a key combination that works whichever app is in front.
- **Fuzzy search:** matching that tolerates gaps, so `tmpcsv` finds `temperature_2024.csv`.
- **SVG / PNG / PDF:** SVG and PDF are *vector* formats (sharp at any zoom, best for papers);
  PNG is a *bitmap* (pixels; size set by DPI).
- **DPI:** dots per inch. 300 is normal for print.
- **Gatekeeper:** the macOS feature that blocks apps not signed by a registered developer.
- **CI:** continuous integration. GitHub runs the checks (and can build the app) on every push.
- **GitHub Actions / workflow:** GitHub's CI service; `.github/workflows/ci.yml` says what to run.
- **Artifact (CI):** a file a CI run keeps for download, here the `.dmg`, for 14 days.
- **Release:** a page under the repo's *Releases* tab with files attached. Publishing a
  `v0.1.0`-style release makes CI build the `.dmg` and attach it.
- **DMG:** the macOS disk-image file you open and drag the app out of.
- **Panel / subplot:** one plot area with its own axes; a figure can hold a grid of them.
- **Histogram / bin:** a histogram counts how many values fall into each of a row of equal
  ranges (bins) and draws one bar per bin.
- **Quartile, median, IQR:** sort the values; the median is the middle one, the lower and
  upper quartiles mark a quarter and three quarters of the way. The box of a box plot spans
  them; its height is the IQR (interquartile range). Whiskers reach the furthest values
  within 1.5 IQR; values beyond are drawn as outliers.
- **Least squares / R²:** a fit chooses the curve with the smallest total of squared gaps
  to the points. R² says how much of the points' spread the curve explains (1 = all).
- **Twin (second) y axis:** a second scale on the right for a series in different units.
- **Colour map / colour bar:** a scale from low values to high ones as colours; the bar
  beside a heatmap or contour shows which colour means which value.
- **Heatmap / contour:** a heatmap colours each cell of a table by its value; a contour
  draws lines through points of equal value, like height lines on a map.
- **KDE (kernel density estimate) / bandwidth:** a smooth curve showing where values are
  common, made by adding a small bell curve for each value; the bandwidth is how wide each
  bell is (wider = smoother).
- **Strip plot / beeswarm / jitter:** a strip plot draws every value of each group as a dot.
  In a beeswarm, dots that would cover each other move sideways just enough to touch; jitter
  moves each dot sideways at random instead (here with a fixed seed, so it looks the same
  every time).
- **Long vs wide data:** *wide* data has one column per group (`growth`, `yield` side by side);
  *long* data has a row per measurement and a column naming its group (`bed` = Shade / Part sun
  / Hot). `y=growth,yield` plots wide data; `by=bed` plots long data.
- **SVG transform / matrix:** SVG can move, turn or mirror a group of shapes as it draws
  them. `matrix(0 1 1 0 0 0)` swaps every x with its y (a mirror across the diagonal),
  which turns upright bars into horizontal ones.
- **Violin plot:** a box plot whose outline follows the KDE, so the shape of the data shows.
- **Polar plot:** points placed by angle and distance from the centre (radar, antenna
  patterns, wind directions).
- **Preset:** a command kept under a name, to draw the same plot with another file.
- **Login item / Launch Agent:** how macOS starts an app when you log in; a Launch Agent is a
  small file in `~/Library/LaunchAgents` that says which app to start.
- **Spotlight index / `mdfind`:** macOS keeps a list of every file for Spotlight search;
  `mdfind` asks that list, so no folders need crawling.
- **Style file:** a small TOML file of settings chosen by name with `--style=`.
- **Ad-hoc signing:** a signature with no Apple developer identity behind it. It keeps
  Apple Silicon happy but doesn't satisfy Gatekeeper, hence the one-time "Open Anyway".
- **`Info.plist` / `LSUIElement`:** the app's settings file for macOS; `LSUIElement` means
  "start as a menu-bar app with no Dock icon".
- **Capabilities (Tauri):** a list of what the windows are allowed to ask the Rust side
  for (`app/src-tauri/capabilities/`). Anything not listed is refused.
- **Vite:** the tool that bundles the windows' HTML, CSS and TypeScript into `app/dist`.
- **Clippy / rustfmt:** Rust's linter (spots likely mistakes) and formatter (fixes layout).
- **Golden file / golden test:** a stored "known good" output. The test fails if the output
  changes, so a change has to be accepted on purpose (`UPDATE_GOLDEN=1`).
- **Points (pt):** the print unit, 1/72 inch. All plot sizes, fonts and line widths use it.
- **Font subsetting:** putting only the letters actually used into a PDF, which keeps it small.
- **Fallback font:** the font used for a character the chosen font doesn't have (e.g. Greek
  in Latin Modern).
- **Log axis (logarithmic):** an axis where each step multiplies (1, 10, 100…) instead of
  adding, so values over many orders of magnitude fit; zero and negatives can't be shown.
- **Decade:** a factor of ten on a log axis (from 10 to 100, say).
- **Theme (plot):** a ready-made look: fonts, colours, frame, ticks and grid together.
- **`<tspan>`:** a piece of SVG text with its own size, style or position, used for
  superscripts and subscripts.
- **Decimation (thinning):** dropping points that can't be seen at the page's resolution,
  while keeping each pixel slice's highest and lowest point.
- **Fuzzy matching:** see *Fuzzy search*; here done by `nucleo-matcher`, the matcher from
  the Helix editor.
- **Vibrancy / blur:** macOS's frosted-glass effect behind a window. The launcher blurs by
  a radius (0–100, like Ghostty's `background-blur`) through a private macOS call, with
  Apple's "popover" material as the fallback.
- **Private API / `dlsym`:** a macOS function Apple doesn't document. Looking it up by name
  while running (`dlsym`) instead of linking it means the app still starts if Apple removes it.
- **Opacity:** how see-through something is; 1 = solid, 0.9 = slightly see-through.
- **Active app / key window:** the active app is the one whose menus show in the menu bar;
  the key window is the one that gets the keyboard. Normally the key window belongs to
  the active app.
- **Panel (`NSPanel`) / non-activating panel:** a macOS window type for floating tools.
  A non-activating panel can become the key window without making its app active, which
  is how Spotlight, Alfred and Raycast appear without disturbing the app you were in.
- **AppKit / `objc2`:** AppKit is Apple's library for windows and screens; `objc2` lets
  Rust call it (Apple's own code is written in Objective-C).
- **FSEvents / file watcher:** a macOS service that tells a program when files in a folder
  change, so it doesn't have to keep checking. `notify` is the Rust crate that uses it.
- **Workbook / sheet:** an Excel (or LibreOffice) file is a workbook; each tab in it is a
  sheet.
- **JSON:** a text format for structured data (lists and named values), common for data
  exported from websites and instruments.
- **Thumbnail:** a small picture of a plot, for the saved-plots browser; kept in the cache.
- **Drag and drop target:** a view that macOS asks "can you take these files?" while you
  drag over it, and hands the files to when you let go.
- **First responder:** of all the views in the key window, the one that gets the typing.
  The launcher must make its web view the first responder, or keys go nowhere.
- **Moving average (smoothing):** each value replaced by the average of the values around
  it, which irons out noise; `smooth(col, 5)` uses 5 rows.
- **Style vs preset:** a style is only the look (fonts, colours, sizes), so it fits any
  data; a preset is a whole command, for files with the same columns.
- **File pattern (wildcard):** `*` stands for any text and `?` for one character, so
  `run_*.csv` means every file starting with `run_` and ending in `.csv`.
- **Undo stack:** the list of earlier states ⌘Z steps back through; ⌘⇧Z steps forward.
- **Draft (launcher):** what you had typed or picked; kept for 10 minutes after a click
  elsewhere.
- **Clipboard:** what ⌘C copies and ⌘V pastes.
- **Symlink (link):** a file that points at another file. `/usr/local/bin/easyplots` is one,
  pointing into EasyPlots.app.
- **PATH:** the list of folders Terminal searches for commands; `/usr/local/bin` is on it by
  default on macOS, which is why `easyplots` works without `./`.
- **async (command):** runs in the background so the window stays responsive.
- **Metadata:** information stored inside a file besides the picture, here the command that
  made the plot.
- **FFT (fast Fourier transform):** a quick way to split a signal sampled over time into
  the frequencies it's made of; `fft` plots how strong each one is.
- **Window (FFT):** a gentle fade-in and fade-out applied to the samples before the FFT,
  so the abrupt start and end don't show up as false frequencies. Hann is the usual one.
- **Interquartile range (IQR), quartiles:** sort the values; Q1 is the value a quarter of
  the way up, Q3 three quarters. The IQR is the stretch from Q1 to Q3: the middle half of
  the values. It goes with a median the way the standard deviation goes with a mean.
- **Pasteboard:** macOS's name for the clipboard. One copy can hold the same thing in
  several kinds (PNG, TIFF, text); the app that pastes picks the kind it likes.
- **Inset:** a small second plot inside the main one, here a zoomed copy of part of it.
- **Prominence:** how far a peak stands out from its surroundings: its height above the
  higher of the two lowest points between it and higher ground on either side. Used to
  ignore noise wiggles when finding peaks.
- **Onset (two-tangent):** the usual way to put one number on where a gradual step starts
  (a glass transition, a melting peak): extend the flat baseline before the step and the
  tangent at its steepest point; the onset is where they cross.
- **Trapezoid rule:** the area under a curve measured as thin trapezoids between
  neighbouring points; used by `integral` and `area=`.
- **`<textarea>` / `<input>`:** the two kinds of text box in a web page. An `<input>` is
  always one line (a long text slides sideways); a `<textarea>` can wrap onto several. The
  launcher's command box is a `<textarea>` held to one to three lines, with Enter caught so
  it never makes a new line.
- **Pearson's r / Spearman's r_s:** two correlation numbers from −1 to 1. Pearson's
  measures how close the points are to a straight line; Spearman's is Pearson's on the
  points' ranks (1st, 2nd…), so it measures whether y keeps rising (or falling) with x,
  straight or not.
- **Halo:** a thin outline in the background colour drawn behind text, so a label stays
  readable where it crosses a line or bar.
- **Residual:** how far a measured point is from its fitted curve (data − fit). If the fit
  is the right shape, residuals scatter around 0 with no pattern; a curve or a funnel in
  them means the shape (or the error model) is off.
- **Q–Q plot (quantile–quantile):** the residuals sorted from smallest to largest, plotted
  against where the same number of values from a normal (bell-curve) distribution would
  fall. On the straight line: normally spread; an S-shape: too many or too few large
  residuals; a bend: lopsided.
- **RMSE (root mean square error):** the typical size of a residual: square each, take
  the mean, take the square root. In the same units as y.
- **Baseline:** the slowly changing background under a signal (the slope under a
  spectrum's peaks, a DSC curve's drift), estimated from stretches where there is no
  signal and then subtracted.
