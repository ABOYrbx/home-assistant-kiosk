import test from 'node:test';
import assert from 'node:assert/strict';
import {
  API_BASE,
  LyricsApiError,
  LyricsCache,
  getLyrics,
  searchTracks,
  parseRetryAfter,
  errorMessage,
  isPublishableKey,
} from './api.js';

const KEY = 'sl_pk_testkey123';

function jsonResponse(status, body, headers = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers: { get: (name) => headers[name] ?? headers[name.toLowerCase()] ?? null },
    json: async () => body,
  };
}

test('getLyrics sends the key in a header and never in the URL', async () => {
  let seen;
  const fetchImpl = async (url, options) => {
    seen = { url, options };
    return jsonResponse(200, { Body: { Type: 'Static', Content: [] } });
  };
  const result = await getLyrics('abc/123', { key: KEY, fetchImpl, online: true });
  assert.equal(seen.url, `${API_BASE}/v1/lyrics/abc%2F123`);
  assert.equal(seen.options.headers.Authorization, `Bearer ${KEY}`);
  assert.equal(seen.url.includes(KEY), false);
  assert.equal(result.Body.Type, 'Static');
});

test('publishable key validation rejects secret keys and junk', () => {
  assert.equal(isPublishableKey(KEY), true);
  assert.equal(isPublishableKey('sl_sk_secret'), false);
  assert.equal(isPublishableKey(''), false);
});

test('API errors map to distinct actionable codes', async () => {
  const cases = [
    [401, {}, 'unauthorized'],
    [403, {}, 'forbidden'],
    [404, {}, 'not_found'],
    [429, { 'Retry-After': '12' }, 'rate_limited'],
    [503, {}, 'server'],
  ];
  for (const [status, headers, code] of cases) {
    await assert.rejects(
      getLyrics('id', {
        key: KEY,
        online: true,
        fetchImpl: async () => jsonResponse(status, {}, headers),
      }),
      (error) => {
        assert.ok(error instanceof LyricsApiError);
        assert.equal(error.code, code);
        return true;
      },
    );
  }
});

test('429 carries Retry-After and produces a distinct message', async () => {
  await assert.rejects(
    getLyrics('id', {
      key: KEY,
      online: true,
      fetchImpl: async () => jsonResponse(429, {}, { 'Retry-After': '12' }),
    }),
    (error) => {
      assert.equal(error.retryAfterMs, 12000);
      assert.match(errorMessage(error), /retry in 12 s/);
      return true;
    },
  );
});

test('missing key, offline and network failures never fetch', async () => {
  let calls = 0;
  const fetchImpl = async () => {
    calls += 1;
    return jsonResponse(200, {});
  };
  await assert.rejects(getLyrics('id', { key: '', fetchImpl, online: true }), {
    code: 'missing_key',
  });
  await assert.rejects(getLyrics('id', { key: KEY, fetchImpl, online: false }), {
    code: 'offline',
  });
  await assert.rejects(
    getLyrics('id', {
      key: KEY,
      online: true,
      fetchImpl: async () => {
        throw new Error('connection reset');
      },
    }),
    { code: 'offline' },
  );
  assert.equal(calls, 0);
});

test('a stalled request is aborted and reported as a timeout', async () => {
  await assert.rejects(
    getLyrics('id', {
      key: KEY,
      online: true,
      timeoutMs: 5,
      fetchImpl: (_url, { signal }) =>
        new Promise((_resolve, reject) => {
          signal.addEventListener('abort', () => reject(new Error('aborted')), { once: true });
        }),
    }),
    { code: 'timeout' },
  );
});

test('searchTracks builds the query and normalizes hits', async () => {
  let seen;
  const hits = await searchTracks(
    { title: 'A Song', artist: 'A Name', duration: 120, limit: 4 },
    {
      key: KEY,
      online: true,
      fetchImpl: async (url) => {
        seen = url;
        return jsonResponse(200, {
          Body: [
            { id: 'exact-1', title: 'A Song', artists: ['A Name'], tier: 'exact', duration: 120 },
            { id: 'likely-1', title: 'A Song', tier: 'likely' },
            { title: 'No id', tier: 'exact' },
          ],
        });
      },
    },
  );
  assert.match(seen, /\/v1\/tracks\/search\?/);
  assert.match(seen, /title=A\+Song/);
  assert.match(seen, /duration=120/);
  assert.equal(hits.length, 2);
  assert.equal(hits[0].tier, 'exact');
  assert.equal(hits[1].tier, 'likely');
});

test('Retry-After accepts seconds and dates but rejects nonsense', () => {
  assert.equal(parseRetryAfter('30'), 30);
  assert.equal(parseRetryAfter('-5'), null);
  assert.equal(parseRetryAfter('not a date'), null);
});

function memoryFixture() {
  const values = new Map();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, String(value)),
    removeItem: (key) => values.delete(key),
  };
}

test('cache stores, reads and expires entries', () => {
  let now = 1000;
  const cache = new LyricsCache({ storage: memoryFixture(), now: () => now, ttlMs: 1000 });
  cache.set('a', { Body: { Type: 'Static' } });
  assert.deepEqual(cache.get('a'), { Body: { Type: 'Static' } });
  now += 1001;
  assert.equal(cache.get('a'), null);
  assert.equal(cache.size, 0);
});

test('cache evicts least-recently-used entries and can be cleared', () => {
  let now = 1000;
  const cache = new LyricsCache({
    storage: memoryFixture(),
    now: () => now,
    ttlMs: 100000,
    maxEntries: 2,
  });
  cache.set('a', { value: 1 });
  cache.set('b', { value: 2 });
  cache.get('a'); // 'b' is now the least recently used
  cache.set('c', { value: 3 });
  assert.equal(cache.get('a')?.value, 1);
  assert.equal(cache.get('b'), null);
  assert.equal(cache.get('c')?.value, 3);
  cache.clear();
  assert.equal(cache.size, 0);
});

test('cache respects the byte budget', () => {
  const cache = new LyricsCache({
    storage: memoryFixture(),
    ttlMs: 100000,
    maxBytes: 40,
  });
  cache.set('big', { text: 'x'.repeat(500) });
  assert.equal(cache.size, 0);
});
