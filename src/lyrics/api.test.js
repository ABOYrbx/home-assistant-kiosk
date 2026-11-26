import test from 'node:test';
import assert from 'node:assert/strict';
import { SpicyClient, LyricsCache, LyricsApiError, mapHttpError } from './api.js';

function memoryStorage() {
  const data = new Map();
  return {
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => data.set(k, String(v)),
    removeItem: (k) => data.delete(k),
  };
}

function jsonResponse(body, { status = 200, headers = {} } = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: (name) => headers[name.toLowerCase()] ?? null },
    json: async () => body,
  };
}

const LYRIC_BODY = {
  Body: {
    Type: 'Line', source: 'spotify', StartTime: 0, EndTime: 10,
    Content: [{ Text: 'Hello', StartTime: 0, EndTime: 5 }, { Text: 'world', StartTime: 5, EndTime: 10 }],
  },
  Status: 200,
};

test('error mapping is distinct and carries Retry-After', () => {
  assert.equal(mapHttpError(401).kind, 'unauthorized');
  assert.equal(mapHttpError(403).kind, 'forbidden');
  assert.equal(mapHttpError(404).kind, 'not_found');
  assert.equal(mapHttpError(500).kind, 'server');
  const limited = mapHttpError(429, '12');
  assert.equal(limited.kind, 'rate_limited');
  assert.equal(limited.retryAfterMs, 12000);
  assert.match(limited.message, /12 s/);
});

test('cache is bounded by TTL, count, and size', () => {
  let now = 0;
  const cache = new LyricsCache({ storage: memoryStorage(), now: () => now });
  cache.set('a', { type: 'Line' });
  assert.ok(cache.get('a'));
  now = 24 * 60 * 60 * 1000 + 1;
  assert.equal(cache.get('a'), null);

  now = 0;
  for (let i = 0; i < 35; i += 1) cache.set(`track-${i}`, { i });
  assert.equal(cache.entries.size, 30);
  assert.equal(cache.entries.has('track-0'), false);
  assert.equal(cache.entries.has('track-34'), true);
  cache.set('track-34', { i: 34 });
  cache.get('track-30');
  assert.equal([...cache.entries.keys()].at(-1), 'track-30');
  cache.clear();
  assert.equal(cache.entries.size, 0);
});

test('cache survives a new instance via storage', () => {
  const storage = memoryStorage();
  const first = new LyricsCache({ storage, now: () => 0 });
  first.set('shared', { type: 'Static' });
  const second = new LyricsCache({ storage, now: () => 0 });
  assert.deepEqual(second.get('shared'), { type: 'Static' });
});

test('getLyrics sends the key only as a Bearer header and caches the model', async () => {
  const calls = [];
  const fetchImpl = async (url, options) => {
    calls.push({ url, options });
    return jsonResponse(LYRIC_BODY);
  };
  const client = new SpicyClient({ key: 'sl_pk_example', fetchImpl, cache: new LyricsCache({ storage: null }) });
  const first = await client.getLyrics('spotify-id');
  const second = await client.getLyrics('spotify-id');
  assert.equal(first.type, 'Line');
  assert.equal(second, first);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, 'https://api.spicylyrics.org/v1/lyrics/spotify-id');
  assert.equal(calls[0].options.headers.Authorization, 'Bearer sl_pk_example');
  assert.equal(calls[0].options.headers.Authorization.includes(calls[0].url), false);
  assert.equal(calls[0].url.includes('sl_pk_'), false);
});

test('getLyrics maps 404 and missing key to actionable states', async () => {
  const missing = new SpicyClient({ key: '', fetchImpl: async () => jsonResponse({}) });
  await assert.rejects(() => missing.getLyrics('id'), (error) => error.kind === 'missing_key');
  const notFound = new SpicyClient({
    key: 'sl_pk_example',
    fetchImpl: async () => jsonResponse({}, { status: 404 }),
    cache: new LyricsCache({ storage: null }),
  });
  await assert.rejects(() => notFound.getLyrics('id'), (error) => error.kind === 'not_found');
});

test('searchTracks builds a query with metadata but never a key in the URL', async () => {
  let url = '';
  const client = new SpicyClient({
    key: 'sl_pk_example',
    fetchImpl: async (value) => {
      url = value;
      return jsonResponse({ Body: { results: [{ id: 'id', tier: 'exact' }] } });
    },
    cache: new LyricsCache({ storage: null }),
  });
  const results = await client.searchTracks({ title: 'A Song', artist: 'An Artist', duration: 181.4 });
  assert.equal(results.length, 1);
  const parsed = new URL(url);
  assert.equal(parsed.pathname, '/v1/tracks/search');
  assert.equal(parsed.searchParams.get('title'), 'A Song');
  assert.equal(parsed.searchParams.get('artist'), 'An Artist');
  assert.equal(parsed.searchParams.get('duration'), '181');
  assert.equal(parsed.searchParams.get('limit'), '5');
  assert.equal(url.includes('sl_pk_'), false);
});

test('network failures are reported as offline, errors never leak the key', async () => {
  const client = new SpicyClient({
    key: 'sl_pk_supersecret',
    fetchImpl: async () => { throw new Error('fetch failed for sl_pk_supersecret'); },
    cache: new LyricsCache({ storage: null }),
  });
  await assert.rejects(() => client.getLyrics('id'), (error) => {
    assert.ok(error instanceof LyricsApiError);
    assert.equal(error.kind, 'network');
    assert.equal(error.message.includes('supersecret'), false);
    return true;
  });
});
