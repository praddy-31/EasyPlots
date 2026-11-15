// Settings window test: the built settings page (app/dist) in Chromium with stand-in
// answers instead of the Rust side. The option list comes from the real engine
// (`easyplots --settings-fields`), so the form is tested against what the app sends.
//
//   npm --prefix app run build && node app/tests/settings-ui.mjs
import { chromium } from "playwright";
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const DIST = here("../dist");
const OUT = here("./out");
mkdirSync(OUT, { recursive: true });
const info = JSON.parse(
  execFileSync("cargo", ["run", "-q", "-p", "easyplots-cli", "--", "--settings-json"], { cwd: here("../.."), encoding: "utf8" }),
);
info.presets = [{ name: "thesis", command: "line a.csv x=t y=temp --style=thesis" }];
info.styles = ["bright"];
const server = spawn("python3", ["-m", "http.server", "4174", "--directory", DIST], { stdio: "ignore" });
await new Promise((r) => setTimeout(r, 800));

const browser = await chromium.launch(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {});
let failures = 0;
const check = (ok, what) => { console.log(`${ok ? "ok  " : "FAIL"} ${what}`); if (!ok) failures++; };

const page = await browser.newPage({ viewport: { width: 720, height: 560 } });
page.on("pageerror", (e) => { console.log("page error:", e.message); failures++; });
await page.addInitScript((info) => {
  window.__calls = [];
  window.__look = { opacity: info.values.launcher.opacity, blur: 20, palette: null, faint: false };
  let cb = 0;
  const handlers = {
    // The launcher's look: config's own values until a test hands the glass to Palette.
    settings_info: () => ({ ...info, look: window.__look }),
    // A Palette theme (Kanagawa-like) so the window's colours can be checked.
    current_theme: () => {
      const t = { mode: "dark", colors: { background: "#1f1f28", foreground: "#dcd7ba", accent: "#7e9cd8", selection: "#2d4f67", dark_foreground: "#727169", red: "#e82424" } };
      return { dark: t, light: t, source: "Palette: kanagawa", look: window.__look, stamp: 1 };
    },
    set_setting: ({ section, key, value }) => {
      // No floor for readability any more (owner, 2026-11-01): only above 1 is wrong.
      if (key === "opacity" && (value < 0 || value > 1)) throw "launcher.opacity must be between 0 and 1";
      info.values[section][key] = value;
      return info.values;
    },
    pick_folder: () => null,
    installed_fonts: () => [{ name: "Fira Code", mono: true }, { name: "Menlo", mono: true }, { name: "Helvetica Neue", mono: false }],
    open_config_file: () => null,
    delete_preset: ({ name }) => (info.presets = info.presets.filter((p) => p.name !== name)),
    delete_style: ({ name }) => (info.styles = info.styles.filter((s) => s !== name)),
    "plugin:event|listen": () => ++cb,
  };
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => ++cb,
    invoke: async (cmd, args) => {
      window.__calls.push([cmd, args]);
      if (!handlers[cmd]) throw new Error("no handler " + cmd);
      return handlers[cmd](args);
    },
    metadata: { currentWindow: { label: "settings" }, currentWebview: { windowLabel: "settings", label: "settings" } },
  };
}, info);
await page.goto("http://localhost:4174/settings/index.html");
await page.waitForTimeout(400);
const calls = (name) => page.evaluate((n) => window.__calls.filter((c) => c[0] === n), name);
const tab = async (name) => { await page.click(`nav button:text("${name}")`); await page.waitForTimeout(100); };

const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
const fg = await page.evaluate(() => getComputedStyle(document.body).color);
check(bg === "rgb(31, 31, 40)" && fg === "rgb(220, 215, 186)", `Settings uses the Palette theme (${bg}, ${fg})`);

check((await page.$$eval("nav button", (b) => b.map((x) => x.textContent))).join(",") === "General,Saving,Plot look,Launcher,Presets & styles,Try it,Help", "tabs");
check(await page.isVisible("#general-hotkey"), "General shows the hotkey");
await page.screenshot({ path: `${OUT}/s1_general.png` });

// Hotkey recorder: click, press the keys.
await page.click("#general-hotkey");
await page.keyboard.press("Control+Alt+KeyK");
await page.waitForTimeout(100);
const hk = (await calls("set_setting")).at(-1);
check(hk && hk[1].key === "hotkey" && hk[1].value === "Ctrl+Option+K", `hotkey recorder: ${JSON.stringify(hk?.[1])}`);

// Saving: the format the owner asked for.
await tab("Saving");
check(await page.isVisible("#output-format"), "Saving has the file format");
await page.selectOption("#output-format", "pdf");
await page.waitForTimeout(100);
check((await calls("set_setting")).some((c) => c[1].key === "format" && c[1].value === "pdf"), "format change is saved");
await page.screenshot({ path: `${OUT}/s2_saving.png` });

// The opacity slider starts where the value is and saves as it moves (owner's bug: stuck).
await tab("Launcher");
const opacity = info.values.launcher.opacity;
check(Number(await page.inputValue('input[type="range"]')) === opacity, `slider starts at the opacity (${opacity})`);
await page.focus('input[type="range"]');
await page.keyboard.press("ArrowLeft");
await page.keyboard.press("ArrowLeft");
await page.waitForTimeout(300);
const moved = (await calls("set_setting")).filter((c) => c[1].key === "opacity").at(-1);
check(moved && Math.abs(moved[1].value - (opacity - 0.1)) < 1e-9, `moving the slider saves it: ${JSON.stringify(moved?.[1])}`);
check(Number(await page.inputValue("#launcher-opacity")) === moved?.[1].value, "the number box follows the slider");
// Every installed font is offered for the launcher (owner: "why only 5?"), in a real menu
// (owner: the Mac showed a bare text box).
const fonts = await page.$$eval("select#launcher-font option", (o) => o.map((x) => x.value));
check(fonts.includes("Fira Code") && fonts.includes("Helvetica Neue") && fonts.includes("JetBrains Mono"), `installed fonts in a menu: ${fonts.join(", ")}`);
check((await page.inputValue("select#launcher-font")) === info.values.launcher.font, "the menu shows the current font");
await page.selectOption("select#launcher-font", "Fira Code");
await page.waitForTimeout(100);
check((await calls("set_setting")).some((c) => c[1].key === "font" && c[1].value === "Fira Code"), "choosing a font saves it");
// Size boxes: common values in a menu beside them.
await page.selectOption("#launcher-size + select.suggest", { index: 1 });
await page.waitForTimeout(100);
check((await calls("set_setting")).some((c) => c[1].key === "size" && c[1].section === "launcher"), "a suggested size can be picked from the menu");

// A bad value shows the error under the option.
await page.fill("#launcher-opacity", "2");
await page.dispatchEvent("#launcher-opacity", "change");
await page.waitForTimeout(100);
check((await page.textContent(".row .error:not([hidden])")).includes("between"), "bad value: error shown");
await page.screenshot({ path: `${OUT}/s3_launcher.png` });

// Very see-through is allowed, with a warning (owner: warn, never cap).
await page.fill("#launcher-opacity", "0.2");
await page.dispatchEvent("#launcher-opacity", "change");
await page.waitForTimeout(100);
check((await calls("set_setting")).some((c) => c[1].key === "opacity" && c[1].value === 0.2), "opacity 0.2 is saved (no lower limit)");
const warning = await page.textContent(".row .warn-line:not([hidden])").catch(() => "");
check(warning.includes("80 % see-through") && warning.includes("only a warning"), `below 40 %: a warning, not an error: ${warning}`);
check(Number(await page.getAttribute("#launcher-blur", "max")) === 100, "blur is a strength 0–100");
check((await page.$$('.row:has(#launcher-blur) input[type="range"]')).length === 1, "blur has a slider");

// Palette sets the glass: a banner, Palette's values, and the controls dimmed.
await page.evaluate(() => {
  window.__look = { opacity: 0.05, blur: 60, palette: { kind: "blurred", opacity: 0.05, blur: 60 }, faint: true };
  window.dispatchEvent(new Event("focus"));
});
await page.waitForTimeout(300);
const banner = await page.textContent(".palette-banner").catch(() => "");
check(banner.includes("Glass set by Palette: Blurred") && banner.includes("95 % see-through") && banner.includes("blur 60"), `Palette banner: ${banner}`);
check(await page.isDisabled("#launcher-opacity") && await page.isDisabled("#launcher-blur"), "opacity and blur dimmed while Palette sets them");
check(Number(await page.inputValue("#launcher-opacity")) === 0.05 && Number(await page.inputValue("#launcher-blur")) === 60, "they show Palette's values (95 % see-through, not capped)");
check((await page.textContent(".row .warn-line:not([hidden])")).includes("(from Palette)"), "the warning also covers Palette's value");
await page.screenshot({ path: `${OUT}/s3b_palette_glass.png` });
// Handed back ("Yours" in Palette): the controls come back.
await page.evaluate(() => {
  window.__look = { opacity: 0.9, blur: 20, palette: null, faint: false };
  window.dispatchEvent(new Event("focus"));
});
await page.waitForTimeout(300);
check(!(await page.$(".palette-banner")) && !(await page.isDisabled("#launcher-opacity")), "Yours in Palette: banner gone, sliders back");

await tab("Presets & styles");
check((await page.textContent(".presets")).includes("thesis") && (await page.textContent(".presets")).includes("--style=bright"), "styles and presets listed");
await page.screenshot({ path: `${OUT}/s4_presets_styles.png` });
await page.click(".presets ul:first-of-type button");
await page.waitForTimeout(100);
check((await page.textContent(".presets")).includes("No styles yet"), "style deleted");
await page.click(".presets button");
await page.waitForTimeout(100);
check((await page.textContent(".presets")).includes("No presets yet"), "preset deleted");

await tab("Help");
check((await page.$$(".help table tr")).length > 50, "Help shows the feature list");
await page.waitForTimeout(200);
// The command map, drawn live in three designs (owner, round 13: "implement the mockup maps").
check((await page.$$(".command-map .map-card")).length === 4, "the map opens as four cards");
const chipsShown = await page.$$eval(".command-map code.opt", (c) => c.map((x) => x.textContent));
// Every option on the map is one the engine still suggests (a rename can't linger here).
const known = new Set();
for (const typed of ["line simple.csv ", "scatter simple.csv ", "bar simple.csv ", "hist simple.csv ", "heatmap simple.csv ",
  "contour simple.csv ", "kde simple.csv ", "polar simple.csv ", "fft simple.csv ", "strip simple.csv ", "box simple.csv ",
  "area simple.csv ", "step simple.csv ", "line simple.csv --"]) {
  const out = execFileSync("cargo", ["run", "-q", "-p", "easyplots-cli", "--", "--complete", typed], { cwd: here("../.."), encoding: "utf8" });
  for (const item of JSON.parse(out).items) known.add(item.label);
}
const unknown = chipsShown.filter((o) => !known.has(o));
check(chipsShown.length > 100 && unknown.length === 0, `every map option is a real one (${chipsShown.length}; unknown: ${unknown.join(" ")})`);
await page.$eval(".command-map", (f) => f.scrollIntoView());
await page.screenshot({ path: `${OUT}/s5_help_map_cards.png` });
for (const [design, sel] of [["tree", ".map-tree"], ["terminal", ".map-term"]]) {
  await page.click(`.map-switch button[data-design="${design}"]`);
  await page.waitForTimeout(100);
  check(await page.isVisible(`.command-map ${sel}`), `the map switches to ${design}`);
  await page.$eval(".command-map", (f) => f.scrollIntoView());
  await page.screenshot({ path: `${OUT}/s5_help_map_${design}.png` });
}
await page.click('.map-switch button[data-design="cards"]');
await page.fill(".help-search", "fit band");
await page.waitForTimeout(100);
const helpText = await page.textContent(".help");
check(helpText.includes("fitband") && !helpText.includes("Heatmap"), "Help search keeps only matching rows");
check((await page.$$(".command-map .hit")).length > 0, "the search marks fitband on the map too");
check((await page.$$(".help mark")).length > 0, "search words are marked");
await page.screenshot({ path: `${OUT}/s5_help.png` });

await browser.close();
server.kill();
console.log(failures ? `${failures} FAILED` : "all passed");
process.exit(failures ? 1 : 0);
