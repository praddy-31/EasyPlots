# How a command is built: the map

Every EasyPlots command is a small tree. A **figure** holds one or more **panels**
(separated by `;`); a panel holds one or more **layers** (separated by `+`, one per
`TYPE FILE`); a layer draws one **series** per y column. Each option belongs to exactly
one of these levels. This is the same tree as matplotlib's figure → axes → artists.

Map 1 is the command as it is (`docs/command-map.svg` is the same map as a picture:
redraw it with `docs/make_command_map.sh` when Map 1 changes; the Help tab in Settings
draws its own). An option is written once per level, wherever it stands; only marks
(`hline=`, `note=`…) repeat.
Map 2 is what is still planned (dashed boxes). Zed, GitHub and VS Code draw these blocks
as diagrams.

## Map 1: today

```mermaid
flowchart TD
    CMD["Command text<br/>line a.csv y=temp + b.csv y=p ; hist a.csv x=temp --size=acs"]
    CMD --> FIG

    FIG["Figure<br/>the whole picture<br/>its options start with --"]
    FIG --> PAN1["Panel 1"]
    FIG --> PAN2["Panel 2<br/>after ;"]

    PAN1 --> L1["Layer<br/>type and file: line a.csv<br/>a folder (runs/) gives one panel per file"]
    PAN1 --> L2["Layer after +<br/>b.csv, scatter b.csv,<br/>or no file: the same file and x"]

    L1 --> S1["Series from columns<br/>one per y column<br/>y=temp,pressure"]
    L1 --> S2["Series from groups<br/>one per value of a column<br/>by=bed"]

    FIG -.- FO["Figure options<br/>--size= --theme= --font= --dpi= --format= --out=<br/>--style=NAME (a saved look)<br/>--layout=2x2 --sharex --sharey --letters --suptitle=<br/>--widths=2,1 --heights=3,1 (panel sizes)<br/>--legend=top|bottom|right|off (one legend for all;<br/>on top by default when the panels' entries match)"]

    PAN1 -.- PA["Axes and labels<br/>title= xlabel= ylabel= xlim= ylim= logx logy<br/>legend= grid=on|off|x|y ybreak=20:80<br/>second axes: y2label= y2lim= logy2 x2= x2label= y2="]
    PAN1 -.- PM["Marks<br/>hline= vline= span= yspan= note= arrow=<br/>rect= circle= segment= inset=<br/>hline2= vline2= yspan2="]
    PAN1 -.- PK["Per plot type<br/>bins= density stacked grouped cmap= zlim= zlabel=<br/>levels= filled values bw= angle= scale= window="]
    PAN1 -.- PG["Grouped data<br/>horizontal highlight= mean median jitter summary"]
    PAN1 -.- PR["Reading curves<br/>peaks dips onset (=storage picks a curve) area=<br/>cross=curves|50 crosslabel= corr<br/>fitlabel= fitdigits= fitnames= fitstats residuals hist"]

    L1 -.- LD["Data<br/>x= y= z= c= sheet= skiplines= skiprows= every= clip=<br/>where= dropna smooth= baseline= showbaseline norm=<br/>by= groups="]
    L1 -.- LE["Errors and fits<br/>err= xerr= band stat= fit= fitrange= start=<br/>fitband through0 weighted slope integral"]
    L1 -.- LX["Placement<br/>axis=left|right step=pre|mid|post"]

    S1 -.- SO["Look, one value per column<br/>label= color= marker= dash= width= alpha=<br/>markers (marker= with the usual shapes)"]
    S2 -.- SG["Look, one value per group<br/>label= color= marker= dash= lists follow the groups"]
```

**Which way to make several series:**

| You have | Write | Example |
|---|---|---|
| curves side by side as columns of one file | a list of y columns | `line f.csv x=dose y=growth,yield` |
| the same, each with its own options | `+` without a file | `line f.csv x=dose y=growth + y=yield axis=right` |
| other files or other plot types | `+` with a file | `line a.csv y=t + scatter b.csv y=t` |
| groups as rows, named in one column | `by=` | `scatter f.csv x=width y=yield by=bed` |

## Name clashes found while drawing the map

How each pair of look-alike words was settled.

| Words | Problem | Settled as |
|---|---|---|
| `style=dashed` vs `--style=thesis` | same word for a line's dash and a saved look | `dash=` for lines; `style=` still accepted |
| `markers` vs `marker=o` | `markers` is only a short form of "default markers" | keep both; written together they are refused |
| `step` type vs `step=` | fine in practice (the option only applies to the type) | leave |
| `smooth=5` vs `smooth(col,9)`, `norm(col)` vs `norm=max` | two spellings of one idea | fine: the option is "all columns", the function "this one" (`norm(col)` is 0–1, like `norm=range`) |
| `residuals` (strip) vs `resid` (plot type) | two ways to show residuals | `residuals` kept as the quick strip under a plot; `resid`/`qq` are plot types that combine with `;` and `+` |
| `grid=` on a panel vs `--grid=2x2` | one is lines, the other the panel layout | `--grid=` → `--layout=`; `--grid=` is an unknown option |
| `cross` vs `cross=50` | two features (curves crossing each other, a curve crossing a level) under one word | `cross=curves` and `cross=50`; bare `cross` refused with that message |
| `stats` vs `stat=mean` | look alike, unrelated (numbers written vs one value per group) | `stats` → `summary` |
| `legend=` vs `--legend=` | both place a legend; together the panel's was silently lost | refused together |
| `skip=` vs `skiprows=` | looked alike: lines above the header vs data rows below it | `skip=` renamed `skiplines=` |
| `errorbar` vs `scatter … err=` | two names for the same picture | `errorbar` removed |
