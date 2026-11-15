// The command map in Help: the four levels of a command (figure, panel, layer, series)
// and the options that belong to each, drawn three ways (owner's pick, round 13: the
// mockups in docs/mockups/command-map.html "are good"): a tree, cards side by side, and
// a terminal-style box drawing. Colours come from the window's theme (Palette's keys),
// the lettering is the launcher's JetBrains Mono with Nerd Font icons.
//
// The option lists are checked against the engine's suggestions by app/tests/settings-ui.mjs,
// so a renamed option can't linger here.

export interface Level {
  key: "l1" | "l2" | "l3" | "l4";
  icon: string;
  name: string;
  what: string;
  /** An example command; <b>…</b> marks the part this level adds. */
  example: string;
  groups: [string, string][];
}

export const LEVELS: Level[] = [
  {
    key: "l1", icon: "\u{f03e}", name: "Figure", what: "the whole picture · options start with --",
    example: "line a.csv y=temp ; hist a.csv x=temp <b>--size=acs</b>",
    groups: [
      ["Page", "--size= --theme= --font= --dpi= --format= --out= --style="],
      ["Panels", "--layout= --widths= --heights= --sharex --sharey --letters --suptitle="],
      ["Legend", "--legend="],
    ],
  },
  {
    key: "l2", icon: "\u{f201}", name: "Panel", what: "one plot · a new one after ;",
    example: 'line a.csv y=temp <b>title="Run 3" logy</b>',
    groups: [
      ["Axes", "title= xlabel= ylabel= xlim= ylim= logx logy legend= grid= ybreak="],
      ["Second axes", "y2label= y2lim= logy2 x2= x2label= y2="],
      ["Marks", "hline= vline= span= yspan= note= arrow= rect= circle= segment= inset="],
      ["Plot type", "bins= density stacked cmap= zlim= zlabel= levels= filled values bw= angle= scale= window="],
      ["Groups", "horizontal highlight= mean median jitter summary"],
      ["Reading curves", "peaks dips onset area= cross= crosslabel= corr"],
      ["Fits", "fitlabel= fitdigits= fitnames= fitstats residuals"],
    ],
  },
  {
    key: "l3", icon: "\u{f0ce}", name: "Layer", what: "a type and a file · another after +",
    example: "line a.csv y=temp <b>+ b.csv y=temp axis=right</b>",
    groups: [
      ["Data", "x= y= z= c= sheet= skiplines= skiprows= every= clip= where= dropna"],
      ["Change values", "smooth= baseline= showbaseline norm= slope integral"],
      ["Errors and fits", "err= xerr= band stat= fit= fitrange= start= fitband through0 weighted"],
      ["Placement", "axis= step= by= groups="],
    ],
  },
  {
    key: "l4", icon: "\u{f292}", name: "Series", what: "one line, set of points or bars",
    example: "line a.csv <b>y=temp,pressure color=red,blue</b>",
    groups: [["Look (one value each)", "label= color= marker= dash= width= alpha= markers"]],
  },
];

/** Every option the map names, for the test that checks them against the engine. */
export const MAP_OPTIONS: string[] = LEVELS.flatMap((l) => l.groups.flatMap(([, o]) => o.split(" ")));

export type Design = "cards" | "tree" | "terminal";
export const DESIGNS: [Design, string][] = [["cards", "Cards"], ["tree", "Tree"], ["terminal", "Terminal"]];

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
/** The example with its <b> kept and everything else escaped. */
const example = (s: string) => esc(s).replace(/&lt;b>/g, "<b>").replace(/&lt;\/b>/g, "</b>");

/** An option chip: flags (no `=`) tinted, matches of the search marked. */
function chip(o: string, words: string[]): string {
  const hit = words.length > 0 && words.some((w) => o.toLowerCase().includes(w));
  return `<code class="opt${o.includes("=") ? "" : " flag"}${hit ? " hit" : ""}">${esc(o)}</code>`;
}
const chips = (text: string, words: string[]) => text.split(" ").map((o) => chip(o, words)).join("");

function tree(words: string[]): string {
  const nest = (i: number): string => {
    if (i >= LEVELS.length) return "";
    const l = LEVELS[i];
    return `<ul><li><span class="node ${l.key}"><span class="nf">${l.icon}</span> ${l.name}</span><span class="what">${esc(l.what)}</span>
      <div class="opts example">e.g. ${example(l.example)}</div>
      ${l.groups.map(([g, o]) => `<div class="opts"><span class="group">${esc(g)}</span>${chips(o, words)}</div>`).join("")}
      ${nest(i + 1)}</li></ul>`;
  };
  return `<div class="map-tree">${nest(0)}</div>`;
}

function cards(words: string[]): string {
  const arrows = ["one per command", "separated by ;", "joined by +", "one per column or group"];
  return `<div class="map-arrows">${arrows.map((a) => `<span>${a}</span>`).join("")}</div>
    <div class="map-cards">${LEVELS.map((l) => `<div class="map-card ${l.key}">
      <h3><span class="nf">${l.icon}</span> ${l.name}</h3><div class="sub">${esc(l.what)}</div>
      <div class="example">${example(l.example)}</div>
      ${l.groups.map(([g, o]) => `<h4>${esc(g)}</h4><div>${chips(o, words)}</div>`).join("")}</div>`).join("")}</div>`;
}

/** Box drawing, wrapped to `width` characters. */
function terminal(words: string[], width = 72): string {
  const wrap = (text: string, w: number) => {
    const out: string[] = [];
    let line = "";
    for (const word of text.split(" ")) {
      if ((line + " " + word).trim().length > w) {
        out.push(line.trim());
        line = "";
      }
      line += " " + word;
    }
    if (line.trim()) out.push(line.trim());
    return out;
  };
  const label = 18;
  let t = "";
  LEVELS.forEach((l, i) => {
    const pad = "  ".repeat(i);
    const W = width - pad.length;
    const head = ` ${l.name} — ${l.what} `;
    // The icon is drawn two letters wide (a Nerd Font glyph is wider than one).
    t += `${pad}<span class="${l.key}">╭─<span class="nf icon2">${l.icon}</span>${esc(head)}${"─".repeat(Math.max(2, W - head.length - 4))}╮</span>\n`;
    for (const [g, o] of l.groups) {
      wrap(o, W - label - 3).forEach((row, k) => {
        const cells = row.split(" ").map((x) => {
          const hit = words.length > 0 && words.some((w) => x.toLowerCase().includes(w));
          return `<span class="${x.includes("=") ? "" : "flag"}${hit ? " hit" : ""}">${esc(x)}</span>`;
        }).join(" ");
        t += `${pad}<span class="${l.key}">│</span> <span class="muted">${esc((k ? "" : g).padEnd(label))}</span>${cells}${" ".repeat(Math.max(0, W - label - 3 - row.length))}<span class="${l.key}">│</span>\n`;
      });
    }
    t += `${pad}<span class="${l.key}">╰${"─".repeat(W - 2)}╯</span>\n`;
    if (i < LEVELS.length - 1) t += `${pad}  <span class="muted">└─ contains</span>\n`;
  });
  return `<pre class="map-term">${t}</pre>`;
}

/** Does the map have anything for these search words? */
export function mapMatches(words: string[]): boolean {
  const text = LEVELS.map((l) => [l.name, l.what, ...l.groups.flat()].join(" ")).join(" ").toLowerCase();
  return words.every((w) => text.includes(w)) || words.some((w) => MAP_OPTIONS.some((o) => o.toLowerCase().includes(w)));
}

const STORE = "easyplots.map-design";
function remembered(): Design {
  try {
    const d = localStorage.getItem(STORE);
    if (d === "tree" || d === "terminal" || d === "cards") return d;
  } catch {
    // No storage (private window, tests): the default.
  }
  return "cards";
}

/** The map with its design switch; search words mark the options they match. */
export function commandMap(words: string[]): HTMLElement {
  const box = document.createElement("div");
  box.className = "command-map";
  let design = remembered();
  const draw = () => {
    const body = design === "tree" ? tree(words) : design === "terminal" ? terminal(words) : cards(words);
    box.innerHTML = `<div class="map-switch" role="tablist">${DESIGNS.map(([d, name]) =>
      `<button type="button" data-design="${d}" class="${d === design ? "on" : ""}">${name}</button>`).join("")}</div>${body}`;
    for (const b of box.querySelectorAll<HTMLButtonElement>(".map-switch button")) {
      b.onclick = () => {
        design = b.dataset.design as Design;
        try {
          localStorage.setItem(STORE, design);
        } catch {
          // Not remembered; still switches.
        }
        draw();
      };
    }
  };
  draw();
  return box;
}
