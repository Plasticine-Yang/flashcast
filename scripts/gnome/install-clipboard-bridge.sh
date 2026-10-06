#!/usr/bin/env bash
# 仅安装本仓库的扩展；保留已有 GNOME 扩展设置。不会重启桌面或注销会话。
set -euo pipefail
repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
extension_uuid='flashcast-clipboard@flashcast.app'
extension_dir="$repo_dir/integrations/gnome/$extension_uuid"
for program in gnome-extensions gjs; do
  command -v "$program" >/dev/null || { echo "缺少 $program，请先安装 GNOME 扩展管理工具。" >&2; exit 1; }
done
archive_dir="$(mktemp -d -t flashcast-gnome-XXXXXX)"
trap 'rm -rf -- "$archive_dir"' EXIT
gnome-extensions pack --force --extra-source=bridge.js --out-dir="$archive_dir" "$extension_dir"
gnome-extensions install --force "$archive_dir/$extension_uuid.shell-extension.zip"
gjs -c '
const Gio = imports.gi.Gio;
const settings = new Gio.Settings({schema_id: "org.gnome.shell"});
const uuid = "flashcast-clipboard@flashcast.app";
const enabled = settings.get_strv("enabled-extensions");
if (!enabled.includes(uuid)) {
    if (!settings.set_strv("enabled-extensions", [...enabled, uuid]))
        throw new Error("系统不允许启用扩展，请在扩展管理器中手动启用。");
}
const disabled = settings.get_strv("disabled-extensions");
if (disabled.includes(uuid) && !settings.set_strv("disabled-extensions", disabled.filter(id => id !== uuid)))
    throw new Error("系统不允许启用扩展，请在扩展管理器中手动启用。");
Gio.Settings.sync();
'
if gnome-extensions info "$extension_uuid" >/dev/null 2>&1; then
  gnome-extensions enable "$extension_uuid"
  echo '扩展已安装并请求启用。GNOME 会缓存扩展代码；安装或更新后请重新登录，再启动 Flashcast 检查记录。'
else
  echo '扩展已安装并设置为启用。首次安装需要退出登录再登录，随后启动 Flashcast。'
fi
