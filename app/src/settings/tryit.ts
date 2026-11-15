// The "Try it" tab: short lessons, each a sample file, a command and the picture it
// gives. The lessons and the sample data are built into the app (core's guide.rs and
// demo.rs); "Open in launcher" puts the command in the launcher ready to run. The
// pictures are drawn by the engine when the tab opens, one after another, so the first
// ones appear at once.

import { invoke } from "@tauri-apps/api/core";

interface Lesson {
  section: string;
  title: string;
  text: string;
  command: string;
}

/** Pictures drawn (or being drawn) this session, by command. */
const pictures = new Map<string, Promise<string>>();

function picture(command: string): Promise<string> {
  let p = pictures.get(command);
  if (!p) {
    p = invoke<string>("guide_preview", { command }).catch(() => "");
    pictures.set(command, p);
  }
  return p;
}

export function tryItView(): HTMLElement {
  const box = document.createElement("div");
  box.className = "help tryit";
  const intro = document.createElement("p");
  intro.textContent =
    "Each lesson is a command on the built-in sample data (the demo/ files — nothing " +
    "on your disk). Open one in the launcher, press ↵ to draw it, then change it and " +
    "see what happens.";
  box.append(intro);
  void fill(box);
  return box;
}

async function fill(box: HTMLElement) {
  const lessons = await invoke<Lesson[]>("guide_lessons");
  const holders: [Lesson, HTMLElement][] = [];
  let section = "";
  for (const l of lessons) {
    if (l.section !== section) {
      section = l.section;
      const h = document.createElement("h2");
      h.textContent = section;
      box.append(h);
    }
    const card = document.createElement("div");
    card.className = "lesson";
    const title = document.createElement("h3");
    title.textContent = l.title;
    const text = document.createElement("p");
    text.textContent = l.text;
    const row = document.createElement("div");
    row.className = "lesson-command";
    const code = document.createElement("code");
    code.textContent = l.command;
    const open = document.createElement("button");
    open.type = "button";
    open.textContent = "Open in launcher";
    open.onclick = () => void invoke("try_lesson", { command: l.command });
    row.append(code, open);
    const picture = document.createElement("div");
    picture.className = "lesson-picture";
    card.append(title, text, row, picture);
    box.append(card);
    holders.push([l, picture]);
  }
  await drawPictures(holders);
}

/** Draw the pictures one by one; a redraw of the tab reuses the ones already drawn. */
async function drawPictures(holders: [Lesson, HTMLElement][]) {
  for (const [l, holder] of holders) {
    const svg = await picture(l.command);
    if (svg && holder.isConnected) holder.innerHTML = svg;
  }
}
