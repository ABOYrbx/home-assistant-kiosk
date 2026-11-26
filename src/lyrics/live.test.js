import test from 'node:test';
import assert from 'node:assert/strict';
import {
  PositionAnchor, detectTrackChange, trackFromMedia, resolvePlayer, mapCapabilities, PendingActions,
} from './live.js';

test('position interpolates while playing and freezes on pause', () => {
  let now = 1000;
  const anchor = new PositionAnchor({ now: () => now });
  anchor.setDuration(200000);
  anchor.sync(10000, { playing: true });
  now += 150;
  assert.equal(anchor.positionMs(), 10150);
  anchor.setPlaying(false);
  now += 5000;
  assert.equal(anchor.positionMs(), 10150);
  anchor.sync(40000, { playing: false });
  assert.equal(anchor.positionMs(), 40000);
});

test('offset is applied only to the lyric position and clamps to duration', () => {
  let now = 0;
  const anchor = new PositionAnchor({ now: () => now });
  anchor.setDuration(60000);
  anchor.setOffset(300);
  anchor.sync(1000, { playing: true });
  now += 500;
  assert.equal(anchor.positionMs(), 1500);
  assert.equal(anchor.lyricMs(), 1200);
  anchor.setOffset(9000);
  assert.equal(anchor.offsetMs, 2000);
  anchor.setOffset(-9000);
  assert.equal(anchor.offsetMs, -2000);
  now += 100000;
  assert.equal(anchor.positionMs(), 60000);
});

test('stale anchor and freeze/unfreeze keep a frozen offline frame', () => {
  let now = 0;
  const anchor = new PositionAnchor({ now: () => now });
  anchor.setDuration(300000);
  anchor.sync(1000, { playing: true });
  now += 31000;
  assert.equal(anchor.isStale(), true);
  anchor.freeze();
  const frozen = anchor.positionMs();
  now += 5000;
  assert.equal(anchor.positionMs(), frozen);
  assert.equal(anchor.isStale(), false);
  anchor.unfreeze();
  now += 1000;
  assert.equal(anchor.positionMs(), frozen + 1000);
});

test('track change detection uses uri plus title, artist, and duration', () => {
  const a = { uri: 'spotify:track:one', title: 'A', artist: 'B', durationMs: 1000 };
  assert.equal(detectTrackChange(null, a), true);
  assert.equal(detectTrackChange(a, { ...a }), false);
  assert.equal(detectTrackChange(a, { ...a, uri: 'spotify:track:two' }), true);
  assert.equal(detectTrackChange(a, { ...a, title: 'C' }), true);
  assert.equal(detectTrackChange(a, { ...a, artist: 'C' }), true);
  assert.equal(detectTrackChange(a, { ...a, durationMs: 5000 }), true);
  assert.equal(detectTrackChange(a, null), false);
});

test('trackFromMedia reads MA current_media and queue items', () => {
  const media = {
    uri: 'spotify://track/abc', title: 'Title', artist: 'Artist',
    album: 'Album', image_url: 'https://example.org/cover', duration: 200,
  };
  assert.deepEqual(trackFromMedia(media), {
    uri: 'spotify://track/abc', title: 'Title', artist: 'Artist', album: 'Album',
    imageUrl: 'https://example.org/cover', durationMs: 200000,
  });
  assert.equal(trackFromMedia(null, { current_item: { media_item: media, duration: 201 } }).durationMs, 201000);
  assert.equal(trackFromMedia(null), null);
});

test('player selection pins by id or name, else picks playing then last active', () => {
  const players = [
    { player_id: 'p1', name: 'Kitchen', available: true, playback_state: 'idle' },
    { player_id: 'p2', name: 'Living Room', display_name: 'Stage', available: true, playback_state: 'playing' },
    { player_id: 'p3', name: 'Hidden', available: true, hide_in_ui: true, playback_state: 'playing' },
  ];
  assert.equal(resolvePlayer(players, { configured: 'p1' }).player_id, 'p1');
  assert.equal(resolvePlayer(players, { configured: 'stage' }).player_id, 'p2');
  assert.equal(resolvePlayer(players, { configured: 'missing' }), null);
  assert.equal(resolvePlayer(players, { persistedId: 'p1' }).player_id, 'p1');
  assert.equal(resolvePlayer(players, {}).player_id, 'p2');
  assert.equal(resolvePlayer([{ player_id: 'p4', name: 'Off', available: false }], {}), null);
});

test('capabilities are disabled only when the model withholds them', () => {
  const queue = { queue_id: 'q1', current_item: { duration: 180 } };
  const player = { available: true, powered: true, volume_level: 25, volume_muted: false, current_media: { duration: 180 } };
  assert.deepEqual(mapCapabilities(player, queue), {
    play: true, previous: true, next: true, seek: true, volume: true, mute: true,
  });
  const fixed = mapCapabilities({ ...player, volume_level: undefined, volume_muted: undefined }, queue);
  assert.equal(fixed.volume, false);
  assert.equal(fixed.mute, false);
  assert.equal(mapCapabilities(player, null).play, false);
});

test('pending actions expire and are cleared', () => {
  let now = 0;
  const pending = new PendingActions({ now: () => now, timeoutMs: 1000 });
  pending.begin('play_pause');
  assert.equal(pending.has('play_pause'), true);
  assert.equal(pending.busy, true);
  now = 1001;
  assert.equal(pending.has('play_pause'), false);
  assert.equal(pending.busy, false);
  pending.begin('seek');
  pending.clear();
  assert.equal(pending.busy, false);
});
