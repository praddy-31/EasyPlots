// The settings window: a form for everything in config.toml. The Rust side describes each
// option (label, hint, kind of control) and writes changes into the file with its comments
// kept; this file only draws the form and sends changes, one option at a time.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { helpView } from "./help";
import { tryItView } from "./tryit";

type Kind =
  | { type: "text" }
  | { type: "suggest"; options: string[] }
  | { type: "number"; min: number; max: number; step: number }
  | { type: "whole"; min: number; max: number }
  | { type: "bool" }
  | { type: "choice"; options: string[] }
  | { type: "hotkey" }
  | { type: "folder" };

interface Field {
  section: string;
  key: string;
  label: string;
  help: string;
  kind: Kind;
}
interface Preset {
  name: string;
  command: string;
}
interface Info {
  fields: Field[];
  values: Record<string, Record<string, unknown>>;
  config_path: string;
  presets: Preset[];
  /** Saved styles (`styles/NAME.toml`). */
  styles?: string[];
  warnings: string[];
  version: string;
  /** The launcher's opacity and blur, and Palette's glass when it sets them. */
  look?: Look;
}
interface Look {
  opacity: number;
  blur: number;
  palette: { kind: "solid" | "blurred" | "clear"; opacity: number; blur: number } | null;
  faint: boolean;
}

/** Below this opacity the launcher's text gets hard to read: a warning, never a limit. */
const READABLE = 0.4;

const TABS: [string, string][] = [
  ["general", "General"],
  ["output", "Saving"],
  ["style", "Plot look"],
  ["launcher", "Launcher"],
  ["presets", "Presets & styles"],
  ["tryit", "Try it"],
  ["help", "Help"],
];

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const tabs = $<HTMLElement>("tabs");
const form = $<HTMLElement>("form");
let info: Info | null = null;
let tab = "general";

/** A new element with properties (textContent, className, value…) and children. */
function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  Object.assign(e, props);
  e.append(...kids);
  return e;
}

/** True while a slider is held, so a redraw doesn't pull it out from under the pointer. */
let dragging = false;
document.addEventListener("pointerdown", (e) => (dragging = (e.target as HTMLInputElement).type === "range"));
document.addEventListener("pointerup", () => (dragging = false));

async function load() {
  const next = await invoke<Info>("settings_info");
  const shown = (i: Info | null) => JSON.stringify(i && [i.values, i.presets, i.styles, i.warnings, i.look]);
  const changed = shown(next) !== shown(info);
  info = next;
  $<HTMLSpanElement>("version").textContent = info.version;
  $<HTMLElement>("config-path").textContent = info.config_path;
  if (changed && !dragging) draw();
}

function draw() {
  if (!info) return;
  tabs.replaceChildren(
    ...TABS.map(([id, name]) => {
      const b = el("button", { type: "button", textContent: name, className: id === tab ? "active" : "" });
      b.onclick = () => {
        tab = id;
        draw();
      };
      return b;
    }),
  );
  const rows: Node[] = [];
  if (tab === "general" && info.warnings.length) {
    rows.push(el("div", { className: "warning", textContent: info.warnings.join(" · ") }));
  }
  if (tab === "launcher" && info.look?.palette) rows.push(paletteBanner(info.look.palette));
  if (tab === "help") rows.push(helpView());
  else if (tab === "tryit") rows.push(tryItView());
  else if (tab === "presets") rows.push(presets());
  else for (const f of info.fields.filter((f) => f.section === tab)) rows.push(row(f));
  form.replaceChildren(...rows);
}

/** Palette is in charge of the launcher's glass (`palette glass easyplots`): say so, and how
 *  to hand it back (like Dispatch's Settings → Look). */
function paletteBanner(g: NonNullable<Look["palette"]>): HTMLElement {
  const KIND = { solid: "Solid", blurred: "Blurred", clear: "Clear" };
  const through = Math.round((1 - g.opacity) * 100);
  return el(
    "div",
    { className: "palette-banner" },
    el("strong", { textContent: `🎨 Glass set by Palette: ${KIND[g.kind] ?? g.kind}` }),
    el("span", { textContent: ` · ${through} % see-through${g.kind === "blurred" ? ` · blur ${g.blur}` : ""}. ` }),
    el("span", { textContent: "Change it in Palette → Apps → EasyPlots → Glass, or choose “Yours” there to use the sliders below." }),
  );
}

/** The readability warning under Opacity: shown below 40 % solid, never a limit. */
function faintWarning(opacity: number, byPalette: boolean): string {
  if (opacity >= READABLE) return "";
  const through = Math.round((1 - opacity) * 100);
  return `⚠ ${through} % see-through${byPalette ? " (from Palette)" : ""}: text in the launcher can be hard to read over a busy background. It's allowed; this is only a warning.`;
}

/** One option: its label, a control, the hint, and a place for errors. */
function row(f: Field): HTMLElement {
  // Opacity and blur come from Palette while it sets the glass: shown, but not editable here.
  const glass = f.section === "launcher" && (f.key === "opacity" || f.key === "blur") ? info!.look?.palette : null;
  const value = glass ? (f.key === "opacity" ? glass.opacity : glass.blur) : info!.values[f.section]?.[f.key];
  const warn = el("div", { className: "warn-line" });
  const showWarn = (v: number) => {
    warn.textContent = f.section === "launcher" && f.key === "opacity" ? faintWarning(v, !!glass) : "";
    warn.hidden = !warn.textContent;
  };
  showWarn(Number(value));
  const error = el("div", { className: "error" });
  error.hidden = true;
  const saved = el("span", { className: "saved", textContent: "saved" });
  saved.hidden = true;
  const send = async (v: unknown) => {
    try {
      info!.values = await invoke("set_setting", { section: f.section, key: f.key, value: v });
      error.hidden = true;
      saved.hidden = false;
      setTimeout(() => (saved.hidden = true), 1200);
    } catch (e) {
      error.textContent = String(e);
      error.hidden = false;
    }
  };
  const control = el("div", { className: "control" });
  const id = `${f.section}-${f.key}`;
  const k = f.kind;
  if (k.type === "bool") {
    const c = el("input", { type: "checkbox", id, checked: value === true });
    c.onchange = () => void send(c.checked);
    control.append(c);
  } else if (k.type === "choice") {
    const s = el("select", { id });
    for (const o of k.options) s.append(el("option", { value: o, textContent: o, selected: o === value }));
    s.onchange = () => void send(s.value);
    control.append(s);
  } else if (k.type === "number" || k.type === "whole") {
    // The range comes before the value: a slider is 0–100 in steps of 1 until told
    // otherwise, and would round 0.9 to 1 (it then looked stuck at the right end).
    const n = el("input", { type: "number", id });
    n.min = String(k.min);
    n.max = String(k.max);
    n.step = k.type === "number" ? String(k.step) : "1";
    n.value = String(value ?? "");
    n.onchange = () => {
      showWarn(Number(n.value));
      void send(Number(n.value));
    };
    control.append(n);
    // Opacity and the like (a small range), and blur (0–100): a slider beside the number.
    if ((k.type === "number" && k.max - k.min <= 2) || (k.type === "whole" && k.max - k.min <= 100)) {
      // Opacity and the like: a slider next to the number, applied as it moves.
      const r = el("input", { type: "range" });
      r.min = n.min;
      r.max = n.max;
      r.step = n.step;
      r.value = n.value;
      let timer = 0;
      r.oninput = () => {
        n.value = r.value;
        showWarn(Number(r.value));
        clearTimeout(timer);
        timer = window.setTimeout(() => void send(Number(r.value)), 120);
      };
      n.addEventListener("change", () => (r.value = n.value));
      control.prepend(r);
    }
  } else if (k.type === "hotkey") {
    control.append(hotkeyInput(id, String(value ?? ""), send));
  } else if (k.type === "folder") {
    const t = el("input", { type: "text", id, value: String(value ?? "") });
    t.onchange = () => void send(t.value);
    const b = el("button", { type: "button", textContent: "Choose…" });
    b.onclick = async () => {
      try {
        const v = await invoke<Info["values"] | null>("pick_folder", { section: f.section, key: f.key });
        if (v) {
          info!.values = v;
          t.value = String(v[f.section][f.key]);
        }
      } catch (e) {
        error.textContent = String(e);
        error.hidden = false;
      }
    };
    control.append(t, b);
  } else if (k.type === "suggest" && f.section === "launcher" && f.key === "font") {
    // A real menu: the Mac's web view shows no suggestion list under a text box, so the
    // owner saw a bare text box.
    const s = el("select", { id });
    s.onchange = () => void send(s.value);
    control.append(s);
    void fillFonts(s, k.options, String(value ?? ""));
  } else {
    const t = el("input", { type: "text", id, value: String(value ?? "") });
    t.onchange = () => void send(t.value);
    control.append(t);
    if (k.type === "suggest") {
      // Common values in a menu beside the box (picking one fills it in and saves).
      const pick = el("select", { className: "suggest", title: "common values" });
      pick.append(el("option", { value: "", textContent: "…" }));
      for (const o of k.options) pick.append(el("option", { value: o, textContent: o }));
      pick.onchange = () => {
        if (!pick.value) return;
        t.value = pick.value;
        pick.value = "";
        void send(t.value);
      };
      control.append(pick);
    }
  }
  control.append(saved);
  if (glass) {
    for (const i of control.querySelectorAll("input")) i.disabled = true;
    control.append(el("span", { className: "muted", textContent: "set by Palette" }));
  }
  const label = el("label", { htmlFor: id, textContent: f.label });
  const help = el("div", { className: "help", textContent: f.help });
  return el("div", { className: "row" }, label, control, ...(f.help ? [help] : []), warn, error);
}

let fonts: { name: string; mono: boolean }[] | null = null;

/** The launcher font menu: the built-in suggestions, then every installed font
 * (monospaced first), and the current value even if it isn't installed. */
async function fillFonts(menu: HTMLSelectElement, builtIn: string[], current: string) {
  fonts ??= await invoke<{ name: string; mono: boolean }[]>("installed_fonts").catch(() => []);
  const group = (label: string, names: string[]) => {
    const g = el("optgroup");
    g.label = label;
    for (const n of names) g.append(el("option", { value: n, textContent: n }));
    if (names.length) menu.append(g);
  };
  const seen = new Set(builtIn);
  const mono = fonts.filter((f) => f.mono && !seen.has(f.name)).map((f) => f.name);
  const other = fonts.filter((f) => !f.mono && !seen.has(f.name)).map((f) => f.name);
  group("Suggested", builtIn);
  group("Monospaced", mono);
  group("Other fonts", other);
  if (current && ![...builtIn, ...mono, ...other].includes(current)) {
    menu.prepend(el("option", { value: current, textContent: `${current} (not installed)` }));
  }
  menu.value = current;
}

/** Click, then press the keys: "Ctrl+Option+P". Esc cancels. */
function hotkeyInput(id: string, value: string, send: (v: unknown) => Promise<void>): HTMLInputElement {
  const input = el("input", { type: "text", id, value, readOnly: true, className: "hotkey" });
  let before = value;
  input.onfocus = () => {
    before = input.value;
    input.value = "press the keys…";
    input.classList.add("recording");
  };
  input.onblur = () => {
    input.classList.remove("recording");
    if (input.value === "press the keys…") input.value = before;
  };
  input.onkeydown = (e) => {
    e.preventDefault();
    if (e.key === "Escape") {
      input.blur();
      return;
    }
    if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return;
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Option", e.shiftKey && "Shift", e.metaKey && "Cmd"].filter(Boolean);
    if (!mods.length) {
      input.value = "add Ctrl, Option or Cmd";
      return;
    }
    const key = keyName(e.code);
    if (!key) return;
    input.value = [...mods, key].join("+");
    before = input.value;
    input.blur();
    void send(input.value);
  };
  return input;
}

/** The key's name as the hotkey parser reads it, from its position on the keyboard. */
function keyName(code: string): string | null {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (/^F\d+$/.test(code)) return code;
  const named: Record<string, string> = {
    Space: "Space", Enter: "Enter", Tab: "Tab", Backspace: "Backspace", Comma: "Comma", Period: "Period",
    Slash: "Slash", Semicolon: "Semicolon", Quote: "Quote", BracketLeft: "BracketLeft",
    BracketRight: "BracketRight", Backslash: "Backslash", Minus: "Minus", Equal: "Equal",
    Backquote: "Backquote", ArrowUp: "ArrowUp", ArrowDown: "ArrowDown", ArrowLeft: "ArrowLeft", ArrowRight: "ArrowRight",
  };
  return named[code] ?? null;
}

function presets(): HTMLElement {
  const styles = el("ul");
  for (const name of info!.styles ?? []) {
    const del = el("button", { type: "button", textContent: "Delete" });
    del.onclick = async () => {
      info!.styles = await invoke<string[]>("delete_style", { name });
      draw();
    };
    styles.append(el("li", {}, el("span", { className: "name", textContent: name }), el("code", { textContent: `--style=${name}` }), del));
  }
  const list = el("ul");
  for (const p of info!.presets) {
    const del = el("button", { type: "button", textContent: "Delete" });
    del.onclick = async () => {
      info!.presets = await invoke<Preset[]>("delete_preset", { name: p.name });
      draw();
    };
    list.append(el("li", {}, el("span", { className: "name", textContent: p.name }), el("code", { textContent: p.command, title: p.command }), del));
  }
  return el(
    "div",
    { className: "presets" },
    el("h3", { textContent: "Styles" }),
    el("p", { className: "muted", textContent: "A style is a look: theme, font, size, colours, line styles, markers, legend, grid, log axes and tick steps, without any data. Keep one with ⌘D on a preview; use it with any file by picking it after the columns, or by adding --style=NAME. Styles are files in the styles folder next to config.toml." }),
    info!.styles?.length ? styles : el("p", { textContent: "No styles yet." }),
    el("h3", { textContent: "Presets" }),
    el("p", { className: "muted", textContent: "A preset is a whole command (columns, labels, title and look) for files with the same columns. Keep one with ⌘⇧D on a preview; use it by picking a file and then the preset, or by typing: preset NAME FILE." }),
    info!.presets.length ? list : el("p", { textContent: "No presets yet." }),
  );
}

// ---------- colours: the launcher's theme (Palette's current theme, or a built-in one) ----------

interface UiTheme {
  mode: "light" | "dark";
  colors: Record<string, string>;
}
interface ThemeInfo {
  dark: UiTheme;
  light: UiTheme;
  look?: Look;
  stamp: number;
}
let themeShown = "";

/** Paints the window with the same colours as the launcher. Palette's keys become the
 *  variables shared.css uses (`--bg`, `--fg`…); without a theme the shared.css colours stay. */
async function applyTheme() {
  const theme = await invoke<ThemeInfo>("current_theme").catch(() => null);
  if (!theme) return;
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const t = dark ? theme.dark : theme.light;
  const key = JSON.stringify(t);
  // Palette's glass changed (or was handed back): redraw the Launcher tab's sliders.
  if (JSON.stringify(theme.look ?? null) !== JSON.stringify(info?.look ?? null)) void load();
  if (key === themeShown) return;
  themeShown = key;
  const c = t.colors;
  const root = document.documentElement.style;
  const set = (name: string, value: string | undefined) => value && root.setProperty(name, value);
  set("--bg", c.background);
  set("--fg", c.foreground);
  set("--muted", c.dark_foreground ?? c.muted);
  set("--accent", c.accent);
  set("--border", c.selection ?? c.muted);
  set("--error", c.red);
  // The command map's levels (Help).
  set("--l1", c.magenta);
  set("--l2", c.blue);
  set("--l3", c.green);
  set("--l4", c.yellow);
  set("--flag", c.cyan);
  // The highlight under a recorded hotkey or a search match: the accent, see-through.
  if (c.accent) set("--selected", `color-mix(in srgb, ${c.accent} 22%, transparent)`);
  root.colorScheme = t.mode;
}

// Follow a Palette theme switch (or a new launcher theme) while Settings is open.
setInterval(() => document.hasFocus() && void applyTheme(), 2000);
window.addEventListener("focus", () => void applyTheme());
window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => void applyTheme());

$<HTMLButtonElement>("open-config").onclick = () => void invoke("open_config_file");
// config.toml changed (here, in an editor, or from the launcher): redraw with the new values.
void listen("config-changed", () => {
  void load();
  void applyTheme();
});
void applyTheme();
void load();
