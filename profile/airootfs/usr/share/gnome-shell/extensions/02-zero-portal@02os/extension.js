import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {DURATION_SECONDS, drawPortalFrame} from './motion.js';

const MARKER = '/run/02os-zero-portal/played';
const RESULT = '/run/02os-zero-portal/result.json';
const MAX_COVER_MILLISECONDS = 4200;
const INPUT_EVENTS = new Set([
    Clutter.EventType.KEY_PRESS,
    Clutter.EventType.BUTTON_PRESS,
    Clutter.EventType.TOUCH_BEGIN,
    Clutter.EventType.SCROLL,
]);

export default class ZeroPortalExtension extends Extension {
    enable() {
        this._areas = [];
        this._connections = [];
        this._tickId = 0;
        this._deadlineId = 0;
        this._errorIdleId = 0;
        this._startedAt = 0;
        this._phase = '';
        this._interfaceSettings = null;
        this._diagnosticEvents = [];
        this._diagnosticWriteWarned = false;

        if (Main.sessionMode.currentMode !== 'gdm')
            return;
        // O_EXCL creation is atomic across greeter processes. /run is cleared
        // at reboot; logging out or restarting GDM can never replay the intro.
        try {
            const stream = Gio.File.new_for_path(MARKER).create(
                Gio.FileCreateFlags.NONE, null);
            stream.close(null);
        } catch (error) {
            if (error.matches?.(Gio.IOErrorEnum, Gio.IOErrorEnum.EXISTS)) {
                console.info('02 Zero Portal: skipped (already played this boot).');
                this._restoreDiagnosticEvents();
                this._record('skipped', 'already played this boot');
            } else {
                console.warn(`02 Zero Portal skipped: ${error.message}`);
                this._record('skipped', `marker unavailable: ${error.message}`);
            }
            return;
        }
        this._record('claimed', 'first GDM load this boot');
        this._interfaceSettings = new Gio.Settings({
            schema_id: 'org.gnome.desktop.interface',
        });

        // GNOME loads extensions asynchronously, often just after its startup
        // signal. That first autoload still belongs to this boot: begin the
        // reveal immediately when login is ready, otherwise wait for it. Input
        // always removes this nonreactive overlay and continues to normal GDM.
        // St.Settings also automatically inhibits animations on software
        // renderers. This bounded Cairo effect honours the user's reduced
        // motion preference directly and can run in a software-rendered VM.
        if (!this._interfaceSettings.get_boolean('enable-animations')) {
            console.info('02 Zero Portal: skipped (reduced motion).');
            this._record('skipped', 'reduced motion');
            return;
        }

        try {
            this._connect(global.stage, 'captured-event', (_stage, event) => {
                if (INPUT_EVENTS.has(event.type()))
                    this._finish('input');
                return Clutter.EVENT_PROPAGATE;
            });
            this._connect(Main.sessionMode, 'updated', () => {
                if (Main.sessionMode.currentMode !== 'gdm')
                    this._finish('session changed');
            });
            this._connect(this._interfaceSettings, 'changed::enable-animations', () => {
                if (!this._interfaceSettings.get_boolean('enable-animations'))
                    this._finish('reduced motion');
            });
            // Resizing or unplugging a screen immediately reveals login rather
            // than leaving a partial full-screen mask on another monitor.
            this._connect(Main.layoutManager, 'monitors-changed', () => this._finish('monitor changed'));
            this._connect(Main.layoutManager, 'startup-complete', () => this._beginMotion());
            for (const monitor of Main.layoutManager.monitors) {
                const area = new St.DrawingArea({
                    name: '02-zero-portal',
                    reactive: false,
                    can_focus: false,
                    x: monitor.x,
                    y: monitor.y,
                    width: monitor.width,
                    height: monitor.height,
                });
                area.connect('repaint', () => this._repaint(area));
                this._areas.push(area);
                Main.layoutManager.addTopChrome(area);
                area.queue_repaint();
            }
            this._deadlineId = GLib.timeout_add(GLib.PRIORITY_DEFAULT,
                MAX_COVER_MILLISECONDS, () => {
                    this._deadlineId = 0;
                    this._finish('deadline');
                    return GLib.SOURCE_REMOVE;
                });
            console.info('02 Zero Portal: retained logo ready; waiting for real login.');
            this._record('overlay ready', Main.layoutManager._startingUp ?
                'waiting for login startup' : 'login startup already complete');
            if (!Main.layoutManager._startingUp)
                this._beginMotion();
        } catch (error) {
            console.warn(`02 Zero Portal stopped: ${error.message}`);
            this._record('error', error.message);
            this._finish('error');
        }
    }

    disable() {
        this._finish('disabled');
    }

    _connect(object, signal, callback) {
        this._connections.push([object, object.connect(signal, callback)]);
    }

    _restoreDiagnosticEvents() {
        try {
            const [, contents] = Gio.File.new_for_path(RESULT).load_contents(null);
            const record = JSON.parse(new TextDecoder().decode(contents));
            if (record.version === 1 && Array.isArray(record.events))
                this._diagnosticEvents = record.events.slice(-15);
        } catch {
            // Missing diagnostics never change the once-per-boot marker rule.
        }
    }

    _record(phase, reason = '') {
        try {
            this._diagnosticEvents.push({
                phase,
                reason,
                monotonic_us: GLib.get_monotonic_time(),
                animation_elapsed_seconds: this._elapsed(),
            });
            this._diagnosticEvents = this._diagnosticEvents.slice(-16);
            const contents = new TextEncoder().encode(JSON.stringify({
                version: 1,
                phase,
                reason,
                events: this._diagnosticEvents,
            }, null, 2) + '\n');
            Gio.File.new_for_path(RESULT).replace_contents(contents, null, false,
                Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (error) {
            // This file is optional evidence in a boot-scoped tmpfs. A write
            // failure must never prevent login or extend the animation.
            if (!this._diagnosticWriteWarned) {
                this._diagnosticWriteWarned = true;
                console.warn(`02 Zero Portal diagnostics unavailable: ${error.message}`);
            }
        }
    }

    _beginMotion() {
        if (!this._areas.length || this._startedAt)
            return;
        this._startedAt = GLib.get_monotonic_time();
        console.info('02 Zero Portal: begin (real login ready).');
        this._record('begin', 'real login ready');
        this._tickId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 34, () => {
            try {
                const elapsed = this._elapsed();
                if (elapsed >= DURATION_SECONDS) {
                    this._tickId = 0;
                    this._finish('complete');
                    return GLib.SOURCE_REMOVE;
                }
                const phase = elapsed < 0.15 ? 'logo' : elapsed < 0.95 ? '2 behind 0' :
                    elapsed < 1.45 ? '0 expands' : elapsed < 1.6 ? 'portal opens' :
                        'portal reveals real login';
                if (phase !== this._phase) {
                    this._phase = phase;
                    console.info(`02 Zero Portal: ${phase}.`);
                    this._record(phase);
                }
                for (const area of this._areas)
                    area.queue_repaint();
                return GLib.SOURCE_CONTINUE;
            } catch (error) {
                console.warn(`02 Zero Portal stopped: ${error.message}`);
                this._record('error', error.message);
                this._tickId = 0;
                this._finish('error');
                return GLib.SOURCE_REMOVE;
            }
        });
    }

    _elapsed() {
        return this._startedAt ?
            (GLib.get_monotonic_time() - this._startedAt) / 1_000_000 : 0;
    }

    _repaint(area) {
        let cr;
        try {
            cr = area.get_context();
            const [width, height] = area.get_surface_size();
            drawPortalFrame(cr, width, height, this._elapsed());
        } catch (error) {
            console.warn(`02 Zero Portal stopped: ${error.message}`);
            this._record('paint error', error.message);
            // Destroying a Clutter actor inside its repaint is unsafe. Release
            // Cairo now and remove the actor at the next main-loop iteration.
            if (!this._errorIdleId) {
                this._errorIdleId = GLib.idle_add(GLib.PRIORITY_DEFAULT, () => {
                    this._errorIdleId = 0;
                    this._finish('paint error');
                    return GLib.SOURCE_REMOVE;
                });
            }
        } finally {
            cr?.$dispose();
        }
    }

    _finish(reason) {
        if (this._areas?.length) {
            console.info(`02 Zero Portal: ${reason}; overlay removed at ${this._elapsed().toFixed(3)}s.`);
            this._record('removed', reason);
        }
        for (const key of ['_tickId', '_deadlineId', '_errorIdleId']) {
            if (this[key]) {
                GLib.source_remove(this[key]);
                this[key] = 0;
            }
        }
        for (const [object, id] of this._connections ?? [])
            object.disconnect(id);
        this._connections = [];
        for (const area of this._areas ?? []) {
            Main.layoutManager.removeChrome(area);
            area.destroy();
        }
        this._areas = [];
        this._startedAt = 0;
        this._interfaceSettings = null;
    }
}
