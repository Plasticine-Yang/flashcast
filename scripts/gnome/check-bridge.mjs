// 在私有 D-Bus 中检查生产桥接代码；所有输入均为本检查创建的内容。
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import {ClipboardBridge, exportBridge} from '../../integrations/gnome/flashcast-clipboard@flashcast.app/bridge.js';

const loop = new GLib.MainLoop(null, false);
const bytes = text => new TextEncoder().encode(text);
const sleep = ms => new Promise(resolve => GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
    resolve();
    return GLib.SOURCE_REMOVE;
}));
function assert(ok, message) { if (!ok) throw new Error(message); }
function setOwner(selection, mime, value) {
    selection.set_owner(1, Meta.SelectionSourceMemory.new(mime, new GLib.Bytes(value)));
}

async function check() {
    const selection = new Meta.Selection();
    let allowed = true;
    const bridge = new ClipboardBridge(selection, () => ({id: 'fixture.desktop', name: 'Fixture'}), () => allowed);
    let [epoch, cursor] = bridge.Poll('', 0);
    setOwner(selection, 'text/plain', bytes('检查文字'));
    await sleep(50);
    let event = bridge.Poll(epoch, cursor);
    assert(new TextDecoder().decode(event[2]['text/plain']) === '检查文字', 'native text transfer');
    assert(event[3] === 'fixture.desktop' && event[4] === 'Fixture', 'source app');
    cursor = event[1];
    assert(Object.keys(bridge.Poll(epoch, cursor)[2]).length === 0, 'ack releases payload');
    for (const mime of ['text/html', 'text/rtf', 'image/png', 'text/uri-list', 'x-special/gnome-copied-files']) {
        setOwner(selection, mime, bytes(`fixture-${mime}`));
        await sleep(30);
        event = bridge.Poll(epoch, cursor);
        assert(new TextDecoder().decode(event[2][mime]) === `fixture-${mime}`, `native ${mime} transfer`);
        cursor = event[1];
    }
    bridge.Stop();
    setOwner(selection, 'text/plain', bytes('暂停时不录制'));
    await sleep(30);
    assert(Object.keys(bridge.Poll(epoch, cursor)[2]).length === 0, 'stop skips paused content');
    setOwner(selection, 'text/plain', bytes('恢复后复制'));
    await sleep(30);
    event = bridge.Poll(epoch, cursor);
    assert(new TextDecoder().decode(event[2]['text/plain']) === '恢复后复制', 'resume captures new events');
    cursor = event[1];
    allowed = false;
    setOwner(selection, 'text/plain', bytes('锁屏不录制'));
    await sleep(30);
    assert(Object.keys(bridge.Poll(epoch, cursor)[2]).length === 0, 'locked session');
    allowed = true;
    bridge.Poll(epoch, cursor);
    setOwner(selection, 'text/plain', new Uint8Array(4 * 1024 * 1024 + 2));
    await sleep(100);
    event = bridge.Poll(epoch, cursor);
    assert(!event[2]['text/plain'] && event[5].includes('超过 4 MiB'), 'bounded payload');
    cursor = event[1];
    setOwner(selection, 'text/plain', bytes('被取消的旧复制'));
    setOwner(selection, 'text/plain', bytes('最新复制'));
    await sleep(50);
    event = bridge.Poll(epoch, cursor);
    assert(new TextDecoder().decode(event[2]['text/plain']) === '最新复制', 'cancel obsolete owner');
    cursor = event[1];
    bridge.Poll(epoch, cursor);
    await sleep(2300);
    setOwner(selection, 'text/plain', bytes('租约过期不录制'));
    await sleep(30);
    assert(Object.keys(bridge.Poll(epoch, cursor)[2]).length === 0, 'lease expiry');
    bridge.Stop();
    print('PASS: native Mutter transfers, formats, source, stop/resume, lock, limits, cancellation, lease');
}

// 多种格式由各自的真实 Mutter Selection 提供；生产 bridge 仍执行原生异步传输。
class FixtureSelection {
    constructor() { this.master = new Meta.Selection(); this.sources = new Map(); }
    connect(...args) { return this.master.connect(...args); }
    disconnect(...args) { this.master.disconnect(...args); }
    get_mimetypes() { return [...this.sources.keys()]; }
    transfer_async(type, mime, ...args) { this.sources.get(mime).transfer_async(type, mime, ...args); }
    copy(mode) {
        let payloads;
        if (mode === 'rich') {
            payloads = {'text/plain': bytes('桥接富文本'), 'text/html': bytes('<b>桥接富文本</b>'),
                'text/rtf': bytes('{\\rtf1 bridge}')};
        } else if (mode === 'image') {
            payloads = {'image/png': GLib.base64_decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aRfoAAAAASUVORK5CYII=')};
        } else if (mode === 'files') {
            payloads = {'text/uri-list': bytes('file:///tmp/flashcast-fixture.txt\r\n')};
        } else {
            payloads = {'text/plain': bytes(mode)};
        }
        this.sources.clear();
        for (const [mime, value] of Object.entries(payloads)) {
            const selection = new Meta.Selection();
            setOwner(selection, mime, value);
            this.sources.set(mime, selection);
        }
        setOwner(this.master, 'text/plain', bytes('owner-event'));
    }
}

async function serve() {
    const selection = new FixtureSelection();
    const bridge = new ClipboardBridge(selection, () => ({id: 'fixture.desktop', name: 'Fixture'}));
    const unexport = exportBridge(bridge);
    const fixture = Gio.DBusExportedObject.wrapJSObject(`<node><interface name="org.flashcast.ClipboardFixture">
      <method name="Copy"><arg type="s" direction="in" name="mode"/></method>
      <method name="Quit"/>
    </interface></node>`, {
        Copy(mode) { selection.copy(mode); },
        Quit() { unexport(); loop.quit(); },
    });
    fixture.export(Gio.DBus.session, '/org/flashcast/ClipboardFixture');
    await sleep(100);
    print('READY: private native clipboard fixture');
}

let failed = false;
const action = ARGV.includes('--serve') ? serve : check;
action().then(() => { if (action === check) loop.quit(); }).catch(error => {
    printerr(error.stack);
    failed = true;
    loop.quit();
});
loop.run();
if (failed) imports.system.exit(1);
