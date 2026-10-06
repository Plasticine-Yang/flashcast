import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export const BUS_NAME = 'org.gnome.Shell.Extensions.FlashcastClipboard';
export const OBJECT_PATH = '/org/gnome/Shell/Extensions/FlashcastClipboard';
export const INTERFACE_XML = `<node><interface name="${BUS_NAME}">
  <method name="Ping"><arg type="u" direction="out" name="version"/></method>
  <method name="Poll">
    <arg type="s" direction="in" name="epoch"/><arg type="t" direction="in" name="after"/>
    <arg type="s" direction="out" name="epoch"/><arg type="t" direction="out" name="sequence"/>
    <arg type="a{say}" direction="out" name="payloads"/>
    <arg type="s" direction="out" name="appId"/><arg type="s" direction="out" name="appName"/>
    <arg type="s" direction="out" name="problem"/>
  </method>
  <method name="Stop"/>
</interface></node>`;

const CLIPBOARD = 1; // Meta.SelectionType.SELECTION_CLIPBOARD
const TEXT_LIMIT = 4 * 1024 * 1024;
const IMAGE_LIMIT = 16 * 1024 * 1024;
const QUEUE_LIMIT = 32 * 1024 * 1024;
const LEASE_USEC = 2 * 1000 * 1000;
const MIME_GROUPS = [
    ['text/plain;charset=utf-8', 'text/plain', 'UTF8_STRING'],
    ['text/html'], ['text/rtf', 'application/rtf'], ['image/png'],
    ['text/uri-list', 'x-special/gnome-copied-files'],
];

// 内存队列只保留已请求的复制事件。停止、租约过期或锁屏时立即清空。
export class ClipboardBridge {
    constructor(selection, sourceApp, canCapture = () => true) {
        this._selection = selection;
        this._sourceApp = sourceApp;
        this._canCapture = canCapture;
        this._epoch = GLib.uuid_string_random();
        this._sequence = 0;
        this._generation = 0;
        this._queue = [];
        this._pending = new Set();
        this._signal = 0;
        this._timer = 0;
    }

    Ping() { return 1; }

    Poll(epoch, after) {
        if (!this._canCapture()) {
            this.Stop();
            return [this._epoch, this._sequence, {}, '', '', ''];
        }
        this._deadline = GLib.get_monotonic_time() + LEASE_USEC;
        if (!this._signal) {
            this._signal = this._selection.connect('owner-changed', (_selection, type, owner) => {
                if (type === CLIPBOARD && owner)
                    this._capture();
            });
            this._timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
                if (!this._canCapture() || GLib.get_monotonic_time() >= this._deadline) {
                    this._timer = 0;
                    this.Stop();
                    return GLib.SOURCE_REMOVE;
                }
                return GLib.SOURCE_CONTINUE;
            });
        }
        const cursor = epoch === this._epoch ? after : 0;
        // 一个宿主消费队列；按序确认后及时释放载荷。
        this._queue = this._queue.filter(event => event[1] > cursor);
        return this._queue[0] ?? [this._epoch, this._sequence, {}, '', '', ''];
    }

    Stop() {
        this._generation++;
        if (this._signal)
            this._selection.disconnect(this._signal);
        this._signal = 0;
        if (this._timer)
            GLib.source_remove(this._timer);
        this._timer = 0;
        for (const cancellable of this._pending)
            cancellable.cancel();
        this._pending.clear();
        this._queue = [];
    }

    async _capture() {
        if (!this._canCapture() || GLib.get_monotonic_time() >= this._deadline) {
            this.Stop();
            return;
        }
        // 新 owner 到来后，取消旧 owner 的未完成读取，避免混合两个复制事件。
        for (const cancellable of this._pending)
            cancellable.cancel();
        const generation = ++this._generation;
        const cancellable = new Gio.Cancellable();
        this._pending.add(cancellable);
        const source = this._sourceApp();
        const types = this._selection.get_mimetypes(CLIPBOARD) ?? [];
        const payloads = {};
        const problems = [];
        let timedOut = false;
        const timeout = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1200, () => {
            timedOut = true;
            cancellable.cancel();
            return GLib.SOURCE_REMOVE;
        });
        try {
            await Promise.all(MIME_GROUPS.map(async group => {
                const mime = group.find(type => types.includes(type));
                if (!mime)
                    return;
                const limit = mime === 'image/png' ? IMAGE_LIMIT : TEXT_LIMIT;
                try {
                    const bytes = await this._read(mime, limit + 1, cancellable);
                    if (bytes.length > limit)
                        problems.push(`${mime} 超过 ${limit / (1024 * 1024)} MiB，未保存。`);
                    else if (bytes.length)
                        payloads[mime] = bytes;
                } catch (_error) {
                    problems.push(`${mime} 读取失败或超时。`);
                }
            }));
            if (generation !== this._generation || !this._signal || !this._canCapture())
                return;
            this._queue.push([this._epoch, ++this._sequence, payloads,
                source?.id ?? '', source?.name ?? '', problems.join(' ')]);
            let size = this._queue.reduce((total, event) => total +
                Object.values(event[2]).reduce((n, bytes) => n + bytes.length, 0), 0);
            while (this._queue.length > 32 || size > QUEUE_LIMIT) {
                const dropped = this._queue.shift();
                size -= Object.values(dropped[2]).reduce((n, bytes) => n + bytes.length, 0);
            }
        } finally {
            this._pending.delete(cancellable);
            // 超时 source 已自行移除时不能再移除同一个 ID。
            if (!timedOut)
                GLib.source_remove(timeout);
        }
    }

    _read(mime, limit, cancellable) {
        const stream = Gio.MemoryOutputStream.new_resizable();
        return new Promise((resolve, reject) => {
            this._selection.transfer_async(CLIPBOARD, mime, limit, stream, cancellable,
                (selection, result) => {
                    try {
                        selection.transfer_finish(result);
                        stream.close(null);
                        resolve(stream.steal_as_bytes().toArray());
                    } catch (error) {
                        stream.close(null);
                        reject(error);
                    }
                });
        });
    }
}

export function exportBridge(bridge) {
    const object = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, bridge);
    object.export(Gio.DBus.session, OBJECT_PATH);
    const owner = Gio.bus_own_name_on_connection(Gio.DBus.session, BUS_NAME,
        Gio.BusNameOwnerFlags.NONE, null, null);
    return () => {
        bridge.Stop();
        object.unexport();
        Gio.bus_unown_name(owner);
    };
}
