// The Help tab: docs/FEATURES.md (bundled into the app when it's built), drawn as headings,
// tables, lists and paragraphs, with a search box that keeps only the matching rows.
import features from "../../../docs/FEATURES.md?raw";
import { commandMap, mapMatches } from "./commandmap";

/** Pictures FEATURES.md may show (`![alt](name)`): the command map is drawn live
 *  (commandmap.ts) in place of its picture. */
const IMAGES: Record<string, string> = { "command-map.svg": "map" };

type Block =
  | { kind: "table"; head: string[]; rows: string[][] }
  | { kind: "list"; items: string[] }
  | { kind: "text"; text: string }
  | { kind: "image"; alt: string; src: string };

interface Section {
  title: string;
  blocks: Block[];
}

/** Split the Markdown into sections (## headings) of tables, lists and paragraphs. */
function parse(md: string): Section[] {
  const sections: Section[] = [{ title: "", blocks: [] }];
  const cells = (line: string) =>
    line.trim().replace(/^\||\|$/g, "").split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|"));
  const lines = md.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const blocks = sections[sections.length - 1].blocks;
    if (line.startsWith("## ")) {
      sections.push({ title: line.slice(3).replace(/^\d+\.\s*/, ""), blocks: [] });
    } else if (line.startsWith("# ") || /^-{3,}$/.test(line.trim()) || !line.trim()) {
      continue;
    } else if (/^!\[[^\]]*\]\([^)]+\)\s*$/.test(line)) {
      const [, alt, src] = line.match(/^!\[([^\]]*)\]\(([^)]+)\)/)!;
      if (IMAGES[src]) blocks.push({ kind: "image", alt, src: IMAGES[src] });
    } else if (line.startsWith("|")) {
      const head = cells(line);
      const rows: string[][] = [];
      i += 1; // the |---| line
      while (lines[i + 1]?.startsWith("|")) rows.push(cells(lines[++i]));
      blocks.push({ kind: "table", head, rows });
    } else if (line.startsWith("- ")) {
      const items = [line.slice(2)];
      while (lines[i + 1] && (lines[i + 1].startsWith("- ") || lines[i + 1].startsWith("  "))) {
        const next = lines[++i];
        if (next.startsWith("- ")) items.push(next.slice(2));
        else items[items.length - 1] += " " + next.trim();
      }
      blocks.push({ kind: "list", items });
    } else {
      let text = line;
      while (lines[i + 1]?.trim() && !/^(\||- |#|-{3})/.test(lines[i + 1])) text += " " + lines[++i].trim();
      blocks.push({ kind: "text", text });
    }
  }
  return sections.filter((s) => s.blocks.length);
}

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** `code`, **bold** and *italic*, with the search words marked. */
function inline(s: string, words: string[]): string {
  const mark = (t: string) =>
    words.reduce((acc, w) => acc.replace(new RegExp(`(${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})(?![^<]*>)`, "gi"), "<mark>$1</mark>"), t);
  return s
    .split(/(`[^`]+`)/)
    .map((part) =>
      part.startsWith("`") && part.endsWith("`") && part.length > 1
        ? `<code>${mark(escape(part.slice(1, -1)))}</code>`
        : mark(escape(part).replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>").replace(/\*([^*]+)\*/g, "<i>$1</i>")),
    )
    .join("");
}

const sections = parse(features);

/** The help, keeping only what matches every word typed (all of it with no words). */
export function helpView(): HTMLElement {
  const box = document.createElement("div");
  box.className = "help";
  const search = document.createElement("input");
  search.type = "text";
  search.placeholder = "Search the help: fit, colour, skip, ⌘P…";
  search.className = "help-search";
  const body = document.createElement("div");
  const draw = () => {
    const words = search.value.toLowerCase().split(/\s+/).filter(Boolean);
    const hit = (...texts: string[]) => {
      const t = texts.join(" ").toLowerCase();
      return words.every((w) => t.includes(w));
    };
    let html = "";
    for (const s of sections) {
      let inner = "";
      const titleHit = words.length > 0 && hit(s.title);
      for (const b of s.blocks) {
        if (b.kind === "table") {
          const rows = b.rows.filter((r) => titleHit || hit(...r));
          if (!rows.length) continue;
          inner += `<table><thead><tr>${b.head.map((h) => `<th>${inline(h, [])}</th>`).join("")}</tr></thead><tbody>`;
          inner += rows.map((r) => `<tr>${r.map((c) => `<td>${inline(c, words)}</td>`).join("")}</tr>`).join("");
          inner += "</tbody></table>";
        } else if (b.kind === "list") {
          const items = b.items.filter((it) => titleHit || hit(it));
          if (items.length) inner += `<ul>${items.map((it) => `<li>${inline(it, words)}</li>`).join("")}</ul>`;
        } else if (b.kind === "image") {
          if (!words.length || titleHit || hit(b.alt) || mapMatches(words))
            inner += `<p>${inline(b.alt, words)}</p><div class="map-slot"></div>`;
        } else if (titleHit || hit(b.text)) {
          inner += `<p>${inline(b.text, words)}</p>`;
        }
      }
      if (inner) html += (s.title ? `<h2>${inline(s.title, words)}</h2>` : "") + inner;
    }
    body.innerHTML = html || `<p class="muted">Nothing matches “${escape(search.value)}”.</p>`;
    for (const slot of body.querySelectorAll<HTMLElement>(".map-slot")) slot.replaceWith(commandMap(words));
  };
  search.addEventListener("input", draw);
  draw();
  box.append(search, body);
  setTimeout(() => search.focus());
  return box;
}
