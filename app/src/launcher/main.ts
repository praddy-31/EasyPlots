// The launcher: a prompt line and a list, fuzzel style, in four modes.
//
// - picker: file → plot type → only the columns that type needs (hist: y; heatmap: x, y,
//   z…). The command being built shows above the prompt; at the end it moves into the box
//   (command mode).
// - command: the typed language from docs/SPEC.md, coloured as you type, with suggestions
//   for the word under the cursor.
// - preview: ↵ draws the plot in the (enlarged) window without saving. ↵ again or ⌘S saves
//   to the plots folder, ⌘⇧S asks where, ⌘C copies the image. Typing edits the command.
// - saved: like preview, plus ↵ opens the file and ⌘↵ shows it in Finder.
//
// Colours come from the theme (Palette's current theme, or a built-in one) as CSS variables
// named after Palette's keys. The Rust side does all the real work; this file builds text.
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";

type Mode = "picker" | "command" | "preview" | "saved" | "name" | "browse";
type Stage = "file" | "type" | "x" | "y" | "group" | "z" | "style";

/** Nerd Font icons (code points must be in assets/fonts/ui/make_ui_fonts.py). */
const ICON = {
  file: "",
  chart: "",
  line: "",
  scatter: "",
  bar: "",
  area: "",
  numbers: "",
  dates: "",
  text: "",
  row: "",
  check: "",
  history: "",
  command: "",
  colour: "",
  option: "",
  saved: "",
  image: "",
  folder: "",
};

interface Item {
  label: string;
  detail?: string;
  /** What choosing the item puts into the command. */
  value: string | null;
  icon?: string;
  /** CSS colour for the icon, e.g. "var(--green)". */
  iconColor?: string;
  swatch?: string;
  checked?: boolean;
  /** Shown but not choosable (hints, planned plot types). */
  note?: boolean;
  /** Command mode: the text range this suggestion replaces. */
  range?: [number, number];
  /** File step: the file (and workbook sheet) behind the entry. */
  hit?: api.FileHit;
  /** Files: the folder, shown grey after the name and shortened from its start. */
  folder?: string;
}

/** `Documents/projects/run_a.csv › Run2` → name `run_a.csv › Run2`, folder `Documents/projects`. */
function splitPath(display: string): { label: string; folder?: string } {
  const sheetAt = display.indexOf(" › ");
  const path = sheetAt >= 0 ? display.slice(0, sheetAt) : display;
  // A folder entry (`lab/runs/`): its own name keeps the slash.
  const slash = path.lastIndexOf("/", path.endsWith("/") ? path.length - 2 : path.length);
  if (slash < 0) return { label: display };
  return { label: display.slice(slash + 1), folder: path.slice(0, slash + 1) };
}

interface Layer {
  file: string; // as inserted (quoted if needed)
  display: string; // as shown and looked up
  x: string | null; // null = row number
  xKind: string | null;
  ys: string[];
  kind: string | null;
  z: string | null;
  /** heatmap / contour of the whole file as a table (no x, y or z). */
  table: boolean;
  /** A saved style chosen at the end (`--style=NAME`). */
  style: string | null;
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const panel = $<HTMLDivElement>("panel");
const query = $<HTMLTextAreaElement>("query");
const mirror = $<HTMLDivElement>("mirror");
const field = query.parentElement as HTMLDivElement;
const list = $<HTMLUListElement>("list");
const results = list.parentElement as HTMLDivElement;
const live = $<HTMLImageElement>("live");
const peek = $<HTMLDivElement>("peek");
const crumb = $<HTMLDivElement>("crumb");
const promptIcon = $<HTMLSpanElement>("prompt-icon");
const stageLabel = $<HTMLSpanElement>("stage");
const errorBox = $<HTMLDivElement>("error");
const errorText = $<HTMLDivElement>("error-text");
const previewBox = $<HTMLDivElement>("preview");
const grid = $<HTMLDivElement>("grid");
const plotImage = $<HTMLImageElement>("plot");
const status = $<HTMLDivElement>("status");
const adviceLine = $<HTMLDivElement>("advice");
/** The plot advisor's first hint for the shown preview (⇥ applies its fix). */
let advice: api.Advice | null = null;
const footer = $<HTMLElement>("footer");
const hints = $<HTMLSpanElement>("hints");
const warning = $<HTMLSpanElement>("warning");

const KINDS: [string, string][] = [
  ["line", "lines through the points"],
  ["scatter", "points only"],
  ["bar", "bars from zero"],
  ["step", "steps between the points"],
  ["area", "filled down to zero"],
  ["hist", "histogram of the x column(s)"],
  ["box", "box plot: median, quartiles, outliers"],
  ["kde", "smooth density of the x column(s)"],
  ["violin", "box plot with the density's shape"],
  ["strip", "every value as a dot, per group"],
  ["heatmap", "a table of numbers as coloured cells"],
  ["contour", "lines of equal value over a grid"],
  ["polar", "angle (x) and radius (y) on a round plot"],
  ["fft", "spectrum: how strong each frequency is (x = time)"],
  ["resid", "residuals: each point's distance from its fit (add fit=)"],
  ["qq", "normal Q–Q plot of a fit's residuals (add fit=)"],
];
/** Icon and colour per plot type. */
const KIND_LOOK: Record<string, [string, string]> = {
  line: [ICON.line, "var(--cyan)"],
  scatter: [ICON.scatter, "var(--magenta)"],
  bar: [ICON.bar, "var(--orange)"],
  step: [ICON.line, "var(--blue)"],
  area: [ICON.area, "var(--green)"],
  hist: [ICON.bar, "var(--yellow)"],
  box: [ICON.option, "var(--red)"],
  kde: [ICON.area, "var(--cyan)"],
  violin: [ICON.option, "var(--magenta)"],
  strip: [ICON.scatter, "var(--green)"],
  heatmap: [ICON.file, "var(--red)"],
  contour: [ICON.chart, "var(--green)"],
  polar: [ICON.chart, "var(--blue)"],
  fft: [ICON.bar, "var(--magenta)"],
  resid: [ICON.scatter, "var(--yellow)"],
  qq: [ICON.chart, "var(--yellow)"],
};
const PLANNED_KINDS: [string, string, string][] = [];
const TYPE_WORDS = new Set(KINDS.map((k) => k[0]));
const TYPE_START = new RegExp(`^(${KINDS.map((k) => k[0]).join("|")}|again|preset)\\s`, "i");

let mode: Mode = "picker";
let stage: Stage = "file";
let layers: Layer[] = [];
let current: Layer | null = null;
let columns: api.ColumnInfo[] = [];
let items: Item[] = [];
let selected = 0;
let info: api.LauncherInfo | null = null;
let themeStamp = -1;
let previewed = ""; // the command currently shown as a preview
let lastSaved: api.SaveResult | null = null;
let errorSpan: [number, number] | null = null;
/** A ready fix for the shown error, valid while the command is unchanged. */
let errorFix: { start: number; end: number; text: string; command: string } | null = null;
let busy = false;
let requestId = 0;

// ---------- helpers ----------

function escapeHtml(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}

/** Fuzzy filter: letters in order, best when they start early and sit close together. */
function fuzzy<T>(q: string, things: T[], text: (t: T) => string): T[] {
  const needle = q.trim().toLowerCase();
  if (!needle) return things;
  const scored: [T, number][] = [];
  for (const t of things) {
    const hay = text(t).toLowerCase();
    let pos = -1;
    let score = 0;
    let ok = true;
    for (const ch of needle) {
      const next = hay.indexOf(ch, pos + 1);
      if (next < 0) {
        ok = false;
        break;
      }
      score += next - pos - 1;
      pos = next;
    }
    if (ok) scored.push([t, score + (hay.startsWith(needle) ? -100 : 0)]);
  }
  return scored.sort((a, b) => a[1] - b[1]).map(([t]) => t);
}

/** Edit distance where swapping two neighbouring letters counts as one typo. */
function distance(a: string, b: string): number {
  const d: number[][] = Array.from({ length: a.length + 1 }, (_, i) =>
    Array.from({ length: b.length + 1 }, (_, j) => (i === 0 ? j : j === 0 ? i : 0)),
  );
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost);
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1);
      }
    }
  }
  return d[a.length][b.length];
}

/** No fuzzy match? The closest spellings instead (a likely typo). */
function closest<T>(q: string, things: T[], text: (t: T) => string, limit = 4): T[] {
  const needle = q.trim().toLowerCase();
  if (!needle) return [];
  const allowed = Math.max(2, Math.floor(needle.length / 2));
  return things
    .map((t) => [t, distance(needle, text(t).toLowerCase())] as [T, number])
    .filter(([, d]) => d <= allowed)
    .sort((a, b) => a[1] - b[1])
    .slice(0, limit)
    .map(([t]) => t);
}

/** The command text for the finished layers plus the one being picked. */
function buildCommand(all: Layer[]): string {
  const parts: string[] = [];
  all.forEach((l, i) => {
    if (i === 0) parts.push(l.kind ?? "…");
    else {
      parts.push("+");
      if (l.kind && l.kind !== all[0].kind) parts.push(l.kind);
    }
    parts.push(l.file);
    // Each step only asks for what its plot type uses, so whatever is set goes in.
    if (l.x) parts.push(`x=${l.x}`);
    // Histograms and kde count values along x, so their column(s) go in x= (owner's note).
    if (l.ys.length) parts.push(`${countsX(l.kind) ? "x" : "y"}=${l.ys.join(",")}`);
    if (l.z) parts.push(`z=${l.z}`);
  });
  const style = all.find((l) => l.style)?.style;
  if (style) parts.push(`--style=${style}`);
  return parts.join(" ");
}

/** The column steps a plot type needs after the file and the type. */
function stepsFor(kind: string | null): Stage[] {
  switch (kind) {
    case "hist":
    case "kde":
      return ["y"];
    case "box":
    case "violin":
    case "strip":
      // Grouping is by a text column; without one there is nothing to ask.
      return columns.some((c) => c.kind === "text") ? ["y", "group"] : ["y"];
    case "heatmap":
    case "contour":
      return ["x", "y", "z"];
    default:
      return ["x", "y"];
  }
}

/** hist and kde: the one column step is the values on x (kept as `ys` inside). */
function countsX(kind: string | null): boolean {
  return kind === "hist" || kind === "kde";
}

/** Can the y step tick several columns (one line, box or histogram each)? */
function manyYs(kind: string | null): boolean {
  return !["heatmap", "contour"].includes(kind ?? "");
}

// ---------- colouring the command ----------

/** Split into words like the parser does (quotes keep spaces in), with their positions. */
function words(text: string): [number, number][] {
  const out: [number, number][] = [];
  let start = -1;
  let quote = "";
  for (let i = 0; i <= text.length; i++) {
    const c = text[i];
    if (c === undefined || (!quote && /\s/.test(c))) {
      if (start >= 0) out.push([start, i]);
      start = -1;
      continue;
    }
    if (start < 0) start = i;
    if (quote && c === quote) quote = "";
    else if (!quote && (c === '"' || c === "'")) quote = c;
  }
  return out;
}

function highlight(text: string): string {
  const cls: string[] = new Array(text.length).fill("");
  let expectType = true;
  let expectFile = false;
  for (const [a, b] of words(text)) {
    const w = text.slice(a, b);
    let c = "";
    if (w === "+" || w === ";") {
      c = "tok-plus";
      expectType = true;
      expectFile = w === "+";
    } else if (expectType && TYPE_WORDS.has(w.toLowerCase())) {
      c = "tok-type";
      expectType = false;
      expectFile = true;
    } else if (w.startsWith("--")) {
      c = "tok-flag";
    } else if (w.includes("=")) {
      const eq = text.indexOf("=", a);
      for (let i = a; i <= eq; i++) cls[i] = "tok-key";
      expectType = false;
      continue;
    } else if (expectFile) {
      c = "tok-file";
      expectFile = false;
      expectType = false;
    } else if ((info?.flags ?? ["markers"]).includes(w.toLowerCase())) {
      c = "tok-flag";
    }
    for (let i = a; i < b; i++) cls[i] = c;
  }
  if (errorSpan) {
    const [s, e] = errorSpan;
    for (let i = s; i < Math.max(e, s + 1) && i < text.length; i++) cls[i] = "tok-err";
  }
  let html = "";
  let run = "";
  let runCls = cls[0] ?? "";
  for (let i = 0; i <= text.length; i++) {
    if (i === text.length || cls[i] !== runCls) {
      html += runCls ? `<span class="${runCls}">${escapeHtml(run)}</span>` : escapeHtml(run);
      run = "";
      runCls = cls[i] ?? "";
    }
    if (i < text.length) run += text[i];
  }
  return html;
}

/** The box grows with a long command, one to three lines (then it scrolls inside). */
const MAX_LINES = 3;
function fitLines() {
  const line = parseFloat(getComputedStyle(query).lineHeight) || 24;
  field.style.height = `${line}px`;
  const lines = Math.min(MAX_LINES, Math.max(1, Math.round(query.scrollHeight / line)));
  field.style.height = `${lines * line}px`;
}

/** Scroll a long command so the cursor is in view (setting the text doesn't, in WebKit). */
function revealCaret() {
  fitLines();
  const at = query.selectionEnd ?? query.value.length;
  if (at >= query.value.length) query.scrollTop = query.scrollHeight;
  else {
    // Measure the wrapped text before the cursor with the mirror's font and width.
    const probe = document.createElement("div");
    probe.style.cssText = `position:absolute;visibility:hidden;white-space:pre-wrap;overflow-wrap:anywhere;width:${query.clientWidth}px;line-height:${getComputedStyle(query).lineHeight}`;
    probe.textContent = query.value.slice(0, at) || " ";
    field.append(probe);
    const y = probe.offsetHeight;
    probe.remove();
    if (y > query.scrollTop + query.clientHeight || y - query.clientHeight / MAX_LINES < query.scrollTop) {
      query.scrollTop = Math.max(0, y - query.clientHeight);
    }
  }
  mirror.scrollTop = query.scrollTop;
}

function syncMirror() {
  if (mode === "picker") {
    field.classList.add("plain");
    fitLines();
    return;
  }
  field.classList.remove("plain");
  refold();
  mirror.innerHTML = `<div class="mirror-text">${highlight(query.value)}</div>`;
  fitLines();
  alignMirror();
  mirror.scrollTop = query.scrollTop;
}

/** WebKit's text box doesn't always fold its lines again when a word gets shorter: after
 *  `color=` wrapped onto a new line, deleting back to `colo` left it there, while the
 *  coloured copy moved it up, so the caret showed a line below the text (owner's
 *  recording, round 14; reproduced in Playwright's WebKit). Nudging the width makes it
 *  lay the text out afresh. */
function refold() {
  query.style.width = "calc(100% - 1px)";
  void query.offsetWidth;
  query.style.width = "";
  void query.offsetWidth;
}

/** The coloured copy must fold its lines where the text box does, or the caret shows in
 *  the wrong place (owner, round 13: a 4-line command, the caret a line off while
 *  deleting). The Mac's text box keeps a pixel or two at the line end for the caret,
 *  which a plain block doesn't, so a line that just fits in one folds in the other.
 *  Narrow the copy a pixel at a time until both have the same number of lines. */
let mirrorTrim = 0;
function alignMirror() {
  const lines = () => {
    const text = mirror.firstElementChild as HTMLElement | null;
    return text ? text.offsetHeight : 0;
  };
  if (!query.value) return;
  for (let i = 0; i < 6; i++) {
    const want = query.scrollHeight;
    const have = lines();
    if (have < want && mirrorTrim < 6) mirrorTrim++;
    else if (have > want && mirrorTrim > 0) mirrorTrim--;
    else break;
    mirror.style.paddingRight = `${mirrorTrim}px`;
  }
}

// ---------- theme and appearance ----------

function hexToRgb(hex: string): [number, number, number] {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function applyTheme(theme: api.ThemeInfo) {
  themeStamp = theme.stamp;
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const t = dark ? theme.dark : theme.light;
  const root = document.documentElement.style;
  for (const [key, value] of Object.entries(t.colors)) {
    root.setProperty(`--${key.replace(/_/g, "-")}`, value);
  }
  const [r, g, b] = hexToRgb(t.colors.background ?? "#1e1e2e");
  // Opacity: Palette's glass when it sets one, else [launcher] opacity. No lower limit.
  root.setProperty("--panel", `rgba(${r}, ${g}, ${b}, ${theme.look?.opacity ?? info?.ui.opacity ?? 0.9})`);
  root.colorScheme = t.mode;
}

function applyAppearance(ui: api.UiSettings) {
  const root = document.documentElement.style;
  root.setProperty("--font", `"${ui.font}", "JetBrains Mono", ui-monospace, Menlo, monospace`);
  root.setProperty("--font-size", `${ui.font_size}px`);
  footer.hidden = !ui.hints && !(info?.warnings.length);
  hints.hidden = !ui.hints;
  applyTheme(ui.theme);
}

// Follow a Palette theme switch while the launcher is in front.
setInterval(async () => {
  if (!document.hasFocus()) return;
  const t = await api.currentTheme();
  if (t.stamp !== themeStamp) applyTheme(t);
}, 2000);
window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (info) applyTheme(info.ui.theme);
});

// ---------- drawing ----------

function renderList() {
  list.innerHTML = "";
  items.forEach((item, i) => {
    const li = document.createElement("li");
    if (item.note) li.className = "note";
    else if (i === selected) li.className = "selected";
    const check = mode === "picker" && stage === "y" && !item.note
      ? `<span class="nf check" aria-hidden="true">${item.checked ? ICON.check : ""}</span>`
      : "";
    const icon = item.swatch
      ? `<span class="swatch" style="background:${escapeHtml(item.swatch)}"></span>`
      : `<span class="nf icon" aria-hidden="true" style="color:${item.iconColor ?? "var(--accent)"}">${item.icon ?? ""}</span>`;
    li.innerHTML = `${check}${icon}<span class="label">${escapeHtml(item.label)}</span>` +
      (item.folder ? `<span class="folder"><bdi>${escapeHtml(item.folder)}</bdi></span>` : "") +
      (item.detail ? `<span class="detail">${escapeHtml(item.detail)}</span>` : "");
    li.addEventListener("mousedown", (e) => {
      e.preventDefault();
      if (item.note) return;
      selected = i;
      void choose();
    });
    list.appendChild(li);
  });
  list.querySelector("li.selected")?.scrollIntoView({ block: "nearest" });
  schedulePeek();
}

// ---------- a quick look at the highlighted file's first rows ----------

let peekTimer = 0;
let peekSeq = 0;
let peekFor = "";

/** Show the first rows of the highlighted file beside the list (file step only). */
function schedulePeek() {
  clearTimeout(peekTimer);
  const hit = mode === "picker" && stage === "file" ? items[selected]?.hit : undefined;
  if (!hit || hit.guess) {
    peekSeq++;
    peekFor = "";
    peek.hidden = true;
    return;
  }
  const key = `${hit.file}\u0000${hit.sheet ?? ""}`;
  if (key === peekFor) return;
  peekTimer = window.setTimeout(async () => {
    const seq = ++peekSeq;
    try {
      const t = await api.peekFile(hit.file, hit.sheet);
      if (seq !== peekSeq) return; // the highlight has moved on
      peekFor = key;
      const head = t.names.map((n, i) => `<th class="${t.kinds[i]}">${escapeHtml(n)}</th>`).join("");
      const body = t.cells
        .map((r) => `<tr>${r.map((c, i) => `<td class="${t.kinds[i]}">${escapeHtml(c)}</td>`).join("")}</tr>`)
        .join("");
      const more = t.rows > t.cells.length ? `first ${t.cells.length} of ${t.rows} rows` : `${t.rows} rows`;
      peek.innerHTML = `<div class="peek-title">${more} · ${t.names.length} columns</div>` +
        `<table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table>`;
      peek.hidden = false;
    } catch {
      // Not readable, or outside the data folder (opened only when picked): no table.
      if (seq === peekSeq) {
        peekFor = "";
        peek.hidden = true;
      }
    }
  }, 180);
}

function setHints(pairs: [string, string][]) {
  hints.innerHTML = pairs.map(([k, t]) => `<span><kbd>${k}</kbd> ${t}</span>`).join("");
}

function setPrompt(icon: string, word: string) {
  promptIcon.textContent = icon;
  stageLabel.textContent = word;
}

function showError(e: api.PlotError | null, command = "") {
  if (!e) {
    errorFix = null;
    errorBox.hidden = true;
    panel.classList.remove("has-error");
    errorSpan = null;
    syncMirror();
    return;
  }
  errorSpan = e.start !== null && e.end !== null && command ? [e.start, e.end] : null;
  errorFix = e.fix && command ? { ...e.fix, command } : null;
  const hint = errorFix ? `⇥ puts in <b>${escapeHtml(errorFix.text)}</b>, then ↵` : "fix it and press ↵ again";
  errorText.innerHTML = `${escapeHtml(e.message)}<br><span class="hint">${hint}</span>`;
  errorBox.hidden = false;
  panel.classList.add("has-error");
  syncMirror();
}

/** ⇥ on an error that has a ready fix (quotes round a name with spaces): put it in. */
function applyErrorFix() {
  if (!errorFix) return;
  const { start, end, text, command } = errorFix;
  query.value = command.slice(0, start) + text + command.slice(end);
  const caret = start + text.length;
  query.setSelectionRange(caret, caret);
  showError(null);
  revealCaret();
  void refreshCommand();
  changed(false);
  scheduleLive();
}

function updateCrumb() {
  if (mode === "picker") {
    const all = current ? [...layers, current] : layers;
    crumb.innerHTML = all.length
      ? escapeHtml(buildCommand(all))
      : `${escapeHtml(info?.data_dir ?? "your data folder")} · type a command such as <b>line data.csv</b> to skip picking`;
  } else if (mode === "command") {
    crumb.textContent = query.value.trim() ? "command" : "recent commands";
  } else {
    crumb.textContent = mode === "saved" ? "saved" : mode === "browse" ? `saved plots in your plots folder · ${plots.length}` : "preview, not saved yet";
  }
}

function moveSelection(step: number) {
  const n = items.length;
  if (!n || items.every((i) => i.note)) return;
  do selected = (selected + step + n) % n;
  while (items[selected].note);
  renderList();
  if (mode === "picker") scheduleLive();
}

// ---------- picker mode ----------

const BACK: [string, string][] = [["↵", "choose"], ["⌫", "back"], ["esc", "close"]];
const STAGE_HINTS: Record<Stage, [string, string][]> = {
  file: [["↵", "choose"], ["⌘E", "type a command · recent ones"], ["⌘P", "pin"], ["esc", "close"]],
  type: BACK,
  x: BACK,
  y: [["⇥", "tick"], ["↵", "done"], ["⌫", "back"], ["esc", "close"]],
  group: BACK,
  z: BACK,
  style: BACK,
};
const PLACEHOLDER: Record<Stage, string> = {
  file: "search CSV files…",
  type: "plot type",
  x: "x column",
  y: "y column(s)",
  group: "group by (a text column)",
  z: "value column (the colour)",
  style: "a saved look (⌘D on a preview keeps one)",
};

/** Start picking from scratch (first stage: file). */
async function startPicker() {
  layers = [];
  current = null;
  overlayFiles = [];
  showError(null);
  clip = await api.clipboardTable().catch(() => null);
  await enterStage("file");
}

async function enterStage(s: Stage) {
  await leavePreview();
  mode = "picker";
  stage = s;
  query.value = "";
  const asX = s === "y" && countsX(current?.kind ?? null);
  query.placeholder = asX ? "x column(s): the values to count" : PLACEHOLDER[s];
  setPrompt(ICON.chart, asX ? "x" : s);
  selected = 0;
  syncMirror();
  updateCrumb();
  setHints(s === "y" && !manyYs(current?.kind ?? null) ? BACK : STAGE_HINTS[s]);
  await refreshPicker();
  changed(false);
}

function columnItem(c: api.ColumnInfo, checked = false): Item {
  const look = {
    numbers: [ICON.numbers, "var(--blue)"],
    dates: [ICON.dates, "var(--magenta)"],
    text: [ICON.text, "var(--yellow)"],
  }[c.kind];
  // The icon shows the kind; the detail what's in the column, so messy ones stand out.
  return { label: c.name, detail: c.summary ?? c.kind, value: c.insert, icon: look[0], iconColor: look[1], checked };
}

/** The columns a step offers. */
function stagePool(s: Stage): api.ColumnInfo[] {
  const l = current;
  const numbers = columns.filter((c) => c.kind === "numbers");
  switch (s) {
    case "x":
      return columns;
    case "y":
      return numbers.filter((c) => c.insert !== l?.x);
    case "group":
      return columns.filter((c) => c.kind === "text");
    case "z":
      return numbers.filter((c) => c.insert !== l?.x && !l?.ys.includes(c.insert));
    default:
      return [];
  }
}

const NO_GROUPS = "\u0000none";
const NO_STYLE = "\u0000nostyle";
const CLIPBOARD = "\u0000clipboard";

/** The clipboard's table (rows × columns), looked at when the file step opens. */
let clip: { rows: number; columns: number } | null = null;

/** Quote a file name for the command if it has spaces or separators. */
const quoteName = (s: string) => (/[\s,"';]/.test(s) ? `"${s.replace(/"/g, '\\"')}"` : s);
const WHOLE_TABLE = "\u0000table";
const PRESET = "\u0000preset:";

async function refreshPicker() {
  const q = query.value;
  const id = ++requestId;
  if (stage === "file") {
    const hits = await api.searchFiles(q);
    if (id !== requestId) return;
    items = hits.map((h) => ({
      ...splitPath(h.display),
      value: h.insert,
      icon: h.folder ? ICON.folder : ICON.file,
      iconColor: h.folder ? "var(--yellow)" : "var(--green)",
      detail: h.guess ? "did you mean?" : h.folder ? "folder · one panel per file" : h.sheet ? "sheet" : undefined,
      hit: h,
    }));
    // A table copied in Excel or Numbers: first in the list (kept as a file when chosen).
    const word = q.trim().toLowerCase();
    if (clip && (!word || "clipboard".startsWith(word) || "paste".startsWith(word))) {
      items.unshift({
        label: "Clipboard",
        detail: `${clip.rows} rows × ${clip.columns} columns · pasted`,
        value: CLIPBOARD,
        icon: ICON.file,
        iconColor: "var(--yellow)",
      });
    }
    if (!items.length) {
      items = [{
        label: info?.scanning ? "Looking for CSV files…" : `No CSV files match in ${info?.data_dir ?? "the data folder"}`,
        value: null,
        note: true,
      }];
    }
  } else if (stage === "type") {
    const first = layers[0]?.kind;
    let kinds = fuzzy(q, KINDS, (k) => k[0]);
    const guessing = !kinds.length && q.trim() !== "";
    if (guessing) kinds = closest(q, KINDS, (k) => k[0]);
    items = kinds.map(([k, d]) => ({
      label: k,
      detail: guessing ? "did you mean?" : first && k === first ? `${d} (same as first)` : d,
      value: k,
      icon: KIND_LOOK[k][0],
      iconColor: KIND_LOOK[k][1],
    }));
    if (!q.trim()) {
      for (const [k, icon, m] of PLANNED_KINDS) {
        items.push({ label: k, detail: `coming in ${m}`, value: null, icon, iconColor: "var(--dark-foreground)", note: true });
      }
      // Presets at the end: the same plot with this file (↑ from the top reaches them).
      items.push(...(info?.presets ?? []).map((p) => ({
        label: `preset: ${p.name}`,
        detail: p.command,
        value: PRESET + p.name,
        icon: ICON.history,
        iconColor: "var(--yellow)",
      })));
    }
  } else if (stage === "style") {
    const names = fuzzy(q, info?.styles ?? [], (n) => n);
    items = names.map((n) => ({ label: n, detail: `--style=${n}`, value: n, icon: ICON.colour, iconColor: "var(--magenta)" }));
    if (!q.trim()) {
      items.unshift({ label: "(no style)", detail: "the usual look", value: NO_STYLE, icon: ICON.row, iconColor: "var(--light-foreground)" });
    }
    if (!items.length) items = [{ label: `No style like "${q}"`, value: null, note: true }];
  } else {
    const pool = stagePool(stage);
    let found = fuzzy(q, pool, (c) => c.name);
    const guessing = !found.length && q.trim() !== "";
    if (guessing) found = closest(q, pool, (c) => c.name);
    const ticked = new Set(current?.ys ?? []);
    items = found.map((c) => {
      const item = columnItem(c, stage === "y" && ticked.has(c.insert));
      if (guessing) item.detail = "did you mean?";
      return item;
    });
    const kind = current?.kind ?? null;
    if (!q.trim()) {
      if (stage === "x" && (kind === "heatmap" || kind === "contour")) {
        items.unshift({ label: "(whole table)", detail: "the file is a grid of numbers", value: WHOLE_TABLE, icon: ICON.file, iconColor: "var(--light-foreground)" });
      } else if (stage === "x") {
        items.unshift({ label: "(row number)", detail: "1, 2, 3…", value: null, icon: ICON.row, iconColor: "var(--light-foreground)" });
      } else if (stage === "group") {
        items.unshift({ label: "(no groups)", detail: "one box per y column", value: NO_GROUPS, icon: ICON.row, iconColor: "var(--light-foreground)" });
      } else if (stage === "y" && manyYs(kind) && pool.length > 1) {
        // Several y columns give one line (box, histogram) each; say how, since ↵ takes one.
        const n = ticked.size;
        items.unshift({
          label: n ? `${n} ticked: ⇥ tick another, ↵ when done` : "⇥ ticks a column: tick several for one each",
          value: null,
          icon: ICON.check,
          iconColor: "var(--light-foreground)",
          note: true,
        });
      }
    }
    if (!items.length) {
      items = [{
        label: !pool.length ? "No number columns left in this file" : `No column like "${q}"`,
        value: null,
        note: true,
      }];
    }
  }
  selected = Math.min(selected, Math.max(0, items.length - 1));
  if (items[selected]?.note) selected = Math.max(0, items.findIndex((i) => !i.note));
  renderList();
  scheduleLive();
}

/** Put the chosen item into a layer, for the current step. */
function applyChoice(l: Layer, s: Stage, value: string | null) {
  if (s === "type") l.kind = value;
  else if (s === "x" && value === WHOLE_TABLE) l.table = true;
  else if (s === "x") {
    l.x = value;
    l.xKind = columns.find((c) => c.insert === value)?.kind ?? null;
  } else if (s === "y" && !l.ys.length && value) l.ys = [value];
  else if (s === "group") l.x = value === NO_GROUPS ? null : value;
  else if (s === "z") l.z = value;
  else if (s === "style") l.style = value === NO_STYLE ? null : value;
}

/** The step after this one, or null when the layer is complete. */
function nextStage(): Stage | null {
  if (!current) return null;
  const order = stageOrder(current);
  return order[order.indexOf(stage) + 1] ?? null;
}

async function choosePicker(item: Item) {
  if (item.note) return;
  if (stage === "file" && item.value === CLIPBOARD) {
    // Keep the pasted table as a file, then carry on as if that file was picked.
    try {
      const file = await api.saveClipboard();
      const insert = quoteName(file);
      await choosePicker({ label: file, value: insert, hit: { display: file, insert, file, sheet: null, guess: false } });
    } catch (e) {
      showError(e as api.PlotError);
    }
    return;
  }
  if (stage === "file" && item.value) {
    try {
      columns = (await api.listColumns(item.hit?.file ?? item.label, item.hit?.sheet ?? null)).columns;
    } catch (e) {
      showError(e as api.PlotError);
      return;
    }
    showError(null);
    current = { file: item.value, display: item.hit?.file ?? item.label, x: null, xKind: null, ys: [], kind: null, z: null, table: false, style: null };
    await enterStage("type");
    return;
  }
  if (!current) return;
  if (stage === "type" && item.value?.startsWith(PRESET)) {
    try {
      const command = await api.applyPreset(item.value.slice(PRESET.length), current.display);
      current = null;
      await enterCommand(command + " ");
    } catch (e) {
      showError(e as api.PlotError);
    }
    return;
  }
  if (stage === "type" && !item.value) return;
  applyChoice(current, stage, item.value);
  if (stage === "y" && !current.ys.length) return;
  const next = nextStage();
  if (next) {
    await enterStage(next);
    return;
  }
  layers.push(current);
  // Several files dropped on the icon: the others get the same columns, overlaid (+).
  for (const file of overlayFiles) layers.push({ ...current, ys: [...current.ys], file, display: file });
  overlayFiles = [];
  current = null;
  // Hand over to command mode with the finished command, ready for options or ↵.
  await enterCommand(buildCommand(layers) + " ");
}

/** All the steps for this layer: file, type, its columns, then a style if any are saved. */
function stageOrder(l: Layer): Stage[] {
  const columns = l.table ? stepsFor(l.kind).slice(0, 1) : stepsFor(l.kind);
  return ["file", "type", ...columns, ...(info?.styles.length ? ["style" as Stage] : [])];
}

/** Files dropped on the menu-bar icon after the first: overlaid with the same columns. */
let overlayFiles: string[] = [];

/** The first dropped file is picked (next: the plot type); the rest are overlaid at the end. */
async function pickDropped(files: string[]) {
  const [first, ...rest] = files.map(quoteName);
  overlayFiles = rest;
  await choosePicker({ label: files[0], value: first, hit: { display: files[0], insert: first, file: files[0], sheet: null, guess: false } });
  if (rest.length) crumb.textContent = `${buildCommand([current!])} + ${rest.length} more file${rest.length > 1 ? "s" : ""}`;
}

/** ⌫ in an empty box: one step back, forgetting that step's choice. */
async function backPicker() {
  if (!current) return;
  const order = stageOrder(current);
  const back = order[order.indexOf(stage) - 1];
  if (!back) return;
  current.style = null;
  if (back === "file") {
    current = null;
  } else if (back === "type") {
    Object.assign(current, { kind: null, x: null, xKind: null, ys: [], z: null, table: false });
  } else if (back === "x") {
    Object.assign(current, { x: null, xKind: null, ys: [], table: false });
  } else if (back === "y") {
    Object.assign(current, { ys: [], z: null });
    if (stage === "group") Object.assign(current, { x: null, xKind: null });
  }
  await enterStage(back);
}

// ---------- command mode ----------

const COMMAND_HINTS: [string, string][] = [["↵", "preview"], ["⇥", "accept"], ["⌘E", "picker"], ["esc", "close"]];

async function enterCommand(text: string) {
  await leavePreview();
  mode = "command";
  setPrompt(ICON.command, "cmd");
  query.placeholder = "line, hist, heatmap… · again <plot> · preset <name> <file>";
  // Typing in a preview lands here with the box already holding the text: leave the
  // caret alone then (owner: it jumped to the end after the first letter).
  if (query.value !== text) {
    query.value = text;
    query.setSelectionRange(text.length, text.length);
  }
  revealCaret();
  setHints(COMMAND_HINTS);
  syncMirror();
  updateCrumb();
  await refreshCommand();
  changed(false);
}

async function refreshCommand() {
  const text = query.value;
  const id = ++requestId;
  updateCrumb();
  if (!text.trim()) {
    items = (info?.history ?? []).map((c) => ({ label: c, value: c, icon: ICON.history, iconColor: "var(--yellow)" }));
    if (!items.length) items = [{ label: "No commands yet — start with a plot type such as line or hist", value: null, note: true }];
  } else {
    const c = await api.complete(text, query.selectionStart ?? text.length);
    if (id !== requestId) return;
    items = c.items.map((s) => suggestionItem(s, [c.start, c.end]));
    const word = text.slice(c.start, c.end).toLowerCase();
    if (word.startsWith("color=") || word.startsWith("colour=")) {
      items.push({ label: "Pick a colour…", detail: "the macOS colour panel", value: "\u0000colour", icon: ICON.colour, iconColor: "var(--accent)", range: [c.start, c.end] });
    }
  }
  selected = 0;
  renderList();
}

function suggestionItem(s: api.Suggestion, range: [number, number]): Item {
  const item: Item = { label: s.label, detail: s.detail, value: s.insert, range };
  if (/^#[0-9a-f]{6}$/i.test(s.detail)) item.swatch = s.detail;
  else if (s.detail === "file") Object.assign(item, { ...splitPath(s.label), icon: ICON.file, iconColor: "var(--green)", detail: "" });
  else if (s.detail === "numbers" || s.detail === "dates" || s.detail === "text") {
    Object.assign(item, columnItem({ name: s.label, insert: s.insert, kind: s.detail }), { value: s.insert, range });
  } else if (TYPE_WORDS.has(s.label)) Object.assign(item, { icon: KIND_LOOK[s.label][0], iconColor: KIND_LOOK[s.label][1] });
  else if (s.detail === "saved plot") Object.assign(item, { icon: ICON.image, iconColor: "var(--green)" });
  else Object.assign(item, { icon: ICON.option, iconColor: "var(--blue)" });
  return item;
}

/** Put the highlighted suggestion into the text (⇥). */
async function acceptSuggestion() {
  const item = items[selected];
  if (!item || item.note || item.value === null) return;
  if (item.value === "\u0000colour" && item.range) {
    pickColour(item.range);
    return;
  }
  if (!item.range) {
    query.value = item.value; // a recent command: load it for editing
  } else {
    const [a, b] = item.range;
    const t = query.value;
    query.value = t.slice(0, a) + item.value + t.slice(b);
    const caret = a + item.value.length;
    query.setSelectionRange(caret, caret);
  }
  revealCaret();
  showError(null);
  syncMirror();
  await refreshCommand();
  changed(false);
  scheduleLive();
}

// ---------- preview and saving ----------

async function runPreview(command: string) {
  const text = command.trim();
  if (busy || !text) return;
  busy = true;
  const started = session;
  setHints([["", "drawing…"]]);
  try {
    const p = await api.preview(text);
    if (started !== session) {
      // Closed and opened afresh while drawing: don't show it (and undo the bigger window).
      await api.endPreview();
      return;
    }
    if (info) info.history = [text, ...info.history.filter((c) => c !== text)];
    showError(null);
    query.value = text;
    previewed = text;
    showPreview(p);
  } catch (e) {
    if (started !== session) return;
    const err = e as api.PlotError;
    if (mode !== "command") await enterCommand(text);
    showError(err, query.value);
    if (err.start !== null && err.end !== null) query.setSelectionRange(err.start, err.end);
    setHints(COMMAND_HINTS);
  } finally {
    busy = false;
  }
}

function showPreview(p: api.Preview) {
  mode = "preview";
  lastSaved = null;
  panel.classList.remove("saved");
  results.hidden = true;
  previewBox.hidden = false;
  plotImage.src = p.image;
  status.innerHTML = `<span class="path">not saved yet</span><span>${Math.round(p.millis)} ms</span>`;
  // One hint at a time: the most useful first; the next shows once that one is dealt with.
  advice = p.advice[0] ?? null;
  adviceLine.hidden = !advice;
  adviceLine.replaceChildren();
  if (advice) {
    // The whole hint on hover, when a narrow window cuts it short.
    const text = Object.assign(document.createElement("span"), { className: "text", textContent: `hint: ${advice.text}`, title: advice.text });
    adviceLine.append(text);
    if (advice.fix) {
      const key = Object.assign(document.createElement("span"), { className: "key", textContent: "⇥ apply" });
      adviceLine.append(key);
    }
  }
  setPrompt(ICON.image, "plot");
  setHints(PREVIEW_HINTS());
  syncMirror();
  updateCrumb();
}

const PREVIEW_HINTS = (): [string, string][] => [
  ["↵", `save ${(info?.format ?? "png").toUpperCase()}`],
  ...(advice?.fix ? ([["⇥", "apply hint"]] as [string, string][]) : []),
  ["⌘1 ⌘2 ⌘3", "PNG · PDF · SVG"],
  ["⌘⇧S", "save as…"],
  ["⌘C", "copy"],
  ["⌘⇧C", "copy the numbers"],
  ["⌘R", "redraw"],
  ["⌘D", "keep the look"],
  ["⌘⇧D", "keep as preset"],
  ["esc", "close"],
];

/** Leave the preview (shrinking the window back), e.g. when the command is edited. */
async function leavePreview() {
  if (mode === "browse") {
    grid.hidden = true;
    results.hidden = false;
    await api.endPreview();
    return;
  }
  // Naming a style or preset (⌘D / ⌘⇧D) also has the preview showing.
  if (mode !== "preview" && mode !== "saved" && mode !== "name") return;
  previewBox.hidden = true;
  results.hidden = false;
  panel.classList.remove("saved");
  previewed = "";
  await api.endPreview();
}

// ---------- saved-plots browser (⌘O, or ↵ on a bare `again`) ----------

const GRID_COLUMNS = 3;
let plots: api.RecentPlot[] = [];
let shownPlots: api.RecentPlot[] = [];
const thumbs = new Map<string, string>();

async function enterBrowse() {
  await leavePreview();
  mode = "browse";
  results.hidden = true;
  grid.hidden = false;
  live.hidden = true;
  query.value = "";
  query.placeholder = "search saved plots (name or command)…";
  setPrompt(ICON.image, "plots");
  setHints([["↵", "edit again"], ["⌘↵", "open"], ["← ↑ ↓ →", "move"], ["⌘E", "picker"], ["esc", "close"]]);
  syncMirror();
  showError(null);
  selected = 0;
  await api.growLauncher();
  plots = await api.recentPlots().catch(() => []);
  updateCrumb();
  renderGrid();
}

function ago(secs: number): string {
  const s = Date.now() / 1000 - secs;
  if (s < 90) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return new Date(secs * 1000).toISOString().slice(0, 10);
}

function renderGrid() {
  shownPlots = fuzzy(query.value, plots, (p) => `${p.name} ${p.command}`);
  selected = Math.min(selected, Math.max(0, shownPlots.length - 1));
  grid.innerHTML = "";
  if (!shownPlots.length) {
    grid.innerHTML = `<div class="empty">${plots.length ? "No saved plot matches" : "No plots saved yet in your plots folder"}</div>`;
    return;
  }
  shownPlots.forEach((p, i) => {
    const tile = document.createElement("div");
    tile.className = i === selected ? "tile selected" : "tile";
    tile.title = p.command;
    const img = document.createElement("img");
    img.alt = "";
    const cached = thumbs.get(p.path);
    if (cached) img.src = cached;
    else {
      void api.plotThumb(p.path).then((src) => {
        thumbs.set(p.path, src);
        img.src = src;
      }).catch(() => img.classList.add("none"));
    }
    const name = document.createElement("div");
    name.className = "name";
    name.textContent = p.name;
    const when = document.createElement("div");
    when.className = "when";
    when.textContent = ago(p.modified);
    tile.append(img, name, when);
    tile.addEventListener("mousedown", (e) => {
      e.preventDefault();
      selected = i;
      void reopenPlot(e.metaKey);
    });
    grid.append(tile);
  });
  grid.querySelector(".tile.selected")?.scrollIntoView({ block: "nearest" });
}

function moveInGrid(key: string) {
  const n = shownPlots.length;
  if (!n) return;
  const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -GRID_COLUMNS, ArrowDown: GRID_COLUMNS }[key] ?? 0;
  selected = Math.min(n - 1, Math.max(0, selected + step));
  renderGrid();
}

/** ↵: the plot's command back in the box, ready to change; ⌘↵: open the file. */
async function reopenPlot(open: boolean) {
  const p = shownPlots[selected];
  if (!p) return;
  if (open) {
    await api.openPlot(p.path, false);
    await close();
    return;
  }
  await enterCommand(p.command + " ");
}

async function afterSave(r: api.SaveResult | null) {
  if (!r) return; // Save dialog cancelled
  if (r.after_save !== "thumbnail") {
    await close();
    return;
  }
  mode = "saved";
  lastSaved = r;
  panel.classList.add("saved");
  status.innerHTML = `<span class="path">${escapeHtml(r.shown_path)}</span>`;
  setPrompt(ICON.saved, "saved");
  setHints([["↵", "open"], ["⌘↵", "Finder"], ["⌘1 ⌘2 ⌘3", "copy as PNG · PDF · SVG"], ["⌘⇧S", "save as…"], ["esc", "close"]]);
  updateCrumb();
}

async function save(asNew: boolean, format: string | null = null) {
  if (busy) return;
  busy = true;
  const started = session;
  try {
    const r = asNew ? await api.savePreviewAs() : await api.savePreview(format);
    // Closed (Esc) while saving: the file is saved, but the launcher has moved on.
    if (started === session) await afterSave(r);
  } catch (e) {
    if (started === session) showError(e as api.PlotError);
  } finally {
    busy = false;
  }
}

/** `preset NAME FILE`: the preset's command with this file, drawn at once. */
async function usePreset(text: string) {
  const m = /^preset\s+(\S+)\s+(.+)$/i.exec(text);
  if (!m) {
    showError({ message: "type preset NAME FILE, e.g. preset thesis run_4.csv", start: null, end: null });
    return;
  }
  try {
    const command = await api.applyPreset(m[1], m[2].replace(/^"(.*)"$/, "$1"));
    await enterCommand(command);
    await runPreview(command);
  } catch (e) {
    showError(e as api.PlotError);
  }
}

// ---------- styles (⌘D) and presets (⌘⇧D) in the preview ----------

let namingFrom = "";
/** What ⌘D / ⌘⇧D keeps: the look only (a style) or the whole command (a preset). */
let namingKind: "style" | "preset" = "style";

function enterNaming(kind: "style" | "preset") {
  namingFrom = previewed;
  namingKind = kind;
  mode = "name";
  query.value = "";
  query.placeholder = kind === "style"
    ? "name this look (one word), ↵ to keep it as a style"
    : "name this preset (one word), ↵ to keep the whole command";
  setPrompt(kind === "style" ? ICON.colour : ICON.history, kind);
  setHints([["↵", "keep"], ["esc", "close"]]);
  syncMirror();
  query.focus();
}

async function finishNaming() {
  const name = query.value.trim();
  try {
    let done: string;
    if (namingKind === "style") {
      await api.saveStyle(name);
      if (info && !info.styles.includes(name)) info.styles = [...info.styles, name].sort();
      done = `kept the look as style “${escapeHtml(name)}”: --style=${escapeHtml(name)}, or pick it after the columns`;
    } else {
      await api.savePreset(name);
      if (info) info.presets = [...info.presets.filter((p) => p.name !== name), { name, command: namingFrom }];
      done = `kept as preset “${escapeHtml(name)}”: the same plot for files with the same columns`;
    }
    mode = "preview";
    query.value = namingFrom;
    setPrompt(ICON.image, "plot");
    setHints(PREVIEW_HINTS());
    syncMirror();
    status.innerHTML = `<span class="path">${done}</span>`;
    showError(null);
  } catch (e) {
    showError(e as api.PlotError);
  }
}

// ---------- live preview while typing or picking (launcher.live_preview) ----------

let liveTimer = 0;
let liveFor = "";
let liveSeq = 0;

/** The command the picker would make with the highlighted item (null: nothing to draw yet). */
function pickerCandidate(): string | null {
  if (!current || stage === "file") return null;
  const item = items[selected];
  if (stage === "type" && (!item?.value || item.value.startsWith(PRESET))) return null;
  const l: Layer = { ...current, ys: [...current.ys] };
  if (item && !item.note) applyChoice(l, stage, item.value);
  if (!l.kind) return null;
  return buildCommand([...layers, l]);
}

/** What the small preview would draw now (null: nothing). */
function liveText(): string | null {
  const text = mode === "command" ? query.value.trim() : mode === "picker" ? pickerCandidate() : null;
  return !text || /^(again|preset)\b/i.test(text) ? null : text;
}

/** Draw a small preview of what's typed or highlighted, after a short pause. The old
 *  picture goes the moment there's nothing to draw, and fades the moment the text
 *  changes, so it never stands for a command that's no longer there (owner, round 13). */
function scheduleLive(wait = 150) {
  clearTimeout(liveTimer);
  if (!info?.live_preview) {
    live.hidden = true;
    return;
  }
  const now = liveText();
  if (!now) {
    hideLive();
    return;
  }
  if (now !== liveFor) live.classList.add("stale");
  liveTimer = window.setTimeout(async () => {
    const text = liveText();
    if (!text || text === liveFor) return;
    liveFor = text;
    const seq = ++liveSeq;
    try {
      const image = await api.previewThumb(text);
      if (seq !== liveSeq) return; // a newer one is on its way
      live.src = image;
      live.hidden = false;
      live.classList.remove("stale");
    } catch {
      // Unfinished commands keep the last good drawing, faded to show it's not this one.
      if (seq === liveSeq) live.classList.add("stale");
    }
  }, wait);
}

function hideLive() {
  clearTimeout(liveTimer);
  liveSeq++;
  liveFor = "";
  live.hidden = true;
  live.removeAttribute("src");
}

// ---------- "Pick a colour…" after color= ----------

/** The macOS colour panel, through a hidden colour input. */
function pickColour(range: [number, number]) {
  const input = Object.assign(document.createElement("input"), { type: "color" });
  input.style.position = "fixed";
  input.style.opacity = "0";
  document.body.append(input);
  void api.holdOpen(true);
  const done = () => {
    void api.holdOpen(false);
    input.remove();
    query.focus();
  };
  input.addEventListener("change", () => {
    const t = query.value;
    const word = t.slice(range[0], range[1]);
    const key = word.split("=")[0] || "color";
    const insert = `${key}=${input.value} `;
    query.value = t.slice(0, range[0]) + insert + t.slice(range[1]);
    const caret = range[0] + insert.length;
    query.setSelectionRange(caret, caret);
    syncMirror();
    void refreshCommand();
    done();
  });
  input.addEventListener("blur", () => setTimeout(done, 300));
  input.click();
}

/** `again PLOT`: put the command a saved plot was made from back in the box. */
async function recall(plot: string) {
  try {
    const command = await api.recallPlot(plot);
    showError(null);
    await enterCommand(command + " ");
  } catch (e) {
    showError(e as api.PlotError);
  }
}

// ---------- undo / redo (⌘Z / ⌘⇧Z) ----------

/** One state of the launcher that ⌘Z can go back to. */
interface Snap {
  mode: "picker" | "command";
  stage: Stage;
  text: string;
  caret: number;
  layers: Layer[];
  current: Layer | null;
}

const copyLayer = (l: Layer): Layer => ({ ...l, ys: [...l.ys] });
let undoStack: Snap[] = [];
let redoStack: Snap[] = [];
let shownSnap: Snap | null = null;
let lastChange = 0;
let restoring = false;

function snapshot(): Snap | null {
  if (mode !== "picker" && mode !== "command" && mode !== "preview") return null;
  return {
    mode: mode === "picker" ? "picker" : "command",
    stage,
    text: mode === "preview" ? previewed : query.value,
    caret: query.selectionEnd ?? query.value.length,
    layers: layers.map(copyLayer),
    current: current && copyLayer(current),
  };
}

const sameSnap = (a: Snap, b: Snap) =>
  a.mode === b.mode && a.stage === b.stage && a.text === b.text && JSON.stringify([a.layers, a.current]) === JSON.stringify([b.layers, b.current]);

/** Note a change: the state before it can be undone. Typing within a second is one step. */
function changed(typing: boolean) {
  if (restoring) return;
  const now = snapshot();
  if (!now) return;
  if (shownSnap && !sameSnap(shownSnap, now)) {
    const merge = typing && Date.now() - lastChange < 1000 && undoStack.length > 0;
    if (!merge) undoStack.push(shownSnap);
    if (undoStack.length > 200) undoStack.shift();
    redoStack = [];
  }
  shownSnap = now;
  lastChange = typing ? Date.now() : 0;
}

async function restore(s: Snap) {
  restoring = true;
  try {
    layers = s.layers.map(copyLayer);
    current = s.current && copyLayer(s.current);
    if (s.mode === "picker") {
      await enterStage(s.stage);
      query.value = s.text;
      await refreshPicker();
    } else {
      await enterCommand(s.text);
    }
    query.setSelectionRange(s.caret, s.caret);
    revealCaret();
    syncMirror();
    showError(null);
  } finally {
    restoring = false;
  }
  shownSnap = snapshot();
  lastChange = 0;
}

async function undo(redo: boolean) {
  const from = redo ? redoStack : undoStack;
  const to = redo ? undoStack : redoStack;
  const target = from.pop();
  const now = snapshot();
  if (!target || !now) return;
  to.push(now);
  await restore(target);
}

// ---------- start / reset / keeping the draft ----------

/** A draft left by clicking away is kept this long; after that the launcher starts fresh. */
const DRAFT_MINUTES = 10;
let lastUsed = 0;
let freshNext = false;
/** Counts fresh starts, so an answer that arrives after one (a slow save) is ignored. */
let session = 0;

/** ⌘P: kept open when another app is clicked (closing unpins). */
let pinned = false;
function setPinned(on: boolean) {
  pinned = on;
  $<HTMLSpanElement>("pin").hidden = !on;
  void api.setPinned(on);
  if (on) crumb.textContent = "pinned: drag files from Finder onto this window · ⌘P unpins";
  else updateCrumb();
}

/** Close on purpose (Esc, after saving, opening the plot): next time starts fresh. */
async function close() {
  if (pinned) setPinned(false);
  freshNext = true;
  await api.hideLauncher();
}

async function loadInfo() {
  info = await api.launcherInfo();
  warning.textContent = info.warnings.join(" · ");
  warning.title = info.warnings.join("\n");
  applyAppearance(info.ui);
  if (!info.live_preview) hideLive();
}

async function reset() {
  session++;
  undoStack = [];
  redoStack = [];
  shownSnap = null;
  busy = false; // whatever was running belongs to the old session
  // Whatever was showing (a preview, the browser, a name being typed) goes.
  await leavePreview();
  previewBox.hidden = true;
  grid.hidden = true;
  results.hidden = false;
  panel.classList.remove("saved");
  previewed = "";
  showError(null);
  await loadInfo();
  lastSaved = null;
  hideLive();
  if (info!.mode === "command") await enterCommand("");
  else await startPicker();
  query.focus();
}

/** Shown again: start fresh, or carry on where the owner was (clicked away, hotkey again). */
async function shown(dropped: string[] = []) {
  // Hidden by the hotkey or Esc: that unpinned it (a drop keeps the pin).
  if (pinned && !dropped.length) setPinned(false);
  // Files dropped on the launcher: start afresh with them picked.
  if (dropped.length) {
    freshNext = false;
    await reset();
    await pickDropped(dropped);
    return;
  }
  const stale = Date.now() - lastUsed > DRAFT_MINUTES * 60_000;
  if (freshNext || stale || mode === "saved" || mode === "name") {
    freshNext = false;
    await reset();
    return;
  }
  await loadInfo();
  // The window opens at the normal size, so a preview comes back as its command (↵ redraws).
  if (mode === "preview") await enterCommand(previewed || query.value);
  else if (mode === "browse") await enterBrowse();
  else if (mode === "picker") {
    if (stage === "file") clip = await api.clipboardTable().catch(() => null);
    await refreshPicker();
  }
  query.focus();
  revealCaret();
}

async function choose() {
  const item = items[selected];
  if (mode === "command") {
    // ↵ on a recent command (empty box) draws it; otherwise draw what's typed.
    const again = /^again\s+(.+)$/i.exec(query.value.trim());
    if (/^again$/i.test(query.value.trim())) await enterBrowse();
    else if (!query.value.trim() && item?.value && !item.range) await runPreview(item.value);
    else if (again) await recall(again[1].replace(/^"(.*)"$/, "$1"));
    else if (/^preset\s+/i.test(query.value.trim())) await usePreset(query.value.trim());
    else await runPreview(query.value);
  } else if (mode === "picker" && item) {
    await choosePicker(item);
  }
}

for (const ev of ["keydown", "input", "mousedown"]) {
  document.addEventListener(ev, () => (lastUsed = Date.now()), true);
}

query.addEventListener("input", () => {
  // A pasted line break would make a second command line: it's a space instead.
  if (/[\r\n]/.test(query.value)) {
    const at = query.selectionEnd ?? query.value.length;
    query.value = query.value.replace(/\r\n?|\n/g, " ");
    query.setSelectionRange(at, at);
  }
  fitLines();
  if (mode === "browse") {
    selected = 0;
    renderGrid();
    return;
  }
  if (mode === "name") {
    // The box's own text is see-through (the coloured copy behind it shows), so the copy
    // must follow the name being typed (owner: the name was invisible).
    syncMirror();
    return;
  }
  if (mode === "preview" || mode === "saved") {
    // Editing the command goes back to command mode; ↵ draws it again.
    void enterCommand(query.value);
    return;
  }
  showError(null);
  if (mode === "picker") {
    // Typing a plot type and a space switches to command mode.
    if (stage === "file" && TYPE_START.test(query.value)) {
      void enterCommand(query.value);
      return;
    }
    // Typing an option (`c=`, `title=`) after the columns: carry on in command mode with
    // what the picker built so far, so the option's suggestions show (owner: c= listed
    // nothing; the picker was still filtering column names).
    if (stage !== "file" && stage !== "type" && /^[a-z0-9-]+=/i.test(query.value)) {
      const built = current?.kind ? buildCommand([...layers, current]) : null;
      if (built) {
        void enterCommand(`${built} ${query.value}`);
        return;
      }
    }
    void refreshPicker();
  } else {
    syncMirror();
    void refreshCommand();
    scheduleLive();
  }
  changed(true);
});

query.addEventListener("scroll", () => {
  mirror.scrollTop = query.scrollTop;
});

// Moving the cursor changes which word is completed.
query.addEventListener("keyup", (e) => {
  mirror.scrollTop = query.scrollTop;
  if (mode === "command" && ["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) {
    void refreshCommand();
  }
});

document.addEventListener("keydown", (e) => {
  const key = e.key;
  const cmd = e.metaKey;
  if (key === "Escape") {
    e.preventDefault();
    void close();
    return;
  }

  if (mode === "name") {
    if (key === "Enter") {
      e.preventDefault();
      void finishNaming();
    }
    return;
  }
  // ⌘Z / ⌘⇧Z: back and forward through edits and picker steps (not just the box's text).
  if (cmd && key.toLowerCase() === "z") {
    e.preventDefault();
    void undo(e.shiftKey);
    return;
  }
  // ⌘O: the saved-plots browser, from anywhere.
  // ⌘P: pin open, so it stays while you click elsewhere (to drag files onto it).
  if (cmd && key.toLowerCase() === "p") {
    e.preventDefault();
    setPinned(!pinned);
    return;
  }
  if (cmd && key.toLowerCase() === "o") {
    e.preventDefault();
    void enterBrowse();
    return;
  }
  if (mode === "browse") {
    if (key.startsWith("Arrow")) {
      e.preventDefault();
      moveInGrid(key);
    } else if (key === "Enter") {
      e.preventDefault();
      void reopenPlot(cmd);
    } else if (cmd && key.toLowerCase() === "e") {
      e.preventDefault();
      void startPicker();
    }
    return;
  }
  if (mode === "preview" || mode === "saved") {
    const hasSelection = query.selectionStart !== query.selectionEnd;
    const formats: Record<string, string> = { "1": "png", "2": "pdf", "3": "svg" };
    if (cmd && key.toLowerCase() === "s") {
      e.preventDefault();
      void save(e.shiftKey || mode === "saved");
    } else if (cmd && formats[key]) {
      // ⌘1 / ⌘2 / ⌘3: this plot as PNG / PDF / SVG, whatever the setting says.
      e.preventDefault();
      void save(false, formats[key]);
    } else if (cmd && key.toLowerCase() === "d" && previewed) {
      // ⌘D keeps the look (a style), ⌘⇧D the whole command (a preset).
      e.preventDefault();
      enterNaming(e.shiftKey ? "preset" : "style");
    } else if (cmd && e.shiftKey && key.toLowerCase() === "c" && !hasSelection) {
      // ⌘⇧C: the numbers actually drawn (after where=, calculations, fits) as a table.
      e.preventDefault();
      void api.copyPreviewData().then(() => {
        status.lastElementChild?.replaceWith(Object.assign(document.createElement("span"), { textContent: "numbers copied as a table" }));
      }).catch((err: api.PlotError) => showError(err));
    } else if (cmd && key.toLowerCase() === "c" && !hasSelection) {
      e.preventDefault();
      void api.copyPreview().then(() => {
        status.lastElementChild?.replaceWith(Object.assign(document.createElement("span"), { textContent: "image copied" }));
      });
    } else if (key === "Tab") {
      // ⇥: the advisor's fix goes in the box and is drawn (the command stays plain text,
      // so ⌘Z or editing takes it back). Without a fix ⇥ does nothing, as before.
      e.preventDefault();
      if (advice?.fix && !e.shiftKey) {
        const fix = advice.fix;
        query.value = fix;
        // Noted as one step, so ⌘Z gives back the command as it was.
        void runPreview(fix).then(() => changed(false));
      }
    } else if (cmd && key.toLowerCase() === "r") {
      // ⌘R: draw the same command again — the file is read afresh, so a changed CSV
      // shows up (owner, 2026-11-14: no file watching, just this).
      e.preventDefault();
      void runPreview(query.value.trim() || previewed);
    } else if (key === "Enter") {
      e.preventDefault();
      if (mode === "saved" && lastSaved) {
        void api.openPlot(lastSaved.path, cmd).then(close);
      } else if (query.value.trim() === previewed) {
        void save(false);
      } else {
        void runPreview(query.value);
      }
    }
    return;
  }

  if (key === "e" && cmd) {
    e.preventDefault();
    if (mode === "picker") {
      const partial = buildCommand(current ? [...layers, current] : layers).replace(/^… /, "line ");
      void enterCommand(layers.length || current ? partial + " " : "");
    } else {
      void startPicker();
    }
    return;
  }
  if (key === "ArrowDown" || key === "ArrowUp") {
    // ↑ and ↓ only move through the list (owner: ↑ must not jump to the last command;
    // recent commands are listed in command mode, ⌘E, while its box is empty).
    e.preventDefault();
    moveSelection(key === "ArrowDown" ? 1 : -1);
    return;
  }
  if (key === "Tab") {
    e.preventDefault();
    if (mode === "command" && errorFix && errorFix.command === query.value) applyErrorFix();
    else if (mode === "command") void acceptSuggestion();
    else if (mode === "picker" && stage === "y" && current && manyYs(current.kind)) {
      const item = items[selected];
      if (item && !item.note && item.value) {
        const at = current.ys.indexOf(item.value);
        if (at >= 0) current.ys.splice(at, 1);
        else current.ys.push(item.value);
        updateCrumb();
        void refreshPicker();
        changed(false);
      }
    }
    return;
  }
  if (key === "Enter") {
    e.preventDefault();
    void choose();
    return;
  }
  if (key === "Backspace" && mode === "picker" && !query.value) {
    e.preventDefault();
    void backPicker();
  }
});

// The text box always has the keyboard: when the window gets it, and after a click
// anywhere else in the launcher (owner: a click above the box left typing going nowhere).
window.addEventListener("focus", () => query.focus());
document.addEventListener("mousedown", (e) => {
  if (e.target !== query) {
    e.preventDefault();
    query.focus();
  }
});

// Opening is async (it may reset the launcher): a Try-it command that arrives meanwhile
// waits for it, or the reset wiped the command and a second click was needed (owner,
// 2026-11-15).
let showing: Promise<void> = Promise.resolve();
void listen<string[] | null>("launcher-shown", (e) => {
  showing = shown(e.payload ?? []);
});
// A Try-it lesson's "Open in launcher": one click puts the command in and draws it.
void listen<string>("launcher-command", async (e) => {
  await showing;
  await enterCommand(e.payload);
  await runPreview(e.payload);
});
// config.toml was edited: apply the new look straight away (the Rust side has already
// switched the hotkey and blur).
void listen("config-changed", () => void loadInfo());
void listen<boolean>("drag-over", (e) => panel.classList.toggle("dropping", e.payload));
void listen("files-updated", async () => {
  if (info) info.scanning = false;
  if (mode === "picker" && stage === "file") await refreshPicker();
});

void reset();
