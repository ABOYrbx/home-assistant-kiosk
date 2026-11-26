import test from 'node:test';
import assert from 'node:assert/strict';
import { validateConfig } from './config.js';

const valid = {
  ma_url: 'http://music-assistant.local:8095',
  ma_token: 'long-lived-token',
  spicy_key: 'sl_pk_example',
};

test('valid config gets safe defaults and a snapped offset', () => {
  const result = validateConfig({ ...valid, offset_ms: 313 });
  assert.equal(result.ok, true);
  assert.equal(result.setup.length, 0);
  assert.equal(result.config.offsetMs, 300);
  assert.equal(result.config.backPath, '/lovelace/0');
  assert.equal(result.config.fullscreen, true);
  assert.equal(result.config.player, '');
});

test('missing secrets are a setup state, not a structural error', () => {
  const result = validateConfig({});
  assert.equal(result.ok, true);
  assert.ok(result.setup.some((line) => line.includes('ma_url')));
  assert.ok(result.setup.some((line) => line.includes('ma_token')));
  assert.ok(result.setup.some((line) => line.includes('spicy_key')));
});

test('a secret key or placeholder is rejected as a setup error', () => {
  assert.equal(validateConfig({ ...valid, spicy_key: 'sl_sk_secret' }).setup.length, 1);
  assert.equal(validateConfig({ ...valid, spicy_key: 'sl_pk_replace_me' }).setup.length, 1);
  assert.equal(validateConfig({ ...valid, spicy_key: 'sl_pk_realkey' }).setup.length, 0);
});

test('structural errors are reported for bad URL, offset, path and flag', () => {
  const result = validateConfig({
    ...valid, ma_url: 'ftp://example.org', offset_ms: Number.NaN,
    back_path: 'lovelace/0', fullscreen: 'yes',
  });
  assert.equal(result.ok, false);
  assert.ok(result.problems.some((line) => line.includes('ma_url')));
  assert.ok(result.problems.some((line) => line.includes('offset_ms')));
  assert.ok(result.problems.some((line) => line.includes('back_path')));
  assert.ok(result.problems.some((line) => line.includes('fullscreen')));
});

test('offset is clamped to plus or minus 2000 ms', () => {
  assert.equal(validateConfig({ ...valid, offset_ms: 9999 }).config.offsetMs, 2000);
  assert.equal(validateConfig({ ...valid, offset_ms: -9999 }).config.offsetMs, -2000);
  assert.equal(validateConfig({ ...valid, fullscreen: false }).config.fullscreen, false);
});
