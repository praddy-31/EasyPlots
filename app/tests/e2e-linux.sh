#!/bin/bash
# End-to-end check of the real app on Linux (for cloud sessions without a Mac): starts the
# debug app in a virtual screen with a throwaway home folder, presses the hotkey, picks a
# file, type and columns, previews, saves, and makes a typo. The launcher should be in
# Gruvbox colours (from the stand-in Palette file). Then: a click elsewhere keeps the draft,
# and a hotkey edited in config.toml works at once. Screenshots and the saved plot land in OUT.
# (Opening on the display with the mouse can't be checked here: Xvfb has one monitor.)
#
#   apt-get install xvfb xdotool openbox imagemagick
#   npm --prefix app run tauri build -- --debug --no-bundle
#   xvfb-run -a -s "-screen 0 1280x800x24" app/tests/e2e-linux.sh /tmp/easyplots-e2e
#
# A window manager (openbox) is needed: without one the launcher can't take focus and
# hides itself straight away.
set -e
SP=$1
REPO=$(cd "$(dirname "$0")/../.." && pwd)
rm -rf "$SP" && mkdir -p "$SP/home/Documents/lab"
cp "$REPO/tests/fixtures/simple.csv" "$SP/home/Documents/lab/"
cp "$REPO"/tests/fixtures/{run_a,run_b,dates}.csv "$SP/home/Documents/"
export HOME=$SP/home
# A Palette colour file (Gruvbox), like the one Palette writes for the Habit Tracker, to check
# that the launcher follows Palette.
mkdir -p "$HOME/.config/palette/current"
cat > "$HOME/.config/palette/current/habit-tracker.json" <<'JSON'
{"palette":1,"theme":"gruvbox-dark","mode":"dark","background":"#282828","foreground":"#d4be98","accent":"#7daea3","selection":"#504945","muted":"#665c54","dark_foreground":"#7c6f64","light_foreground":"#bdae93","red":"#ea6962","orange":"#e1875c","yellow":"#d8a657","green":"#a9b665","cyan":"#89b482","blue":"#7daea3","magenta":"#d3869b"}
JSON
openbox >/dev/null 2>&1 &
sleep 1
dbus-run-session -- "$REPO/target/debug/easyplots-app" > $SP/e2e.log 2>&1 &
sleep 8
shot() { import -window root "$SP/e2e_$1.png"; }
xdotool key ctrl+alt+p; sleep 1.5; shot 1_open
xdotool type --delay 60 "smp"; sleep 0.8
xdotool key Return; sleep 1.5; shot 2_type
xdotool key Return; sleep 0.8
xdotool type --delay 60 "tim"; sleep 0.5
xdotool key Return; sleep 0.8
xdotool key Tab; sleep 0.3; xdotool key Down Tab; sleep 0.5; shot 2_y
xdotool key Return; sleep 0.8; shot 3_command
xdotool type --delay 40 'title="Bench run"'; sleep 0.6
xdotool key Return; sleep 4; shot 4_preview
xdotool key Return; sleep 4; shot 5_saved
xdotool key Escape; sleep 0.8
xdotool key ctrl+alt+p; sleep 1.2
xdotool type --delay 40 "line lab/simple.csv x=tmie"; sleep 0.6
xdotool key Return; sleep 1.2; shot 6_error
# A click elsewhere hides the launcher but keeps the draft.
xdotool mousemove 20 780 click 1; sleep 1
xdotool key ctrl+alt+p; sleep 1.5; shot 8_draft_kept
xdotool key Escape; sleep 0.8
# A new hotkey in config.toml works within a second, without opening the launcher first.
sed -i 's/hotkey       = "Ctrl+Option+P"/hotkey       = "Ctrl+Option+O"/' "$HOME/.config/easyplots/config.toml"
sleep 2.5
xdotool key ctrl+alt+o; sleep 1.5; shot 7_new_hotkey
xdotool key Escape; sleep 0.5
kill %2 2>/dev/null || true
ls "$HOME/Plots"
