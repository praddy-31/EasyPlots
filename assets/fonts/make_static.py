"""Cut fixed-weight fonts (Regular, Bold, Italic) from Google Fonts' variable font files.

The plot renderer (resvg/svg2pdf) only uses the default weight of a variable font, so
EasyPlots bundles separate files instead.

Usage (needs `pip install fonttools`), e.g. for STIX Two Text:
  curl -LO 'https://raw.githubusercontent.com/google/fonts/main/ofl/stixtwotext/STIXTwoText%5Bwght%5D.ttf'
  curl -LO 'https://raw.githubusercontent.com/google/fonts/main/ofl/stixtwotext/STIXTwoText-Italic%5Bwght%5D.ttf'
  python3 assets/fonts/make_static.py "STIX Two Text" STIXTwoText 'STIXTwoText[wght].ttf' 'STIXTwoText-Italic[wght].ttf'
and for Source Sans 3 (ofl/sourcesans3/SourceSans3[wght].ttf and SourceSans3-Italic[wght].ttf):
  python3 assets/fonts/make_static.py "Source Sans 3" SourceSans3 'SourceSans3[wght].ttf' 'SourceSans3-Italic[wght].ttf'

Latin Modern (LM Roman 10) is used as published (static OTF files from CTAN,
fonts/lm/fonts/opentype/public/lm/lmroman10-{regular,italic,bold}.otf).

STIXTwoMath-Symbols.otf is a cut of STIX Two Math (CTAN fonts/stix2-otf) to Greek, arrows
and maths operators, the fallback for symbols the text fonts lack:
  python3 -m fontTools.subset STIXTwoMath-Regular.otf \
    --unicodes="U+0020-007E,U+00A0-00FF,U+0391-03C9,U+03D1,U+03D5,U+03F5,U+2000-206F,U+2100-214F,U+2190-21FF,U+2200-22FF,U+27E8-27E9" \
    --layout-features='' --no-hinting --name-IDs='*' --output-file=assets/fonts/STIXTwoMath-Symbols.otf
"""
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

OUT = Path(__file__).parent


def cut(family, prefix, src, weight, italic, style_name):
    font = instancer.instantiateVariableFont(TTFont(src), {"wght": weight})
    name = font["name"]
    for rec in list(name.names):
        if rec.nameID in (16, 17, 25):  # typographic names and variations prefix
            name.removeNames(nameID=rec.nameID)
    name.setName(family, 1, 3, 1, 0x409)
    name.setName(style_name, 2, 3, 1, 0x409)
    name.setName(f"{family} {style_name}", 4, 3, 1, 0x409)
    name.setName(f"{prefix}-{style_name.replace(' ', '')}", 6, 3, 1, 0x409)
    os2 = font["OS/2"]
    os2.usWeightClass = weight
    bold = weight >= 700
    # fsSelection bits: 0 italic, 5 bold, 6 regular
    os2.fsSelection = (os2.fsSelection & ~0b1100001) | (1 if italic else 0) | (0b100000 if bold else 0) \
        | (0b1000000 if not (bold or italic) else 0)
    font["head"].macStyle = (1 if bold else 0) | (2 if italic else 0)
    out = f"{prefix}-{style_name.replace(' ', '')}.ttf"
    font.save(OUT / out)
    print("wrote", out)


if __name__ == "__main__":
    family, prefix, upright, italic = sys.argv[1:5]
    cut(family, prefix, upright, 400, False, "Regular")
    cut(family, prefix, upright, 700, False, "Bold")
    cut(family, prefix, italic, 400, True, "Italic")
