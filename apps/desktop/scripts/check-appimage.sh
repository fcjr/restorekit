#!/usr/bin/env bash
# Check archive permissions explicitly: extraction changes ownership and can
# otherwise hide an AppRun.wrapped that only the packaging user can execute.
# Then check GUI startup on a virtual display, without any connected hardware.
set -euo pipefail

if [[ $# != 1 ]]; then
  echo "Usage: $0 path/to/RestoreKit.AppImage" >&2
  exit 1
fi
appimage="$(realpath "$1")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"
"$appimage" --appimage-extract > /dev/null
appdir="$work/squashfs-root"

for entry in AppRun AppRun.wrapped usr/bin/restorekit-desktop usr/bin/helper; do
  mode="$(stat -Lc '%a' "$appdir/$entry")"
  if (( (8#$mode & 5) != 5 )); then
    echo "AppImage launcher is not readable/executable by ordinary users: $entry ($mode)" >&2
    exit 1
  fi
done
echo "AppImage launcher permissions passed"

mkdir "$work/home"
if ! xvfb-run -a dbus-run-session -- bash -c '
  export HOME="$2" XDG_CONFIG_HOME="$2/config" XDG_CACHE_HOME="$2/cache"
  # Xvfb is only a display server. Provide a window manager as on a real
  # desktop, so GTK receives the window-management events it expects.
  openbox > "$4" 2>&1 &
  wm_pid=$!
  pid=""
  trap '\''kill ${pid:+"$pid"} "$wm_pid" 2>/dev/null || true; wait 2>/dev/null || true'\'' EXIT
  timeout 10s bash -c '\''until xprop -root _NET_SUPPORTING_WM_CHECK | grep -q "window id"; do sleep 0.1; done'\'' || exit 1
  "$1/AppRun" > "$3" 2>&1 &
  pid=$!
  if ! timeout 30s xdotool search --sync --onlyvisible --pid "$pid" > /dev/null; then
    echo "No visible window for AppRun process $pid" >&2
    ps -o pid,ppid,stat,args -p "$pid" >&2 || true
    xwininfo -root -tree >&2 || true
    exit 1
  fi
  sleep 2
  kill -0 "$pid"
' bash "$appdir" "$work/home" "$work/startup.log" "$work/window-manager.log"; then
  cat "$work/startup.log" "$work/window-manager.log" >&2
  exit 1
fi
echo "AppImage permissions and GUI startup passed"
