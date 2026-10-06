#!/usr/bin/env bash
set -euo pipefail
repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_dir"
# Ubuntu/Debian 的 Mutter typelib 带版本子目录。也可由调用者显式指定。
if [[ -z "${FLASHCAST_MUTTER_DIR:-}" ]]; then
  for candidate in /usr/lib/x86_64-linux-gnu/mutter-* /usr/lib64/mutter-*; do
    if compgen -G "$candidate/Meta-*.typelib" >/dev/null; then
      export FLASHCAST_MUTTER_DIR="$candidate"
    fi
  done
fi
if [[ -z "${FLASHCAST_MUTTER_DIR:-}" ]]; then
  echo '未找到 Mutter typelib，请设置 FLASHCAST_MUTTER_DIR。' >&2
  exit 1
fi
export GI_TYPELIB_PATH="$FLASHCAST_MUTTER_DIR${GI_TYPELIB_PATH:+:$GI_TYPELIB_PATH}"
export LD_LIBRARY_PATH="$FLASHCAST_MUTTER_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
if [[ "${1:-}" != '--private-session' ]]; then
  exec dbus-run-session -- "$0" --private-session
fi
export XDG_CURRENT_DESKTOP=GNOME
export XDG_SESSION_TYPE=wayland
gjs -m scripts/gnome/check-bridge.mjs
gjs -m scripts/gnome/check-bridge.mjs --serve &
fixture_pid=$!
trap 'kill "$fixture_pid" 2>/dev/null || true; wait "$fixture_pid" 2>/dev/null || true' EXIT
for _attempt in {1..30}; do
  if gdbus call --session --dest org.gnome.Shell.Extensions.FlashcastClipboard \
    --object-path /org/gnome/Shell/Extensions/FlashcastClipboard \
    --method org.gnome.Shell.Extensions.FlashcastClipboard.Ping >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
FLASHCAST_GNOME_CLIPBOARD_CHECK=1 cargo test -p flashcast-core --test gnome_clipboard -- --ignored --nocapture
