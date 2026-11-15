#!/bin/bash
# Type-check and lint the macOS-only app code (the launcher panel, Dock icon switch…) from
# Linux, without a Mac or Apple's SDK. Nothing is linked, so a stand-in C compiler that
# writes empty files is enough for the few crates that compile Objective-C; DOCS_RS=1 makes
# the notification crate skip its own. Catches type errors and clippy warnings before a
# [build-mac] push; it can't tell whether the code works on a Mac.
#
#   rustup target add aarch64-apple-darwin      (once)
#   app/tests/check-macos.sh
set -e
REPO=$(cd "$(dirname "$0")/../.." && pwd)
FAKE=$(mktemp -d)
trap 'rm -rf "$FAKE"' EXIT
cat > "$FAKE/cc" <<'SH'
#!/bin/sh
out=""
while [ $# -gt 0 ]; do
  case "$1" in -o) out="$2"; shift;; esac
  shift
done
[ -n "$out" ] && : > "$out"
exit 0
SH
chmod +x "$FAKE/cc"
cd "$REPO"
DOCS_RS=1 CC_aarch64_apple_darwin="$FAKE/cc" AR_aarch64_apple_darwin=ar \
  cargo clippy -p easyplots-app --target aarch64-apple-darwin --all-targets -- -D warnings
