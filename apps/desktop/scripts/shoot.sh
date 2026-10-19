#!/usr/bin/env bash
# Take a screenshot of the desktop app on the real Wayland session.
#
# usage: shoot.sh <out.png> [app args...]
#
# Why not Xwayland or Xvfb: the app draws with Vulkan, and a nested X server
# gives black frames. Run it natively, then capture the active window with
# spectacle (KDE). Another window can take the focus while the app starts. The
# script then captures that window, so it checks the shape of the image and
# fails if it is not the app. Run it again when it fails, and keep other
# windows still while it runs.
#
# The app path comes from APP (default: the debug build). SHOOT_WAIT is the
# start-up wait in seconds. Real data must never go in the repository. Save
# real-data images outside it.
set -u
out="$1"
shift
app="${APP:-${CARGO_TARGET_DIR:-target}/debug/garmin-companion}"
size="1280x900"
prev=""
for arg in "$@"; do
    [[ "$prev" == "--size" ]] && size="$arg"
    prev="$arg"
done

"$app" "$@" >/dev/null 2>&1 &
pid=$!
sleep "${SHOOT_WAIT:-5}"
spectacle -b -n -a -o "$out" >/dev/null 2>&1
kill "$pid" 2>/dev/null

if [[ ! -s "$out" ]]; then
    echo "spectacle made no image" >&2
    exit 1
fi
# The capture of the app has the shape of the window, within 2 percent.
read -r w h < <(identify -format '%w %h' "$out")
want_w="${size%x*}"
want_h="${size#*x}"
if (( (w * want_h - h * want_w) * 50 > h * want_w || (h * want_w - w * want_h) * 50 > h * want_w )); then
    echo "not the app window: got ${w}x${h}, wanted the shape of ${size}" >&2
    rm -f "$out"
    exit 1
fi
echo "captured $out (${w}x${h})"
