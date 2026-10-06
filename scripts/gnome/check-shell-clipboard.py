#!/usr/bin/env python3
"""私有 GNOME Shell 会话：安装生产扩展、原生选区事件、真实 watcher 与焦点检查。"""
import os
from pathlib import Path
import subprocess
import tempfile
import time

REPO = Path(__file__).resolve().parents[2]
if os.environ.get("FLASHCAST_PRIVATE_SHELL_CHECK") != "1":
    subprocess.run(["cargo", "build", "-q", "-p", "flashcast-platform", "--example", "clipboard-capture-check"], cwd=REPO, check=True)
    env = os.environ.copy()
    env["FLASHCAST_PRIVATE_SHELL_CHECK"] = "1"
    subprocess.run(["dbus-run-session", "--", "/usr/bin/python3", str(Path(__file__).resolve())], env=env, check=True)
    raise SystemExit(0)

GTK_CHECK = '''
import gi, subprocess
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GLib
window = Gtk.Window(title='Flashcast private clipboard focus check')
window.set_default_size(420, 160)
window.add(Gtk.Label(label='Private clipboard verification'))
state = {'process': None}
def compositor_focus(method):
    result = subprocess.run(['gdbus', 'call', '--session', '--dest', 'org.gnome.Shell', '--object-path', '/org/flashcast/FocusFixture', '--method', 'org.flashcast.FocusFixture.' + method], capture_output=True, text=True, check=True)
    return result.stdout.strip() == '(true,)'
def start():
    if not compositor_focus('Arm'):
        print('FAIL: fixture window did not receive focus', flush=True)
        Gtk.main_quit()
        return False
    state['process'] = subprocess.Popen(['target/debug/examples/clipboard-capture-check', '--expect-text', 'FLASHCAST_PRIVATE_COPY_FIXTURE'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return False
def copy():
    # Headless 会话没有真实 seat 输入 serial，GTK 无法取得选区；由测试扩展向真实
    # Shell Selection 设置原生内存 source，事件与传输仍由生产扩展处理。
    compositor_focus('Copy')
    return False
def finish():
    process = state['process']
    if process is None:
        Gtk.main_quit()
        return False
    output, error = process.communicate(timeout=8)
    print(output.strip(), flush=True)
    if error: print(error, flush=True)
    preserved = compositor_focus('Finish')
    state['passed'] = process.returncode == 0 and preserved
    print('compositor_focus_preserved=' + str(preserved), flush=True)
    Gtk.main_quit()
    return False
window.show_all()
window.present()
GLib.timeout_add(1500, start)
GLib.timeout_add(2300, copy)
GLib.timeout_add(3600, finish)
Gtk.main()
raise SystemExit(0 if state.get('passed') else 1)
'''

with tempfile.TemporaryDirectory(prefix="flashcast-private-shell-") as temp:
    root = Path(temp)
    env = os.environ.copy()
    env.update({
        "XDG_DATA_HOME": str(root / "data"),
        "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_RUNTIME_DIR": str(root / "runtime"),
        "GSETTINGS_BACKEND": "keyfile",
        "GIO_USE_VFS": "local",
        "LIBGL_ALWAYS_SOFTWARE": "1",
        "XDG_SESSION_TYPE": "wayland",
        "XDG_CURRENT_DESKTOP": "GNOME",
        "WAYLAND_DISPLAY": "wayland-flashcast-check",
        "GDK_BACKEND": "wayland",
    })
    (root / "runtime").mkdir(mode=0o700)
    # 运行同一个安装脚本；设置与文件均局限于本检查目录。
    subprocess.run(["bash", "scripts/gnome/install-clipboard-bridge.sh"], cwd=REPO, env=env, check=True, timeout=15)
    # 无输入设备的 headless compositor 需要测试辅助扩展给夹具窗口一次初始焦点。
    # 之后不再激活任何窗口，让读取夺焦点能被检测出来。
    focus_extension = root / "data/gnome-shell/extensions/flashcast-focus-check@fixture"
    focus_extension.mkdir(parents=True)
    (focus_extension / "metadata.json").write_text('{"uuid":"flashcast-focus-check@fixture","name":"Private focus fixture","description":"Private test only","shell-version":["50"]}')
    (focus_extension / "extension.js").write_text('''
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
export default class FocusFixture extends Extension {
    enable() {
        this._object = Gio.DBusExportedObject.wrapJSObject('<node><interface name="org.flashcast.FocusFixture"><method name="Arm"><arg type="b" direction="out"/></method><method name="Finish"><arg type="b" direction="out"/></method><method name="Copy"><arg type="b" direction="out"/></method></interface></node>', this);
        this._object.export(Gio.DBus.session, '/org/flashcast/FocusFixture');
        this._focus = global.display.connect('notify::focus-window', () => {
            if (this._armed && !this._focused()) this._lost = true;
        });
        this._signal = global.display.connect('window-created', (_display, window) => {
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
                if (window.get_title() !== 'Flashcast private clipboard focus check') return GLib.SOURCE_REMOVE;
                Main.overview.hide();
                GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
                    window.activate(global.get_current_time());
                    return GLib.SOURCE_REMOVE;
                });
                return GLib.SOURCE_REMOVE;
            });
        });
    }
    _focused() { return global.display.focus_window?.get_title() === 'Flashcast private clipboard focus check'; }
    Arm() { this._armed = true; this._lost = false; return this._focused(); }
    Finish() { this._armed = false; return !this._lost && this._focused(); }
    Copy() {
        const bytes = new TextEncoder().encode('FLASHCAST_PRIVATE_COPY_FIXTURE');
        const source = Meta.SelectionSourceMemory.new('text/plain', new GLib.Bytes(bytes));
        global.display.get_selection().set_owner(1, source);
        return true;
    }
    disable() { global.display.disconnect(this._signal); global.display.disconnect(this._focus); this._object.unexport(); }
}
''')
    subprocess.run(["gsettings", "set", "org.gnome.shell", "enabled-extensions",
                    "['flashcast-clipboard@flashcast.app', 'flashcast-focus-check@fixture']"], env=env, check=True)
    subprocess.run(["gsettings", "set", "org.gnome.shell", "welcome-dialog-last-shown-version", "'50.1'"], env=env, check=True)
    with (root / "shell.log").open("w") as log:
        shell = subprocess.Popen([
            "gnome-shell", "--headless", "--wayland", "--no-x11",
            "--virtual-monitor=1280x720", "--wayland-display=wayland-flashcast-check",
            "--mode=" + env.get("FLASHCAST_SHELL_MODE", "user"),
        ], cwd=REPO, env=env, stdout=log, stderr=log)
        try:
            ready = False
            for _attempt in range(80):
                result = subprocess.run([
                    "gdbus", "call", "--session", "--dest", "org.gnome.Shell.Extensions.FlashcastClipboard",
                    "--object-path", "/org/gnome/Shell/Extensions/FlashcastClipboard",
                    "--method", "org.gnome.Shell.Extensions.FlashcastClipboard.Ping",
                ], env=env, capture_output=True, timeout=3)
                if result.returncode == 0:
                    ready = True
                    break
                if shell.poll() is not None:
                    break
                time.sleep(0.25)
            if not ready:
                raise RuntimeError("production extension did not load in private GNOME Shell")
            print("PASS: production extension loaded in private GNOME Shell", flush=True)
            subprocess.run(["/usr/bin/python3", "-c", GTK_CHECK], cwd=REPO, env=env, check=True, timeout=15)
            print("PASS: real GNOME Shell selection captured and compositor focus preserved", flush=True)
        except Exception:
            print((root / "shell.log").read_text()[-5000:])
            raise
        finally:
            shell.terminate()
            try:
                shell.wait(timeout=5)
            except subprocess.TimeoutExpired:
                shell.kill()
                shell.wait()
