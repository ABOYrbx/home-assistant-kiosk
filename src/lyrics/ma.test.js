import test from 'node:test';
import assert from 'node:assert/strict';
import { MaAdapter, deriveWsUrl, redactUrl, safeError } from './ma.js';

class FakeClient {
  constructor() {
    this.listeners = new Map();
    this.stateListeners = new Set();
    this.players = new Map();
    this.queues = new Map();
    this.isConnected = false;
    this.calls = [];
  }

  on(event, listener) {
    if (!this.listeners.has(event)) this.listeners.set(event, new Set());
    this.listeners.get(event).add(listener);
    return () => this.listeners.get(event)?.delete(listener);
  }

  onStateChange(listener) {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  }

  emit(event, data, objectId = null) {
    for (const listener of this.listeners.get(event) ?? []) listener(data, objectId);
  }

  emitState(state) {
    this.isConnected = state === 'connected';
    for (const listener of this.stateListeners) listener(state);
  }

  start() {}

  stop() {}

  async playPause(id) { this.calls.push(['playPause', id]); }

  async next(id) { this.calls.push(['next', id]); }

  async previous(id) { this.calls.push(['previous', id]); }

  async seek(id, seconds) { this.calls.push(['seek', id, seconds]); }

  async setVolume(id, level) { this.calls.push(['setVolume', id, level]); }

  async setMute(id, muted) { this.calls.push(['setMute', id, muted]); }

  async resync(id) { this.calls.push(['resync', id]); }
}

function adapterWithData() {
  const client = new FakeClient();
  const adapter = new MaAdapter({
    url: 'http://music-assistant.local:8095',
    token: 'secret-token',
    clientFactory: () => client,
  });
  client.players.set('p1', {
    player_id: 'p1', name: 'Stage', available: true, powered: true,
    playback_state: 'playing', volume_level: 25, volume_muted: false,
    current_media: { uri: 'spotify:track:one', title: 'Title', duration: 180 },
  });
  client.queues.set('p1', {
    queue_id: 'p1', active: true, state: 'playing', elapsed_time: 42,
    current_item: { duration: 180, media_item: { uri: 'spotify:track:one' } },
  });
  return { adapter, client };
}

test('URL derivation and redaction never expose credentials', () => {
  assert.equal(deriveWsUrl('http://music-assistant.local:8095'), 'ws://music-assistant.local:8095/ws');
  assert.equal(deriveWsUrl('https://music-assistant.local'), 'wss://music-assistant.local/ws');
  assert.equal(deriveWsUrl('not a url'), '');
  assert.equal(redactUrl('http://user:hunter2@music-assistant.local:8095/ws?token=abc'),
    'http://music-assistant.local:8095/ws');
  assert.equal(redactUrl('nonsense'), '<invalid-url>');
  assert.ok(!safeError(new Error('auth failed for hunter2secrettoken012345678901234567')).includes('hunter2'));
});

test('adapter maps players, queues, elapsed time and connection state', () => {
  const { adapter, client } = adapterWithData();
  const states = [];
  adapter.onConnection((state) => states.push(state));
  adapter.start();
  adapter.selectPlayerId('p1');
  client.emitState('connected');
  const snapshot = adapter.snapshot();
  assert.equal(snapshot.playerName, 'Stage');
  assert.equal(snapshot.queue.queue_id, 'p1');
  assert.equal(snapshot.elapsedMs, 42000);
  assert.equal(snapshot.connected, true);
  assert.deepEqual(states, ['connected']);
  adapter.stop();
  assert.equal(states.length, 1);
});

test('commands are confirmed by events, not by send alone', async () => {
  const { adapter, client } = adapterWithData();
  adapter.start();
  adapter.selectPlayerId('p1');

  const pause = adapter.playPause('p1', 'playing');
  await Promise.resolve();
  client.emit('queue_updated', { state: 'paused' }, 'p1');
  assert.deepEqual(await pause, { confirmed: true });

  const volume = adapter.setVolume('p1', 40);
  await Promise.resolve();
  client.emit('player_updated', { volume_level: 40 }, 'p1');
  assert.deepEqual(await volume, { confirmed: true });

  assert.deepEqual(client.calls, [['playPause', 'p1'], ['setVolume', 'p1', 40]]);
  adapter.stop();
});

test('an unconfirmed command is reported as unconfirmed', async () => {
  const { adapter } = adapterWithData();
  const original = globalThis.setTimeout;
  globalThis.setTimeout = (fn) => { fn(); return 0; };
  try {
    assert.deepEqual(await adapter.seek('p1', 5000, 0), { confirmed: false });
  } finally {
    globalThis.setTimeout = original;
  }
});

test('a rejected command surfaces an error through the adapter', async () => {
  const { adapter, client } = adapterWithData();
  client.playPause = async () => { throw new Error('secret-token rejected'); };
  await assert.rejects(() => adapter.playPause('p1', 'playing'), /rejected/);
});
