"""Build the launcher's fonts in app/src/fonts/.

- JetBrains Mono Regular and Bold (OFL), from the @fontsource/jetbrains-mono npm package.
- Symbols Nerd Font (MIT), from the nerdfonts-web npm package, cut down to the icons the
  launcher uses (ICONS below) so it stays a few KB. Add a code point here to use a new icon.

Usage (needs `pip install fonttools brotli` and npm):
  mkdir /tmp/uifonts && cd /tmp/uifonts
  npm pack @fontsource/jetbrains-mono nerdfonts-web && for f in *.tgz; do tar xzf $f && mv package ${f%.tgz}; done
  python3 <repo>/assets/fonts/ui/make_ui_fonts.py /tmp/uifonts
"""
import shutil
import sys
from pathlib import Path

from fontTools import subset

ICONS = {
    0xF0CE: "table (CSV file)",
    0xF201: "line chart (prompt, line plots)",
    0xF080: "bar chart",
    0xF2A1: "dots (scatter)",
    0xF1FE: "area chart",
    0xF292: "# (number column)",
    0xF073: "calendar (date column)",
    0xF031: "A (text column)",
    0xF00C: "check (ticked)",
    0xF1DA: "history",
    0xF120: "terminal (command mode)",
    0xF1FB: "eyedropper (colours)",
    0xF071: "warning (errors)",
    0xF0C7: "floppy (saved)",
    0xF03E: "picture (preview)",
    0xF1DE: "sliders (options)",
    0xF054: "chevron (row number)",
    0xF07C: "folder",
    0xF0C5: "copy",
    0xF002: "search",
    0xF00D: "close",
    0xF08D: "thumb tack (pinned)",
}

src = Path(sys.argv[1])
out = Path(__file__).resolve().parents[3] / "app" / "src" / "fonts"
out.mkdir(parents=True, exist_ok=True)
files = next(p for p in src.glob("*fontsource-jetbrains-mono*") if p.is_dir()) / "files"
shutil.copy(files / "jetbrains-mono-latin-400-normal.woff2", out / "JetBrainsMono-Regular.woff2")
shutil.copy(files / "jetbrains-mono-latin-700-normal.woff2", out / "JetBrainsMono-Bold.woff2")
symbols = next(next(p for p in src.glob("*nerdfonts-web*") if p.is_dir()).glob("*.woff2"))
options = subset.Options()
options.flavor = "woff2"
font = subset.load_font(str(symbols), options)
subsetter = subset.Subsetter(options)
subsetter.populate(unicodes=list(ICONS))
subsetter.subset(font)
subset.save_font(font, str(out / "SymbolsNerdFont.woff2"), options)
print("wrote", sorted(p.name for p in out.glob("*.woff2")))
