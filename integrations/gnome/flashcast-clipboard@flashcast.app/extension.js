import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {ClipboardBridge, exportBridge} from './bridge.js';

export default class FlashcastClipboardExtension extends Extension {
    enable() {
        this._bridge = new ClipboardBridge(global.display.get_selection(), () => {
            const app = Shell.WindowTracker.get_default().focus_app;
            return app ? {id: app.get_id(), name: app.get_name()} : null;
        }, () => !Main.sessionMode.isLocked && (Main.sessionMode.currentMode === 'user' ||
            Main.sessionMode.parentMode === 'user'));
        this._unexport = exportBridge(this._bridge);
        this._sessionSignal = Main.sessionMode.connect('updated', () => this._bridge.Stop());
    }

    disable() {
        if (this._sessionSignal)
            Main.sessionMode.disconnect(this._sessionSignal);
        this._sessionSignal = 0;
        this._unexport?.();
        this._unexport = null;
        this._bridge = null;
    }
}
