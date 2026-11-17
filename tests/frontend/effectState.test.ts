import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  classicTarget, createEffectController, DEFAULT_CUSTOM_COLORS, defaultStripSettings, effectColorError, isRainbowAt,
  RAINBOW_SPEEDS, sameTarget, stripSettings, toEffectColor, updateStrip, withCustomColor,
} from '../../src/lib/effectState.ts';
import type { ConfirmedEffect, CustomColors, EffectTarget, StripSettings } from '../../src/lib/effectState.ts';

const A = '192.168.0.10';
const B = '192.168.0.11';

/** Fake backend: every call waits until the test resolves or rejects it. */
function fakeDevice() {
  const calls: { ip: string; target: EffectTarget; brightness: number;
    resolve: () => void; reject: (e: Error) => void }[] = [];
  const apply = (ip: string, target: EffectTarget, brightness: number) =>
    new Promise<ConfirmedEffect>((resolve, reject) => {
      calls.push({
        ip, target, brightness,
        resolve: () => resolve({ target, brightness, elapsedMs: 1 }),
        reject,
      });
    });
  return { calls, controller: createEffectController(apply) };
}

const flush = () => new Promise(r => setImmediate(r));

test('effect colors accept the backend limits and reject everything else', () => {
  assert.equal(effectColorError({ h: 0, s: 0, v: 1 }), null);
  assert.equal(effectColorError({ h: 360, s: 100, v: 100 }), null);
  for (const bad of [
    { h: -1, s: 50, v: 50 }, { h: 361, s: 50, v: 50 }, { h: 10, s: -1, v: 50 }, { h: 10, s: 101, v: 50 },
    { h: 10, s: 50, v: 0 }, { h: 10, s: 50, v: 101 }, { h: 1.5, s: 50, v: 50 }, { h: NaN, s: 50, v: 50 },
  ]) {
    assert.notEqual(effectColorError(bad), null, JSON.stringify(bad));
  }
});

test('picker values are rounded and clamped before they are stored', () => {
  assert.deepEqual(toEffectColor({ h: 359.6, s: 84.4, v: 0 }), { h: 360, s: 84, v: 1 });
  assert.deepEqual(toEffectColor({ h: -3, s: 120, v: 100.2 }), { h: 0, s: 100, v: 100 });
  assert.deepEqual(toEffectColor({ h: NaN, s: 50, v: 50 }), { h: 0, s: 50, v: 50 });
});

test('defaults are the X1/X2 red, white and blue', () => {
  assert.deepEqual(DEFAULT_CUSTOM_COLORS, [{ h: 359, s: 85, v: 100 }, { h: 0, s: 0, v: 100 }, { h: 236, s: 72, v: 100 }]);
});

test('only three Rainbow speeds exist and only Rainbow carries one', () => {
  assert.deepEqual(RAINBOW_SPEEDS.map(s => s.id), ['fast', 'normal', 'slow']);
  assert.deepEqual(classicTarget('rainbow', 'slow'), { kind: 'classic', id: 'rainbow', rainbowSpeed: 'slow' });
  assert.deepEqual(classicTarget('aurora', 'slow'), { kind: 'classic', id: 'aurora' });
});

test('a brightness change keeps the selected Rainbow speed', async () => {
  const { calls, controller } = fakeDevice();
  const sent = controller.send(A, classicTarget('rainbow', 'slow'), 80);
  calls[0].resolve();
  assert.equal((await sent).current, true);
  const resent = controller.resend(A, 30);
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[1].target, { kind: 'classic', id: 'rainbow', rainbowSpeed: 'slow' });
  assert.equal(calls[1].brightness, 30);
  calls[1].resolve();
  assert.equal((await resent)?.confirmed.brightness, 30);
});

test('a brightness change keeps the custom colors', async () => {
  const { calls, controller } = fakeDevice();
  const colors: CustomColors = [{ h: 120, s: 100, v: 100 }, { h: 45, s: 60, v: 80 }, { h: 300, s: 0, v: 50 }];
  const sent = controller.send(A, { kind: 'custom', mode: 'sequence', colors }, 60);
  calls[0].resolve();
  await sent;
  void controller.resend(A, 20);
  assert.deepEqual(calls[1].target, { kind: 'custom', mode: 'sequence', colors });
  assert.equal(calls[1].brightness, 20);
});

test('rapid effect changes: the last request wins and late replies are marked stale', async () => {
  const { calls, controller } = fakeDevice();
  const first = controller.send(A, classicTarget('aurora', 'normal'), 50);
  const second = controller.send(A, classicTarget('rainbow', 'fast'), 50);
  const third = controller.send(A, { kind: 'segment', id: 'disco' }, 50);
  calls[0].resolve();
  calls[1].resolve();
  calls[2].resolve();
  assert.equal((await first).current, false);
  assert.equal((await second).current, false);
  assert.equal((await third).current, true);
  void controller.resend(A, 10);
  assert.deepEqual(calls[3].target, { kind: 'segment', id: 'disco' });
});

test('a failed request restores the previous target unless it was already replaced', async () => {
  const { calls, controller } = fakeDevice();
  const ok = controller.send(A, classicTarget('rainbow', 'slow'), 50);
  calls[0].resolve();
  await ok;

  const failing = controller.send(A, classicTarget('ocean', 'slow'), 50);
  calls[1].reject(new Error('Set effect failed: timeout'));
  await assert.rejects(failing, /timeout/);
  assert.deepEqual(controller.current(A), classicTarget('rainbow', 'slow'));

  const stale = controller.send(A, classicTarget('aurora', 'slow'), 50);
  const newer = controller.send(A, { kind: 'segment', id: 'disco' }, 50);
  calls[2].reject(new Error('Set effect failed'));
  await assert.rejects(stale);
  assert.deepEqual(controller.current(A), { kind: 'segment', id: 'disco' });
  calls[3].resolve();
  assert.equal((await newer).current, true);
});

test('rapid brightness changes resend the same latest target each time', async () => {
  const { calls, controller } = fakeDevice();
  const sent = controller.send(A, classicTarget('rainbow', 'slow'), 50);
  calls[0].resolve();
  await sent;
  const resends = [10, 40, 90].map(b => controller.resend(A, b));
  calls.slice(1).forEach(c => c.resolve());
  const results = await Promise.all(resends);
  assert.deepEqual(calls.slice(1).map(c => c.brightness), [10, 40, 90]);
  assert.ok(calls.slice(1).every(c => sameTarget(c.target, classicTarget('rainbow', 'slow'))));
  assert.deepEqual(results.map(r => r?.current), [false, false, true]);
});

test('the two strips keep separate targets', async () => {
  const { calls, controller } = fakeDevice();
  const a = controller.send(A, classicTarget('rainbow', 'slow'), 70);
  const b = controller.send(B, { kind: 'custom', mode: 'static', colors: DEFAULT_CUSTOM_COLORS }, 20);
  calls[1].resolve();
  calls[0].resolve();
  assert.equal((await a).current, true);
  assert.equal((await b).current, true);
  void controller.resend(B, 30);
  assert.equal(calls[2].ip, B);
  assert.equal(calls[2].target.kind, 'custom');
  controller.clear(B);
  assert.equal(await controller.resend(B, 40), null);
  assert.deepEqual(controller.current(A), classicTarget('rainbow', 'slow'));
});

test('after a color or white command there is nothing to resend', async () => {
  const { calls, controller } = fakeDevice();
  const sent = controller.send(A, classicTarget('rainbow', 'fast'), 50);
  calls[0].resolve();
  await sent;
  controller.clear(A);
  assert.equal(await controller.resend(A, 60), null);
  assert.equal(calls.length, 1);
});

test('a late color reply does not drop an effect sent after it', async () => {
  const { calls, controller } = fakeDevice();
  const before = controller.snapshot(A);
  const effect = controller.send(A, classicTarget('rainbow', 'slow'), 50);
  calls[0].resolve();
  await effect;
  assert.equal(controller.clearIfUnchanged(A, before), false);
  assert.deepEqual(controller.current(A), classicTarget('rainbow', 'slow'));
  const again = controller.snapshot(A);
  assert.equal(controller.clearIfUnchanged(A, again), true);
  assert.equal(controller.current(A), undefined);
});

test('a color reply on one strip leaves the other strip alone', async () => {
  const { calls, controller } = fakeDevice();
  const onB = controller.send(B, { kind: 'custom', mode: 'static', colors: DEFAULT_CUSTOM_COLORS }, 40);
  calls[0].resolve();
  await onB;
  assert.equal(controller.clearIfUnchanged(A, controller.snapshot(A)), true);
  assert.equal(controller.current(B)?.kind, 'custom');
});

test('a late reply for an older speed is not recorded as the selected speed', async () => {
  const { calls, controller } = fakeDevice();
  const slow = controller.send(A, classicTarget('rainbow', 'slow'), 50);
  const fast = controller.send(A, classicTarget('rainbow', 'fast'), 50);
  calls[1].resolve();
  await fast;
  assert.equal(isRainbowAt(controller.current(A), 'fast'), true);
  calls[0].resolve();
  assert.equal((await slow).current, false);
  assert.equal(isRainbowAt(controller.current(A), 'slow'), false);
  assert.equal(isRainbowAt({ kind: 'classic', id: 'aurora' }, 'fast'), false);
  assert.equal(isRainbowAt(undefined, 'normal'), false);
});

test('switching strips mid-request keeps each reply on its own strip', async () => {
  const { calls, controller } = fakeDevice();
  const onA = controller.send(A, { kind: 'custom', mode: 'sequence', colors: DEFAULT_CUSTOM_COLORS }, 50);
  const onB = controller.send(B, classicTarget('rainbow', 'slow'), 20);
  const brightnessB = controller.resend(B, 80);
  calls[0].resolve();
  calls[1].resolve();
  calls[2].resolve();
  assert.equal((await onA).current, true);
  assert.equal((await onB).current, false);
  assert.equal((await brightnessB)?.current, true);
  assert.equal(calls[2].ip, B);
  assert.deepEqual(calls[2].target, classicTarget('rainbow', 'slow'));
  assert.equal(controller.current(A)?.kind, 'custom');
});

test('strip settings are independent per strip and per mode', () => {
  let all: Record<string, StripSettings> = {};
  all = updateStrip(all, A, s => ({ ...s, rainbowSpeed: 'slow' }));
  all = updateStrip(all, A, s => withCustomColor(s, 'static', 1, { h: 120, s: 100, v: 100 }));
  assert.equal(stripSettings(all, A).rainbowSpeed, 'slow');
  assert.equal(stripSettings(all, B).rainbowSpeed, 'normal');
  assert.deepEqual(stripSettings(all, A).customColors.static[1], { h: 120, s: 100, v: 100 });
  assert.deepEqual(stripSettings(all, A).customColors.sequence, DEFAULT_CUSTOM_COLORS);
  assert.deepEqual(stripSettings(all, B).customColors.static, DEFAULT_CUSTOM_COLORS);
  assert.deepEqual(DEFAULT_CUSTOM_COLORS[1], { h: 0, s: 0, v: 100 });
});

test('rapid color edits keep only the last value and send nothing', () => {
  const { calls } = fakeDevice();
  let s = defaultStripSettings();
  for (const h of [10, 20, 30, 40.4]) s = withCustomColor(s, 'sequence', 0, { h, s: 50, v: 50 });
  assert.deepEqual(s.customColors.sequence[0], { h: 40, s: 50, v: 50 });
  assert.equal(calls.length, 0);
});

test('invalid color slots are ignored', () => {
  const s = defaultStripSettings();
  assert.equal(withCustomColor(s, 'static', 3, { h: 1, s: 1, v: 1 }), s);
  assert.equal(withCustomColor(s, 'static', -1, { h: 1, s: 1, v: 1 }), s);
});

test('targets compare by content', async () => {
  await flush();
  assert.ok(sameTarget(classicTarget('rainbow', 'slow'), classicTarget('rainbow', 'slow')));
  assert.ok(!sameTarget(classicTarget('rainbow', 'slow'), classicTarget('rainbow', 'fast')));
  assert.ok(!sameTarget({ kind: 'segment', id: 'disco' }, { kind: 'classic', id: 'disco' }));
  const colors = [...DEFAULT_CUSTOM_COLORS] as CustomColors;
  assert.ok(sameTarget({ kind: 'custom', mode: 'static', colors }, { kind: 'custom', mode: 'static', colors: DEFAULT_CUSTOM_COLORS }));
  assert.ok(!sameTarget({ kind: 'custom', mode: 'static', colors }, { kind: 'custom', mode: 'sequence', colors }));
});
