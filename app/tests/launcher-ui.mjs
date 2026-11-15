// Launcher UI test: drives the built launcher (app/dist) in Chromium, with stand-in
// answers instead of the Rust side, and saves screenshots.
//
//   npm --prefix app run build
//   npm --prefix app run test:ui            (screenshots go to app/tests/out/)
//
// Uses Playwright's Chromium, or the browser at $CHROMIUM_PATH if set.
import { chromium, webkit } from "playwright";
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const DIST = here("../dist");
const OUT = here("./out");
mkdirSync(OUT, { recursive: true });
const THUMB = "data:image/png;base64," + readFileSync(here("../../docs/gallery/line_two_columns.png")).toString("base64");
const server = spawn("python3", ["-m", "http.server", "4173", "--directory", DIST], { stdio: "ignore" });
await new Promise((r) => setTimeout(r, 800));

const browser = await chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});
let failures = 0;
const check = (ok, what) => { console.log(`${ok ? "ok  " : "FAIL"} ${what}`); if (!ok) failures++; };

const THEMES = {
  mocha: { name: "catppuccin-mocha", mode: "dark", colors: { background: "#1e1e2e", foreground: "#cdd6f4", accent: "#89b4fa", selection: "#45475a", muted: "#585b70", dark_foreground: "#6c7086", light_foreground: "#bac2de", red: "#f38ba8", orange: "#f6b6ab", yellow: "#f9e2af", green: "#a6e3a1", cyan: "#94e2d5", blue: "#89b4fa", magenta: "#f5c2e7" } },
  gruvbox: { name: "gruvbox-dark", mode: "dark", colors: { background: "#282828", foreground: "#d4be98", accent: "#7daea3", selection: "#504945", muted: "#665c54", dark_foreground: "#7c6f64", light_foreground: "#bdae93", red: "#ea6962", orange: "#e1875c", yellow: "#d8a657", green: "#a9b665", cyan: "#89b482", blue: "#7daea3", magenta: "#d3869b" } },
};

async function open(theme = "mocha", engine = browser) {
  const page = await engine.newPage({ viewport: { width: 640, height: 420 } });
  page.on("pageerror", (e) => { console.log("page error:", e.message); failures++; });
  await page.addInitScript(({ thumb, theme }) => {
    const files = ["simple.csv", "run_a.csv", "run_b.csv", "dates.csv", "lab/missing_and_comments.csv", "Documents/projects/personal projects/thesis/chapter 2/raw data/2026/dma_sample_3.csv"];
    const fuzzy = (q, s) => { let p = -1; for (const c of q.toLowerCase()) { p = s.toLowerCase().indexOf(c, p + 1); if (p < 0) return false; } return true; };
    const themeInfo = { dark: theme, light: theme, source: "test", look: { opacity: 0.9, blur: 20, palette: null, faint: false }, stamp: 1 };
    window.__calls = [];
    let cb = 0;
    const handlers = {
      launcher_info: () => ({ mode: "picker", history: ["scatter run_a.csv x=t y=temp"], warnings: [], data_dir: "~/Documents", scanning: false,
        format: "png", live_preview: true, presets: [{ name: "thesis", command: "line simple.csv x=time y=temp --style=thesis" }],
        styles: window.__styles ?? [],
        flags: ["markers", "between", "band", "dropna", "logx", "logy", "stacked", "grouped", "density", "filled", "values"],
        ui: { opacity: 0.9, font: "JetBrains Mono", font_size: 15, hints: true, theme: themeInfo } }),
      current_theme: () => themeInfo,
      search_files: ({ query }) => {
        const hits = files.filter((f) => fuzzy(query, f)).map((f) => ({ display: f, insert: f, file: f, sheet: null, guess: false }));
        if (query === "runs") return [{ display: "lab/runs/", insert: "lab/runs/", file: "lab/runs/", sheet: null, guess: false, folder: true }];
        if (query === "book") return [{ display: "book.xlsx › Run2", insert: "book.xlsx sheet=Run2", file: "book.xlsx", sheet: "Run2", guess: false }];
        return hits.length ? hits : query === "smiple" ? [{ display: "simple.csv", insert: "simple.csv", file: "simple.csv", sheet: null, guess: true }] : [];
      },
      peek_file: ({ file }) => ({ file, rows: 40, names: ["time", "temp", "note"], kinds: ["numbers", "numbers", "text"],
        cells: [["0", "20", "start"], ["0.5", "21.3", ""], ["1", "21.9", "door open"]] }),
      list_columns: ({ file }) => ({ file, rows: 40, columns: [
        { name: "time", insert: "time", kind: "numbers" },
        { name: "temp", insert: "temp", kind: "numbers", summary: "16.9 – 28.2 · mean 22.4 · 2 blanks" },
        { name: "pressure", insert: "pressure", kind: "numbers" },
        { name: "Voltage (V)", insert: '"Voltage (V)"', kind: "numbers" },
        { name: "date", insert: "date", kind: "dates" },
        { name: "note", insert: "note", kind: "text" },
      ] }),
      complete_command: ({ input, cursor }) => {
        if (input.includes("title=")) return { start: cursor, end: cursor, items: [] };
        const colour = /color=\w*$/.exec(input);
        if (colour) return { start: cursor - colour[0].length, end: cursor, items: [
          { label: "blue", insert: "color=blue ", detail: "#0072b2" },
          { label: "orange", insert: "color=orange ", detail: "#e69f00" },
          { label: "green", insert: "color=green ", detail: "#009e73" },
        ] };
        return { start: cursor, end: cursor, items: [
          { label: "title=", insert: "title=", detail: "plot title" },
          { label: "legend=", insert: "legend=", detail: "legend position" },
        ] };
      },
      preview: ({ command }) => {
        const i = command.indexOf("tmie");
        if (i >= 0) throw { message: 'no column "tmie" in simple.csv — did you mean "time"?', start: i, end: i + 4 };
        // The plot advisor: two columns far apart get a hint with a fix (round 11).
        const advice = command.endsWith("y=temp,pressure")
          ? [{ text: "pressure looks flat beside the other curve: axis=right gives it its own axis", fix: command.replace(",pressure", " + y=pressure axis=right") }]
          : command.includes("legend=br") ? [{ text: "panel 1: the legend covers data: legend=out puts it beside the plot", fix: null }] : [];
        return { image: thumb, millis: 98, advice };
      },
      end_preview: () => null,
      save_preview: () => window.__slowSave
        ? new Promise((r) => setTimeout(() => r({ path: "/Users/me/Plots/slow.png", shown_path: "~/Plots/slow.png", after_save: "thumbnail" }), 500))
        : ({ path: "/Users/me/Plots/simple_line.png", shown_path: "~/Plots/simple_line_2026-10-21-143005.png", after_save: "thumbnail" }),
      save_preview_as: () => ({ path: "/Users/me/Desktop/fig1.pdf", shown_path: "~/Desktop/fig1.pdf", after_save: "thumbnail" }),
      copy_preview: () => null,
      hide_launcher: () => null,
      open_plot: () => null,
      preview_thumb: ({ command }) => {
        if (command.includes("tmie")) throw null;
        return thumb;
      },
      hold_open: () => null,
      clipboard_table: () => window.__clip ?? null,
      recent_plots: () => [
        { path: "~/Plots/simple_line_1.png", name: "simple_line_1.png", modified: Date.now() / 1000 - 300, command: "line simple.csv x=time y=temp" },
        { path: "~/Plots/run_scatter.svg", name: "run_scatter.svg", modified: Date.now() / 1000 - 7200, command: "scatter run_a.csv x=t y=temp" },
        { path: "~/Plots/old_hist.pdf", name: "old_hist.pdf", modified: 1767225600, command: "hist samples.csv x=height" },
        { path: "~/Plots/box.png", name: "box.png", modified: 1767225600, command: "box samples.csv x=site y=height" },
      ],
      plot_thumb: () => thumb,
      grow_launcher: () => null,
      save_clipboard: () => "~/Plots/pasted/pasted-2026-10-23-140500.tsv",
      set_pinned: () => null,
      save_preset: ({ name }) => {
        if (name.includes(" ") || !name) throw { message: "a preset name is one word, e.g. thesis-line", start: null, end: null };
        return null;
      },
      save_style: ({ name }) => {
        if (name.includes(" ")) throw { message: "a style name is one word", start: null, end: null };
        return `~/.config/easyplots/styles/${name}.toml`;
      },
      apply_preset: ({ name, file }) => `line ${file} x=time y=temp --style=${name}`,
      recall_plot: ({ plot }) => {
        if (plot !== "old.png") throw { message: `no plot ${plot}`, start: null, end: null };
        return "hist samples.csv y=height bins=20";
      },
      "plugin:event|listen": ({ event, handler }) => {
        (window.__listeners[event] ??= []).push(handler);
        return handler;
      },
    };
    // Events from the Rust side: window.__emit("launcher-shown").
    window.__listeners = {};
    window.__emit = (event, payload = null) => (window.__listeners[event] ?? []).forEach((id) => window[`_${id}`]({ event, id, payload }));
    window.__TAURI_INTERNALS__ = {
      transformCallback: (fn) => {
        const id = ++cb;
        window[`_${id}`] = fn;
        return id;
      },
      invoke: async (cmd, args) => {
        window.__calls.push([cmd, args]);
        if (!handlers[cmd]) throw new Error("no handler " + cmd);
        return handlers[cmd](args);
      },
      metadata: { currentWindow: { label: "launcher" }, currentWebview: { windowLabel: "launcher", label: "launcher" } },
    };
  }, { thumb: THUMB, theme: THEMES[theme] });
  await page.goto("http://localhost:4173/launcher/index.html");
  await page.waitForTimeout(400);
  return page;
}
const val = (p) => p.$eval("#query", (e) => e.value);
const labels = (p) => p.$$eval("#list li .label", (l) => l.map((e) => e.textContent));
const details = (p) => p.$$eval("#list li .detail", (l) => l.map((e) => e.textContent));
const calls = (p, name) => p.evaluate((n) => window.__calls.filter((c) => c[0] === n), name);
const text = (p, sel) => p.$eval(sel, (e) => e.textContent);
const hasClass = (p, sel, c) => p.$eval(sel, (e, c) => e.classList.contains(c), c);
const pause = (p, ms = 150) => p.waitForTimeout(ms);

// --- picker flow: file → type → columns, with a typo in a column name ---
let p = await open();
await pause(p, 350);
await p.screenshot({ path: `${OUT}/1_files.png` });
check(await p.isVisible("#peek") && (await text(p, "#peek")).includes("first 3 of 40 rows") && (await text(p, "#peek th")).includes("time"),
  "the highlighted file's first rows are shown beside the list (round 10)");
check(await p.$eval("#peek", (e) => e.getBoundingClientRect().right <= document.querySelector("#panel").getBoundingClientRect().right), "the peek stays inside the launcher");
await p.keyboard.type("runs");
await pause(p, 350);
check((await labels(p))[0] === "runs/" && (await details(p))[0] === "folder · one panel per file", "a folder of data files is listed as one entry");
await p.screenshot({ path: `${OUT}/1b_folder.png` });
for (let i = 0; i < 4; i++) await p.keyboard.press("Backspace");
await pause(p);
check((await labels(p)).length === 6, "file stage lists CSVs");
check((await labels(p)).includes("dma_sample_3.csv"), "a file deep in folders shows its name first (owner: long paths hid it)");
check(await p.$eval("#list li:nth-child(6) .folder", (e) => e.getBoundingClientRect().right <= e.closest("li").getBoundingClientRect().right + 1), "its folder is cut to fit");
check((await p.$eval("#panel", (e) => getComputedStyle(e).backgroundColor)) === "rgba(30, 30, 46, 0.9)", "theme colour at 90% opacity");
await p.keyboard.type("smiple");
await pause(p);
check((await labels(p))[0] === "simple.csv" && (await details(p))[0] === "did you mean?", "file typo offers the closest file");
await p.keyboard.press("Enter");
await pause(p);
check((await text(p, "#stage")) === "type", "type stage right after the file (owner's request)");
const labels_ = await labels(p);
check(["hist", "heatmap", "contour", "kde", "violin", "polar", "fft"].every((k) => labels_.includes(k)), "M4 and M6 plot types listed");
check(labels_.at(-1) === "preset: thesis", "type stage offers presets for the chosen file, last");
await pause(p, 400);
check(await p.isVisible("#live"), "live preview in the picker: the highlighted type, drawn (owner's question)");
check((await calls(p, "preview_thumb")).at(-1)[1].command === "line simple.csv", `live preview draws ${(await calls(p, "preview_thumb")).at(-1)[1].command}`);
await p.screenshot({ path: `${OUT}/3_types.png` });
await p.keyboard.press("Enter"); // line
await pause(p);
check((await text(p, "#stage")) === "x", "x stage after the type");
check((await labels(p))[0] === "(row number)", "row number offered for x");
await p.keyboard.type("tmie");
await pause(p);
check((await labels(p))[0] === "time" && (await details(p))[0] === "did you mean?", "column typo offers the closest column (owner's bug)");
await p.keyboard.press("Enter");
await pause(p);
check((await text(p, "#stage")) === "y", "y stage");
check((await p.$eval("#list li", (e) => e.textContent)).includes("tick several"), "y stage says how to pick several columns");
check(!(await labels(p)).includes("note") && !(await labels(p)).includes("date"), "y lists only number columns");
check((await details(p)).includes("16.9 – 28.2 · mean 22.4 · 2 blanks"), "columns show their range, mean and blanks");
await p.keyboard.press("Tab"); // temp
await p.keyboard.press("ArrowDown");
await p.keyboard.press("ArrowDown");
await p.keyboard.press("Tab"); // Voltage (V)
await pause(p);
await p.screenshot({ path: `${OUT}/2_y_ticks.png` });
check((await text(p, "#crumb")).includes('y=temp,"Voltage (V)"'), "crumb shows ticked y columns");
await p.keyboard.press("Enter");
await pause(p);
check((await val(p)) === 'line simple.csv x=time y=temp,"Voltage (V)" ', `command built: ${await val(p)}`);
check((await text(p, "#mirror .tok-type")) === "line" && (await text(p, "#mirror .tok-file")) === "simple.csv", "command is coloured");

// The picker asks only what each type needs (owner's M4 note on hist).
async function pick(kind, ...steps) {
  const q = await open();
  await q.keyboard.press("Enter"); // simple.csv
  await pause(q);
  await q.keyboard.type(kind);
  await pause(q);
  await q.keyboard.press("Enter");
  const stages = [];
  for (const step of steps) {
    await pause(q);
    stages.push(await text(q, "#stage"));
    if (step) await q.keyboard.type(step);
    await pause(q);
    await q.keyboard.press("Enter");
  }
  await pause(q);
  return [q, stages];
}
let [q, st] = await pick("hist", "pressure");
check(st.join() === "x" && (await val(q)) === "hist simple.csv x=pressure ", `hist asks for x only (owner's note): ${st} → ${await val(q)}`);
[q, st] = await pick("box", "temp", "note");
check(st.join() === "y,group" && (await val(q)) === "box simple.csv x=note y=temp ", `box asks y then a group: ${st} → ${await val(q)}`);
[q, st] = await pick("box", "temp", "");
check((await val(q)) === "box simple.csv y=temp ", `box without groups: ${await val(q)}`);
[q, st] = await pick("heatmap", "");
check(st.join() === "x" && (await val(q)) === "heatmap simple.csv ", `heatmap of the whole table: ${await val(q)}`);
[q, st] = await pick("contour", "time", "temp", "pressure");
check((await val(q)) === "contour simple.csv x=time y=temp z=pressure ", `contour from three columns: ${await val(q)}`);
// ⌫ steps back one stage at a time, to the type and then the file.
q = await open();
await q.keyboard.press("Enter");
await pause(q);
await q.keyboard.type("scatter");
await q.keyboard.press("Enter");
await pause(q);
await q.keyboard.press("Enter"); // x = row number
await pause(q);
await q.keyboard.press("Backspace");
await pause(q);
check((await text(q, "#stage")) === "x", "⌫ goes back to x");
await q.keyboard.press("Backspace");
await pause(q);
check((await text(q, "#stage")) === "type", "⌫ goes back to the type");
await q.keyboard.press("Backspace");
await pause(q);
check((await text(q, "#stage")) === "file", "⌫ goes back to the file");

// --- preview, save, open ---
await p.keyboard.type('title="Run 3"');
await p.keyboard.press("Enter");
await pause(p, 300);
check(await p.isVisible("#plot"), "↵ shows a preview");
check((await calls(p, "save_preview")).length === 0, "nothing saved by the preview");
check((await calls(p, "preview"))[0][1].command === 'line simple.csv x=time y=temp,"Voltage (V)" title="Run 3"', "preview gets the command");
await p.screenshot({ path: `${OUT}/4_preview.png` });
await p.keyboard.press("Meta+c");
await pause(p);
check((await text(p, "#hints")).includes("save PNG") && (await text(p, "#hints")).includes("PDF"), "preview hints show the format and ⌘1–3");
await p.keyboard.press("Meta+d");
await pause(p);
check((await text(p, "#stage")) === "style", "⌘D asks for a style name (the look only)");
await p.keyboard.type("bright");
await p.keyboard.press("Enter");
await pause(p);
check((await calls(p, "save_style")).some((c) => c[1].name === "bright") && (await text(p, "#status")).includes("--style=bright"), "⌘D keeps the look as a style");
await p.keyboard.press("Meta+Shift+d");
await pause(p);
check((await text(p, "#stage")) === "preset", "⌘⇧D asks for a preset name (the whole command)");
await p.keyboard.type("two words");
await p.keyboard.press("Enter");
await pause(p);
check(await p.isVisible("#error"), "a preset name with a space is refused");
await p.fill("#query", "");
await p.keyboard.type("runs");
check((await text(p, "#mirror")) === "runs", "the preset name shows while typing (owner's bug: invisible)");
await p.keyboard.press("Enter");
await pause(p);
check((await calls(p, "save_preset")).some((c) => c[1].name === "runs") && (await text(p, "#status")).includes("runs"), "⌘D keeps the plot as a preset");
check((await text(p, "#stage")) === "plot", "back to the preview after naming");
check((await calls(p, "copy_preview")).length === 1, "⌘C copies the image");
await p.keyboard.press("Enter");
await pause(p);
check((await calls(p, "save_preview")).length === 1 && (await hasClass(p, "#panel", "saved")), "↵ again saves");
await p.screenshot({ path: `${OUT}/5_saved.png` });
await p.keyboard.press("Enter");
await pause(p);
check((await calls(p, "open_plot")).length === 1, "↵ after saving opens the plot");

// --- editing a preview goes back to command mode; Save As ---
p = await open();
await p.keyboard.type("line simple.csv x=time y=temp");
await p.keyboard.press("Enter");
await pause(p, 300);
await p.keyboard.press("Meta+Shift+s");
await pause(p);
check((await calls(p, "save_preview_as")).length === 1 && (await text(p, "#status .path")) === "~/Desktop/fig1.pdf", "⌘⇧S saves where the dialog says");
await p.keyboard.type(" color=");
await pause(p);
check((await text(p, "#stage")) === "cmd" && (await calls(p, "end_preview")).length >= 1, "typing after a preview edits the command");
check((await p.$$("#list .swatch")).length === 3, "colour suggestions show swatches");
await p.screenshot({ path: `${OUT}/6_colours.png` });

// --- an error, in another theme ---
p = await open("gruvbox");
await p.keyboard.type("line simple.csv x=tmie y=temp");
await pause(p);
check((await text(p, "#stage")) === "cmd", "typing a plot type switches to command mode");
await p.keyboard.press("Enter");
await pause(p, 250);
check(await p.isVisible("#error"), "error shown");
check(await hasClass(p, "#panel", "has-error"), "border turns red");
check((await text(p, "#mirror .tok-err")) === "tmie", "the typo is highlighted");
const sel = await p.$eval("#query", (e) => e.value.slice(e.selectionStart, e.selectionEnd));
check(sel === "tmie", "the typo is selected for retyping");
await p.screenshot({ path: `${OUT}/7_error_gruvbox.png` });

// --- ⌘2 saves this plot as PDF ---
p = await open();
await p.keyboard.type("line simple.csv x=time y=temp");
await pause(p, 700);
check(await p.isVisible("#live"), "live preview shows while typing");
await p.keyboard.press("Enter");
await pause(p, 300);
await p.keyboard.press("Meta+2");
await pause(p, 200);
check((await calls(p, "save_preview")).some((c) => c[1].format === "pdf"), "⌘2 saves as PDF");
await p.screenshot({ path: `${OUT}/9_live.png` });

// --- presets in the picker, colour picker in command mode ---
p = await open();
await p.keyboard.press("Enter"); // simple.csv
await pause(p);
await p.keyboard.press("ArrowUp");
await p.keyboard.press("Enter");
await pause(p);
check((await val(p)).startsWith("line simple.csv x=time y=temp --style=thesis"), "a preset fills in the command with this file");
await p.fill("#query", "line simple.csv color=");
await p.keyboard.press("End");
await p.keyboard.type("b");
await pause(p);
check((await labels(p)).includes("Pick a colour…"), "color= offers the colour panel");

// --- history and mode switching ---
p = await open();
await p.keyboard.press("ArrowUp");
await pause(p);
check((await text(p, "#stage")) === "file" && (await val(p)) === "", "↑ in the picker only moves through the list");
await p.keyboard.press("Meta+e");
await pause(p);
check((await text(p, "#stage")) === "cmd", "⌘E opens command mode");
await p.keyboard.press("Meta+e");
await pause(p);
check((await text(p, "#stage")) === "file", "⌘E goes back to the picker");
await p.keyboard.press("Meta+e");
await pause(p);
check((await text(p, "#stage")) === "cmd" && (await val(p)) === "", "⌘E opens an empty command");
check((await labels(p))[0] === "scatter run_a.csv x=t y=temp", "empty command box lists recent commands");
await p.screenshot({ path: `${OUT}/8_history.png` });
await p.keyboard.type("again old.png");
await p.keyboard.press("Enter");
await pause(p, 250);
check((await val(p)) === "hist samples.csv y=height bins=20 ", "again puts a saved plot's command back");
await p.fill("#query", "again nope.png");
await p.keyboard.press("Enter");
await pause(p, 250);
check(await p.isVisible("#error"), "again with an unknown plot shows an error");
await p.keyboard.press("Escape");
await pause(p);
check((await calls(p, "hide_launcher")).length >= 1, "Esc hides the launcher");

// --- clicking away keeps the draft; Esc starts fresh (owner's M4 note) ---
p = await open();
await p.keyboard.press("Meta+e");
await p.keyboard.type("scatter simple.csv x=time y=te");
await pause(p);
await p.evaluate(() => window.__emit("launcher-shown")); // hidden by a click elsewhere, shown again
await pause(p, 250);
check((await val(p)) === "scatter simple.csv x=time y=te", "a click-away keeps what was typed");
await p.keyboard.press("Escape");
await pause(p);
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
check((await text(p, "#stage")) === "file" && (await val(p)) === "", "after Esc it starts fresh");
check(await p.isHidden("#live"), "the old live drawing is gone after Esc (owner's bug)");
// A preview left by clicking away comes back as its command.
await p.keyboard.type("line simple.csv x=time y=temp");
await p.keyboard.press("Enter");
await pause(p, 300);
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
check((await text(p, "#stage")) === "cmd" && (await val(p)) === "line simple.csv x=time y=temp", "a preview comes back as its command");

// A slow save finishing after Esc doesn't take over the fresh launcher (seen in the real app:
// the next ↵ then opened the old plot and closed the launcher).
await p.evaluate(() => (window.__slowSave = true));
await p.keyboard.press("Enter"); // preview
await pause(p, 300);
await p.keyboard.press("Enter"); // save, slowly
await pause(p, 50);
await p.keyboard.press("Escape");
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 700);
check((await text(p, "#stage")) === "file" && !(await hasClass(p, "#panel", "saved")), "a save that ends after Esc leaves the new session alone");
await p.keyboard.type("line simple.csv x=time y=temp");
await p.keyboard.press("Enter");
await pause(p, 300);
check(await p.isVisible("#plot"), "↵ draws straight away in the new session");

// --- ⌘O: the saved-plots browser ---
p = await open();
await p.keyboard.press("Meta+o");
await pause(p, 300);
check((await p.$$(".grid .tile")).length === 4 && (await text(p, "#stage")) === "plots", "⌘O shows the saved plots as a grid");
check(await p.$eval(".grid .tile img", (i) => i.src.startsWith("data:image/png")), "with pictures");
await p.screenshot({ path: `${OUT}/11_browser.png` });
await p.keyboard.press("ArrowDown"); // 3 columns: the 4th tile
await pause(p);
check((await text(p, ".grid .tile.selected .name")) === "box.png", "↓ moves a row down");
await p.keyboard.type("hist");
await pause(p);
check((await p.$$(".grid .tile")).length === 1, "typing filters by name or command");
await p.keyboard.press("Enter");
await pause(p);
check((await text(p, "#stage")) === "cmd" && (await val(p)) === "hist samples.csv x=height ", `↵ puts its command back: ${await val(p)}`);
await p.fill("#query", "again");
await p.keyboard.press("Enter");
await pause(p, 300);
check((await text(p, "#stage")) === "plots", "↵ on a bare `again` opens the browser too");
await p.keyboard.press("Meta+Enter");
await pause(p);
check((await calls(p, "open_plot")).length === 1, "⌘↵ opens the file");

// --- files dropped on the menu-bar icon: the first is picked, the rest overlaid ---
p = await open();
await p.evaluate(() => window.__emit("launcher-shown", ["~/Data/run_a.csv", "~/Data/run b.csv"]));
await pause(p, 300);
check((await text(p, "#stage")) === "type" && (await text(p, "#crumb")).includes("+ 1 more file"), "a drop picks the first file and says more are coming");
await p.keyboard.press("Enter"); // line
await pause(p);
await p.keyboard.type("time");
await p.keyboard.press("Enter");
await pause(p);
await p.keyboard.type("temp");
await p.keyboard.press("Enter");
await pause(p);
check((await val(p)) === 'line ~/Data/run_a.csv x=time y=temp + "~/Data/run b.csv" x=time y=temp ', `the other file is overlaid: ${await val(p)}`);

// --- a table on the clipboard is the first file; choosing it keeps it as a file ---
p = await open();
await p.evaluate(() => (window.__clip = { rows: 12, columns: 3 }));
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
check((await labels(p))[0] === "Clipboard" && (await details(p))[0].startsWith("12 rows × 3 columns"), "the clipboard's table is offered first");
await p.keyboard.press("Enter");
await pause(p);
check((await calls(p, "save_clipboard")).length === 1 && (await text(p, "#stage")) === "type", "choosing it saves it and goes on to the type");
await p.keyboard.press("Enter"); // line
await pause(p);
await p.keyboard.press("Enter"); // row number
await pause(p);
await p.keyboard.press("Enter"); // first y
await pause(p);
check((await val(p)).startsWith("line ~/Plots/pasted/pasted-2026-10-23-140500.tsv"), `the command names the saved file: ${await val(p)}`);

// --- a workbook sheet is one entry; its columns come from that sheet ---
p = await open();
await p.keyboard.type("book");
await pause(p);
check((await labels(p))[0] === "book.xlsx › Run2", "a workbook sheet is listed as its own entry");
await p.keyboard.press("Enter");
await pause(p);
check((await calls(p, "list_columns")).at(-1)[1].sheet === "Run2", "its columns are read from that sheet");
await p.keyboard.press("Enter"); // line
await pause(p);
await p.keyboard.press("Enter"); // row number
await pause(p);
await p.keyboard.type("temp");
await p.keyboard.press("Enter");
await pause(p);
check((await val(p)) === "line book.xlsx sheet=Run2 y=temp ", `the command names the sheet: ${await val(p)}`);

// --- with saved styles, the picker ends with a style step ---
p = await open();
await p.evaluate(() => (window.__styles = ["bright", "thesis"]));
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
await p.keyboard.press("Enter"); // simple.csv
await pause(p);
await p.keyboard.type("hist");
await p.keyboard.press("Enter");
await pause(p);
await p.keyboard.type("temp");
await p.keyboard.press("Enter");
await pause(p);
check((await text(p, "#stage")) === "style" && (await labels(p))[0] === "(no style)", "after the columns: the style step");
await p.keyboard.type("thes");
await p.keyboard.press("Enter");
await pause(p);
check((await val(p)) === "hist simple.csv x=temp --style=thesis ", `a style ends the command: ${await val(p)}`);

// --- closed while naming a style: the next start is clean (owner's bug 8) ---
p = await open();
await p.keyboard.press("Meta+e");
await pause(p);
await p.keyboard.type("line simple.csv x=time y=temp");
await p.keyboard.press("Enter");
await pause(p, 300);
await p.keyboard.press("Meta+d");
await pause(p);
await p.keyboard.press("Escape");
await pause(p);
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
check(!(await p.isVisible("#plot")) && (await labels(p)).length > 0, "after Esc while naming, the launcher opens fresh with suggestions");
await p.keyboard.press("Meta+e");
await pause(p);
await p.keyboard.type("line simple.csv x=");
await pause(p);
check((await labels(p)).includes("title=") && (await p.innerHTML("#mirror")).includes("tok-type"), "…and command mode suggests and colours again");

// --- ⌘P pins the launcher; files dragged onto it are picked ---
p = await open();
await p.keyboard.press("Meta+p");
await pause(p);
check(await p.isVisible("#pin") && (await calls(p, "set_pinned")).at(-1)[1].on === true, "⌘P pins (icon shows, Rust told)");
await p.evaluate(() => window.__emit("drag-over", true));
check(await hasClass(p, "#panel", "dropping"), "dragging over shows where to drop");
await p.evaluate(() => window.__emit("drag-over", false));
await p.evaluate(() => window.__emit("launcher-shown", ["~/Data/run_a.csv"]));
await pause(p, 250);
check((await text(p, "#stage")) === "type" && await p.isVisible("#pin"), "a dropped file is picked and the pin stays");
await p.keyboard.press("Escape");
await pause(p);
check(!(await p.isVisible("#pin")) && (await calls(p, "set_pinned")).at(-1)[1].on === false, "Esc unpins");

// --- ⌘Z / ⌘⇧Z go back and forward through typing and picker steps ---
p = await open();
await p.keyboard.press("Enter"); // simple.csv → type step
await pause(p);
await p.keyboard.type("line");
await p.keyboard.press("Enter"); // → x step
await pause(p);
check((await text(p, "#stage")) === "x", "picker at the x step");
await p.keyboard.press("Meta+z");
await pause(p);
check((await text(p, "#stage")) === "type" && (await val(p)) === "line", `⌘Z goes back to the type step with what was typed: ${await text(p, "#stage")} "${await val(p)}"`);
await p.keyboard.press("Meta+z");
await pause(p);
check((await text(p, "#stage")) === "type" && (await val(p)) === "", "⌘Z again undoes the typing");
await p.keyboard.press("Meta+Shift+z");
await p.keyboard.press("Meta+Shift+z");
await pause(p);
check((await text(p, "#stage")) === "x", "⌘⇧Z redoes both");
p = await open();
await p.keyboard.press("Meta+e");
await pause(p);
await p.keyboard.type("line simple.csv x=time");
await pause(p, 1100);
await p.keyboard.type(" y=temp");
await pause(p);
await p.keyboard.press("Meta+z");
await pause(p);
check((await val(p)) === "line simple.csv x=time", `⌘Z undoes the last burst of typing: "${await val(p)}"`);

// --- a click anywhere in the launcher keeps the keyboard in the box (owner's bug) ---
p = await open();
await p.click("#crumb");
await p.keyboard.type("run");
await pause(p);
check((await val(p)) === "run", "after a click above the box, typing still goes into it");
check((await p.getAttribute("#query", "writingsuggestions")) === "false", "macOS writing suggestions are off for the box");

// --- ⇥ on a long command keeps the end in view (owner's M4 note) ---
p = await open();
await p.keyboard.press("Meta+e");
await p.keyboard.type('line simple.csv x=time y=temp,pressure title="A rather long title for this plot" xlabel="Time (s)" ');
await pause(p);
await p.keyboard.press("Tab");
await pause(p);
// The box folds a long command onto more lines (up to 3) instead of hiding its start.
const lines = await p.$eval("#query", (e) => Math.round(e.clientHeight / parseFloat(getComputedStyle(e).lineHeight)));
check(lines >= 2 && lines <= 3, `a long command folds onto ${lines} lines`);
const scroll = await p.$eval("#query", (e) => [e.scrollTop, e.scrollHeight - e.clientHeight]);
check(scroll[0] >= scroll[1] - 2, `the accepted option is in view (scroll ${scroll})`);
check((await p.$eval("#mirror", (e) => e.scrollTop)) === scroll[0], "the colours scroll with it");
await p.screenshot({ path: `${OUT}/command-folded.png` });
await p.keyboard.type('ylabel="A second long label to go past three lines" legend=out grid=y values=%.1f fitlabel=tr corr ');
await pause(p);
const capped = await p.$eval("#query", (e) => [Math.round(e.clientHeight / parseFloat(getComputedStyle(e).lineHeight)), e.scrollTop, e.scrollHeight - e.clientHeight]);
check(capped[0] === 3 && capped[1] >= capped[2] - 2, `a very long command stops at 3 lines and scrolls to the end (${capped})`);
await p.screenshot({ path: `${OUT}/command-three-lines.png` });
await p.screenshot({ path: `${OUT}/10_long_tab.png` });

// --- the plot advisor (round 11): a hint under the preview, ⇥ applies it ---
await p.keyboard.press("Escape");
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
await p.keyboard.type("line simple.csv x=time y=temp");
await p.keyboard.press("Enter");
await pause(p, 300);
check(await p.isHidden("#advice"), "no hint for a plain plot");
await p.keyboard.press("Tab");
await pause(p);
check((await text(p, "#stage")) === "plot" && (await val(p)) === "line simple.csv x=time y=temp", "⇥ without a hint does nothing in the preview");
await p.keyboard.press("Escape");
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
await p.keyboard.type("line simple.csv x=time y=temp,pressure");
await p.keyboard.press("Enter");
await pause(p, 300);
check((await text(p, "#advice")).startsWith("hint: pressure looks flat") && (await text(p, "#advice")).includes("⇥ apply"), "the hint shows under the preview");
check((await text(p, "#hints")).includes("apply hint"), "the key hints list ⇥");
await p.screenshot({ path: `${OUT}/11_advice.png` });
await p.keyboard.press("Tab");
await pause(p, 300);
check((await val(p)) === "line simple.csv x=time y=temp + y=pressure axis=right", "⇥ puts the fix in the box");
check((await calls(p, "preview")).at(-1)[1].command === "line simple.csv x=time y=temp + y=pressure axis=right" && (await p.isHidden("#advice")), "… and draws it; the hint is gone");
await p.keyboard.press("Meta+z");
await pause(p, 300);
check((await val(p)) === "line simple.csv x=time y=temp,pressure", "⌘Z gives the command back as it was");
await p.keyboard.press("Escape");
await p.evaluate(() => window.__emit("launcher-shown"));
await pause(p, 250);
await p.keyboard.type("line simple.csv x=time y=temp legend=br");
await p.keyboard.press("Enter");
await pause(p, 300);
check((await text(p, "#advice")).startsWith("hint: panel 1") && !(await text(p, "#advice")).includes("⇥"), "a hint without a fix has no ⇥");

// --- round 12: Try it opens in one click; an option typed during the picker ---
{
  const q = await open();
  // The launcher opening (which may reset it) and the lesson's command arrive together.
  await q.evaluate(() => {
    window.__emit("launcher-shown");
    window.__emit("launcher-command", "line simple.csv x=time y=temp");
  });
  await pause(q, 700);
  check(await val(q) === "line simple.csv x=time y=temp" && await q.isVisible("#preview"),
    `Try it: one click fills the box and draws it (${await val(q)})`);
  const q2 = await open();
  await q2.keyboard.type("simple");
  await pause(q2);
  await q2.keyboard.press("Enter"); // the file
  await pause(q2);
  await q2.keyboard.press("Enter"); // line
  await pause(q2);
  await q2.keyboard.press("Enter"); // x = row number
  await pause(q2);
  await q2.keyboard.type("c=");
  await pause(q2, 300);
  check((await val(q2)).startsWith("line simple.csv") && (await val(q2)).endsWith(" c=") && await q2.isVisible("#list"),
    `an option typed in the picker carries on in command mode: ${await val(q2)}`);
  await q.close(); await q2.close();
}

// --- round 13: the caret on a long command; the live preview follows the text at once ---
{
  const q = await open();
  await q.keyboard.press("Meta+e");
  await pause(q);
  // The Mac's text box keeps a few pixels at the line end for the caret; stand in for that
  // with padding, and the coloured copy must fold its lines in the same places.
  await q.addStyleTag({ content: ".field textarea { padding-right: 4px !important; }" });
  const long = 'line "Projects/Personal Projects/EasyPlots/tests/fixtures/run_a.csv" x=t y=temp + "Projects/Personal Projects/EasyPlots/tests/fixtures/run_b.csv" x=t y=temp cross';
  let mismatched = 0;
  for (let n = long.length - 30; n <= long.length; n++) {
    await q.fill("#query", long.slice(0, n));
    await q.waitForTimeout(20);
    const [a, b] = await q.evaluate(() => [document.querySelector("#query").scrollHeight, document.querySelector("#mirror .mirror-text").offsetHeight]);
    if (a !== b) mismatched++;
  }
  check(mismatched === 0, `the coloured copy folds where the text box does (${mismatched} lengths off)`);
  const trim = await q.$eval("#mirror", (m) => m.style.paddingRight);
  check(trim !== "" && trim !== "0px", `…by narrowing itself to match (${trim})`);
  await q.screenshot({ path: `${OUT}/r13_long_command.png` });
  // Typed, drawn small; emptied, the old drawing goes at once (owner: it stayed).
  await q.fill("#query", "line simple.csv x=time y=temp");
  await q.keyboard.type(" ");
  await pause(q, 400);
  check(await q.isVisible("#live"), "the live preview draws the command");
  await q.fill("#query", "");
  await q.keyboard.type("l");
  await q.keyboard.press("Backspace");
  await pause(q, 50);
  check(await q.isHidden("#live"), "an empty box has no live preview");
  await q.fill("#query", "line simple.csv x=time y=temp");
  await q.keyboard.type(" ");
  await pause(q, 400);
  await q.keyboard.press("Meta+e"); // to the picker's file step
  await pause(q, 100);
  check(await q.isHidden("#live"), "the picker's file step shows no leftover drawing (owner's bug)");
  await q.close();
}

// --- round 14: WebKit (the Mac's web view) didn't fold a line back up when a word got
// shorter (owner's recording: `color=` wrapped, ⌫ back to `colo`, the caret a line low). ---
{
  const wk = await webkit.launch();
  const q = await open("mocha", wk);
  await q.keyboard.press("Meta+e");
  await pause(q);
  const base = 'line "Projects/Personal Projects/EasyPlots/tests/fixtures/run_a.csv" x=t y=temp + "Projects/Personal Projects/EasyPlots/tests/fixtures/run_b.csv" x=t y=temp';
  let off = 0;
  for (let w = 655; w < 680; w += 1) {
    await q.setViewportSize({ width: w, height: 500 });
    await q.fill("#query", base);
    await q.focus("#query");
    await q.keyboard.press("End");
    await q.keyboard.type(" color=", { delay: 5 });
    for (let k = 0; k < 3; k++) {
      await q.keyboard.press("Backspace");
      await q.waitForTimeout(30);
      // The box against a fresh one holding the same text: the same number of lines.
      const [box, copy] = await q.evaluate(() => {
        const box = document.querySelector("#query");
        const fresh = box.cloneNode();
        fresh.value = box.value;
        fresh.style.cssText = getComputedStyle(box).cssText;
        fresh.style.position = "absolute";
        fresh.style.width = `${box.clientWidth}px`;
        fresh.style.height = "10px";
        box.parentElement.append(fresh);
        const n = fresh.scrollHeight;
        fresh.remove();
        return [box.scrollHeight, n];
      });
      if (box !== copy) off++;
    }
  }
  check(off === 0, `WebKit: the text box folds back with the coloured copy while deleting (${off} times off)`);
  await wk.close();
}

await browser.close();
server.kill();
console.log(failures ? `${failures} FAILED` : "all passed");
process.exit(failures ? 1 : 0);
