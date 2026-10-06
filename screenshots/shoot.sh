#!/bin/sh
# Regenerates the README screenshots from the window's own markup and CSS,
# rendered at chosen moments of Sway's "This Is My Demo" by the
# render_screenshots test. The lyrics come from LRCLIB.
set -e
cd "$(dirname "$0")/.."
mkdir -p target/screenshots
rm -f target/screenshots/*.html
curl -sf https://lrclib.net/api/get/8887501 | jq -r .syncedLyrics > target/screenshots/lyrics.lrc
SEESONGS_LRC=target/screenshots/lyrics.lrc cargo test --quiet --bin seesongs -- --ignored render_screenshots
profile=$(mktemp -d)
trap 'rm -rf "$profile"' EXIT
for html in target/screenshots/*.html; do
    name=$(basename "$html" .html)
    size=${name##*-}
    google-chrome --headless=new --user-data-dir="$profile" --disable-gpu --hide-scrollbars \
        --force-device-scale-factor=2 --window-size="${size%x*},${size#*x}" \
        --screenshot="screenshots/${name%-*}.png" "file://$PWD/$html" 2>/dev/null
done
