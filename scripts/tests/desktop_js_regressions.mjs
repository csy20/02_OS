import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as Ecs from '../../profile/airootfs/usr/share/gnome-shell/extensions/pop-shell@system76.com/ecs.js';
import * as NodeMod from '../../profile/airootfs/usr/share/gnome-shell/extensions/pop-shell@system76.com/node.js';
import {
    exception_applies,
    literal_rule_field,
} from '../../profile/airootfs/usr/share/gnome-shell/extensions/pop-shell@system76.com/float_pattern.js';
import { WobblyModel } from '../../profile/airootfs/usr/share/gnome-shell/extensions/compiz-windows-effect@hermes83.github.com/src/effects/wobbly_model.js';
import {
    safe_speedup_divider,
    settings_for_effect,
    DEFAULT_SPEEDUP_FACTOR,
} from '../../profile/airootfs/usr/share/gnome-shell/extensions/compiz-windows-effect@hermes83.github.com/settings_data.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const pop = join(root, 'profile/airootfs/usr/share/gnome-shell/extensions/pop-shell@system76.com');

function assertFloatLiterals() {
    const titles = ['Compile (', 'Compile [draft]', 'file.txt', 'a+b', 'C:\\Program Files\\App'];
    const impostors = {
        'Compile (': 'Compile X',
        'Compile [draft]': 'Compile d',
        'file.txt': 'fileXtxt',
        'a+b': 'aaab',
        'C:\\Program Files\\App': 'C:Program FilesApp',
    };
    for (const title of titles) {
        const stored = {
            class: literal_rule_field('org.gnome.Console'),
            title: literal_rule_field(title),
        };
        assert.equal(exception_applies([stored], 'org.gnome.Console', title), true, title);
        assert.equal(exception_applies([stored], 'org.gnome.Console', impostors[title]), false, title);
        assert.equal(exception_applies([stored], 'orgXgnomeXConsole', title), false, title);
    }

    assert.equal(
        exception_applies([{ class: 'org.gnome.Console', title: 'Compile (' }], 'org.gnome.Console', 'Compile ('),
        true,
    );
    assert.equal(
        exception_applies(
            [{ class: 'org.gnome.Console', title: 'Compile (' }, { class: 'OtherApp' }],
            'OtherApp',
            'untouched',
        ),
        true,
    );
    assert.equal(exception_applies([{ class: 'ibus-.*' }], 'ibus-daemon', 'x'), true);
    assert.equal(exception_applies([{ class: 'ibus-.*' }], 'firefox', 'x'), false);
    assert.equal(
        exception_applies([{ class: 'Steam', title: '^.*(Guard|Login).*' }], 'Steam', 'Steam Guard'),
        true,
    );

    for (const name of ['config.js', 'floating_exceptions/config.js']) {
        const source = readFileSync(join(pop, name), 'utf8');
        assert.match(source, /literal_rule_field\(wmclass/);
        assert.match(source, /literal_rule_field\(title\)/);
        assert.match(source, /exception_applies\(/);
        assert.doesNotMatch(source, /new RegExp\(rule\./);
    }
}

function assertEntities() {
    assert.equal(Ecs.entity_eq([1, 0], [1, 99]), false);
    assert.equal(Ecs.entity_eq([1, 99], [1, 99]), true);
    assert.equal(NodeMod.Node.window([1, 99]).is_window([1, 0]), false);
    assert.equal(NodeMod.stack_find({ entities: [[1, 99]] }, [1, 0]), null);
    assert.equal(NodeMod.stack_find({ entities: [[1, 99], [2, 0]] }, [2, 0]), 1);

    const world = new Ecs.World();
    const windows = world.register_storage();
    world.create_entity();
    const old = world.create_entity();
    windows.insert(old, 'old window');
    const pendingFocus = () => windows.get(old);
    world.delete_entity(old);
    const replacement = world.create_entity();
    windows.insert(replacement, 'replacement window');
    assert.deepEqual(old, [1, 0]);
    assert.equal(pendingFocus(), null);
    assert.notEqual(replacement, old);
    assert.deepEqual(replacement, [1, 1]);
    assert.equal(windows.get(replacement), 'replacement window');

    const slotWorld = new Ecs.World();
    const slotWindows = slotWorld.register_storage();
    const slotOld = slotWorld.create_entity();
    slotWindows.insert(slotOld, 'zero');
    slotWorld.delete_entity(slotOld);
    const slotNext = slotWorld.create_entity();
    slotWindows.insert(slotNext, 'next');
    assert.deepEqual(slotOld, [0, 0]);
    assert.deepEqual(slotNext, [0, 1]);
    assert.notEqual(slotOld, slotNext);
    assert.equal(slotWindows.get(slotOld), null);
    assert.equal(slotWindows.get(slotNext), 'next');
    slotWorld.delete_entity(slotOld);
    const third = slotWorld.create_entity();
    assert.deepEqual(third, [1, 0]);
    assert.equal(slotWindows.get(slotNext), 'next');
}

function assertWobblySteps() {
    assert.equal(safe_speedup_divider(0), DEFAULT_SPEEDUP_FACTOR);
    assert.equal(safe_speedup_divider(-4), DEFAULT_SPEEDUP_FACTOR);
    assert.equal(safe_speedup_divider(Number.POSITIVE_INFINITY), DEFAULT_SPEEDUP_FACTOR);
    assert.equal(safe_speedup_divider(Number.NaN), DEFAULT_SPEEDUP_FACTOR);
    assert.equal(safe_speedup_divider(8), 8);

    const raw = {
        SPEEDUP_FACTOR: {
            key: 'speedup-factor-divider',
            get() { return 0; },
            set() {},
        },
    };
    const prepared = settings_for_effect(raw);
    assert.equal(prepared.SPEEDUP_FACTOR.get(), DEFAULT_SPEEDUP_FACTOR);
    assert.equal(settings_for_effect({
        SPEEDUP_FACTOR: { key: 'speedup-factor-divider', get() { return 12; }, set() {} },
    }).SPEEDUP_FACTOR.get(), 12);

    const model = new WobblyModel({ sizeX: 200, sizeY: 120, friction: 3.5, springK: 3.8, mass: 70 });
    const started = Date.now();
    model.step(16 / 0);
    model.step(Number.POSITIVE_INFINITY);
    model.step(Number.NaN);
    const elapsed = Date.now() - started;
    assert.ok(elapsed < 2000, `step did not return promptly (${elapsed}ms)`);
}

assertFloatLiterals();
assertEntities();
assertWobblySteps();
console.log('desktop js regressions ok');
