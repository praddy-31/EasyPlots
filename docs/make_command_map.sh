#!/bin/sh
# Draw Map 1 of docs/COMMAND_MAP.md as docs/command-map.svg (Settings → Help and the
# README show it). Run from the project folder after changing Map 1:
#
#   sh docs/make_command_map.sh
#
# Uses mermaid-cli through npx (fetched on first use, not a project dependency) and the
# Chromium that Playwright installed for the launcher tests, so no second browser is
# downloaded. Plain SVG text (no HTML labels), so it reads the same in any viewer.
set -eu
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The first ```mermaid block after "## Map 1".
awk '/^## Map 1/{m=1} m&&/^```mermaid/{f=1;next} f&&/^```/{exit} f' docs/COMMAND_MAP.md | sed "1s/flowchart TD/flowchart LR/" > "$tmp/map.mmd"
[ -s "$tmp/map.mmd" ] || { echo "no Map 1 block in docs/COMMAND_MAP.md" >&2; exit 1; }

chrome=$(ls -d "$HOME"/Library/Caches/ms-playwright/chromium-*/chrome-mac*/*.app/Contents/MacOS/* 2>/dev/null | head -1 || true)
[ -n "$chrome" ] || chrome=$(ls -d "$HOME"/.cache/ms-playwright/chromium-*/chrome-linux*/chrome 2>/dev/null | head -1 || true)
[ -n "$chrome" ] || { echo "no Playwright Chromium found: run npx playwright install chromium" >&2; exit 1; }
printf '{"executablePath": "%s"}\n' "$chrome" > "$tmp/puppeteer.json"
printf '{"theme": "neutral", "flowchart": {"htmlLabels": false, "wrappingWidth": 300}, "fontFamily": "Helvetica, Arial, sans-serif"}\n' > "$tmp/mermaid.json"

PUPPETEER_SKIP_DOWNLOAD=1 npx -y @mermaid-js/mermaid-cli@11.4.2 \
  -i "$tmp/map.mmd" -o docs/command-map.svg \
  -p "$tmp/puppeteer.json" -c "$tmp/mermaid.json" -b white
# Mermaid writes width="100%", which an <img> can't size from: use the drawing's own size.
python3 - <<'PY'
import re
p = "docs/command-map.svg"
s = open(p).read()
w, h = re.search(r'viewBox="[-\d.]+ [-\d.]+ ([\d.]+) ([\d.]+)"', s).groups()
s = re.sub(r'<svg([^>]*?) width="100%"', rf'<svg\1 width="{w}" height="{h}"', s, count=1)
s = re.sub(r'(<svg[^>]*?) style="max-width: [\d.]+px;?"', r"\1", s, count=1)
open(p, "w").write(s)
PY
echo "wrote docs/command-map.svg"
