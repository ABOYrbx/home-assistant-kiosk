import test from 'node:test';
import assert from 'node:assert/strict';
import { Clock, CLOCK_LIMITS } from './clock.js';

function fakeScheduler() {
  let now = 0;
  let pending = null;
  return {
    now: () => now,
    requestFrame(callback) {
      pending = callback;
      return 1;
    },
    cancelFrame() {
      pending = null;
    },
    advance(ms) {
      now += ms;
      const callback = pending;
      pending = null;
      if (callback) callback();
    },
    get pending() {
      return pending !== null;
    },
  };
}

test('play advances the clock and pause freezes it', () => {
  const scheduler = fakeScheduler();
  const updates = [];
  const clock = new Clock({ duration: 10000, scheduler, onUpdate: (c) => updates.push(c.position) });

  clock.play();
  assert.equal(clock.playing, true);
  scheduler.advance(2000);
  assert.equal(clock.position, 2000);
  clock.pause();
  scheduler.advance(5000);
  assert.equal(clock.position, 2000);
  assert.equal(clock.playing, false);
  assert.ok(updates.length >= 2);
});

test('seek clamps to the duration and restart returns to zero', () => {
  const clock = new Clock({ duration: 10000, scheduler: fakeScheduler() });
  clock.seek(99999);
  assert.equal(clock.position, 10000);
  clock.seek(-500);
  assert.equal(clock.position, 0);
  clock.seek(6000);
  clock.restart();
  assert.equal(clock.position, 0);
});

test('offset is applied to the lyrics position, snapped and clamped', () => {
  const clock = new Clock({ duration: 10000, offsetMs: 137, scheduler: fakeScheduler() });
  assert.equal(clock.offsetMs, 150);
  clock.setOffset(99999);
  assert.equal(clock.offsetMs, CLOCK_LIMITS.OFFSET_LIMIT_MS);
  clock.setOffset(-99999);
  assert.equal(clock.offsetMs, -CLOCK_LIMITS.OFFSET_LIMIT_MS);
});

test('effective position combines offset and clamps', () => {
  const clock = new Clock({ duration: 10000, offsetMs: 500, scheduler: fakeScheduler() });
  clock.seek(2000);
  assert.equal(clock.effectivePosition, 2500);
  clock.setOffset(-2000);
  assert.equal(clock.effectivePosition, 0);
});

test('suspend and resume do not jump the playhead', () => {
  const scheduler = fakeScheduler();
  const clock = new Clock({ duration: 10000, scheduler });
  clock.play();
  scheduler.advance(1000);
  clock.suspend();
  scheduler.advance(30000);
  assert.equal(clock.position, 1000);
  assert.equal(clock.playing, true);
  clock.resume();
  scheduler.advance(500);
  assert.equal(clock.position, 1500);
});

test('the clock stops at the end', () => {
  const scheduler = fakeScheduler();
  const clock = new Clock({ duration: 1000, scheduler });
  clock.play();
  scheduler.advance(5000);
  assert.equal(clock.position, 1000);
  assert.equal(clock.playing, false);
  assert.equal(scheduler.pending, false);
});

test('setDuration clamps an existing position', () => {
  const clock = new Clock({ duration: 10000, scheduler: fakeScheduler() });
  clock.seek(8000);
  clock.setDuration(4000);
  assert.equal(clock.position, 4000);
  assert.equal(clock.duration, 4000);
});
