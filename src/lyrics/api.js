// Spicy Lyrics API client and a bounded, time-limited cache.
// The publishable key is sent only as a Bearer header to the fixed API origin;
// it is never placed in a URL, a log line, or an exception message.

import { parseLyrics } from './parse.js';

export const SPICY_BASE_URL = 'https://api.spicylyrics.org';
const CACHE_KEY = 'lyrics-stage:cache:v1';
const CACHE_TTL_MS = 24 * 60 * 60 * 1000;
const CACHE_MAX_ENTRIES = 30;
const CACHE_MAX_BYTES = 1024 * 1024;

export class LyricsApiError extends Error {
  constructor(kind, message, { status = 0, retryAfterMs = 0 } = {}) {
    super(message);
    this.name = 'LyricsApiError';
    this.kind = kind;
    this.status = status;
    this.retryAfterMs = retryAfterMs;
  }
}

export function mapHttpError(status, retryAfterHeader = null) {
  const seconds = Number(retryAfterHeader);
  const retryAfterMs = Number.isFinite(seconds) && seconds > 0 ? seconds * 1000 : 0;
  switch (status) {
    case 400:
      return new LyricsApiError('invalid_request', 'Lyrics request was rejected.', { status });
    case 401:
      return new LyricsApiError('unauthorized', 'Key missing or invalid.', { status });
    case 403:
      return new LyricsApiError('forbidden', 'Key missing or origin not allowed.', { status });
    case 404:
      return new LyricsApiError('not_found', 'No lyrics for this track.', { status });
    case 429:
      return new LyricsApiError(
        'rate_limited',
        retryAfterMs > 0 ? `Rate limited, retry in ${Math.ceil(retryAfterMs / 1000)} s.`
          : 'Rate limited. Try again shortly.',
        { status, retryAfterMs },
      );
    default:
      if (status >= 500) {
        return new LyricsApiError('server', 'Lyrics service unavailable.', { status });
      }
      return new LyricsApiError('request_failed', `Lyrics request failed (${status}).`, { status });
  }
}

function safeStorage(storage) {
  if (!storage) return null;
  try {
    const probe = '__lyrics_probe__';
    storage.setItem(probe, '1');
    storage.removeItem(probe);
    return storage;
  } catch {
    return null;
  }
}

/** LRU cache: at most 30 tracks / 1 MB, 24 h TTL, memory plus localStorage. */
export class LyricsCache {
  constructor({ storage, key = CACHE_KEY, now = () => Date.now() } = {}) {
    this.storage = safeStorage(storage ?? (typeof localStorage === 'undefined' ? null : localStorage));
    this.key = key;
    this.now = now;
    this.entries = new Map();
    this._load();
  }

  _load() {
    if (!this.storage) return;
    try {
      const raw = this.storage.getItem(this.key);
      if (!raw) return;
      const data = JSON.parse(raw);
      if (data?.v !== 1 || !data.entries || typeof data.entries !== 'object') return;
      const now = this.now();
      for (const [id, entry] of Object.entries(data.entries)) {
        if (entry?.p && Number.isFinite(entry.t) && entry.t > now) {
          this.entries.set(id, { t: entry.t, p: entry.p });
        }
      }
      this._prune();
    } catch {
      this.entries.clear();
    }
  }

  _save() {
    if (!this.storage) return;
    try {
      const entries = Object.fromEntries(this.entries);
      const raw = JSON.stringify({ v: 1, entries });
      if (new TextEncoder().encode(raw).length > CACHE_MAX_BYTES) {
        this.entries = new Map([...this.entries].slice(0, 5));
        this.storage.setItem(this.key, JSON.stringify({ v: 1, entries: Object.fromEntries(this.entries) }));
        return;
      }
      this.storage.setItem(this.key, raw);
    } catch {
      // Storage can be full or disabled; the in-memory cache still works.
    }
  }

  _prune() {
    const now = this.now();
    for (const [id, entry] of this.entries) {
      if (entry.t <= now) this.entries.delete(id);
    }
    // Map iteration order is least-recently-used first.
    while (this.entries.size > CACHE_MAX_ENTRIES) {
      this.entries.delete(this.entries.keys().next().value);
    }
    this._save();
  }

  get(id) {
    if (typeof id !== 'string' || !this.entries.has(id)) return null;
    const entry = this.entries.get(id);
    if (entry.t <= this.now()) {
      this.entries.delete(id);
      return null;
    }
    // Refresh LRU order.
    this.entries.delete(id);
    this.entries.set(id, entry);
    return entry.p;
  }

  set(id, payload) {
    if (typeof id !== 'string' || id === '') return;
    this.entries.delete(id);
    this.entries.set(id, { t: this.now() + CACHE_TTL_MS, p: payload });
    this._prune();
  }

  clear() {
    this.entries.clear();
    if (this.storage) {
      try { this.storage.removeItem(this.key); } catch { /* ignore */ }
    }
  }
}

function combineSignal(timeoutController, externalSignal) {
  if (!externalSignal) return timeoutController.signal;
  if (typeof AbortSignal !== 'undefined' && typeof AbortSignal.any === 'function') {
    return AbortSignal.any([timeoutController.signal, externalSignal]);
  }
  externalSignal.addEventListener('abort', () => timeoutController.abort(externalSignal.reason), { once: true });
  return timeoutController.signal;
}

export class SpicyClient {
  constructor({
    key = '',
    baseUrl = SPICY_BASE_URL,
    fetchImpl = (...args) => fetch(...args),
    timeoutMs = 8000,
    cache,
  } = {}) {
    this.key = typeof key === 'string' ? key.trim() : '';
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.fetchImpl = fetchImpl;
    this.timeoutMs = timeoutMs;
    this.cache = cache ?? new LyricsCache();
  }

  get hasKey() {
    return /^sl_pk_[A-Za-z0-9_-]+$/.test(this.key);
  }

  async _fetchJson(path, { signal } = {}) {
    if (!this.hasKey) {
      throw new LyricsApiError('missing_key', 'Add a publishable Spicy Lyrics key (sl_pk_...).');
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(new Error('timeout')), this.timeoutMs);
    const activeSignal = combineSignal(controller, signal);
    let response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}${path}`, {
        method: 'GET',
        headers: { Authorization: `Bearer ${this.key}` },
        signal: activeSignal,
      });
    } catch (error) {
      if (controller.signal.aborted && !signal?.aborted) {
        throw new LyricsApiError('timeout', 'Lyrics request timed out. Try again.');
      }
      if (signal?.aborted) throw error;
      throw new LyricsApiError('network', 'Offline - lyrics unavailable.');
    } finally {
      clearTimeout(timer);
    }
    if (!response.ok) {
      throw mapHttpError(response.status, response.headers?.get?.('Retry-After'));
    }
    try {
      return await response.json();
    } catch {
      throw new LyricsApiError('malformed', 'Lyrics response could not be read.');
    }
  }

  /**
   * Fetch and normalize lyrics for one track id. Only called on track change;
   * the cache prevents repeat requests for the same track.
   */
  async getLyrics(id, { signal, useCache = true } = {}) {
    if (typeof id !== 'string' || id === '') {
      throw new LyricsApiError('invalid_request', 'No track id available for lyrics.');
    }
    if (useCache) {
      const cached = this.cache.get(id);
      if (cached) return cached;
    }
    const payload = await this._fetchJson(`/v1/lyrics/${encodeURIComponent(id)}`, { signal });
    const model = parseLyrics(payload);
    if (!model) throw new LyricsApiError('not_found', 'No synced lyrics for this track.', { status: 404 });
    this.cache.set(id, model);
    return model;
  }

  async searchTracks({ title = '', artist = '', album = '', duration = 0 } = {}, { signal } = {}) {
    const query = new URLSearchParams();
    if (title) query.set('title', title);
    if (artist) query.set('artist', artist);
    if (album) query.set('album', album);
    if (Number.isFinite(duration) && duration > 0) query.set('duration', String(Math.round(duration)));
    query.set('limit', '5');
    const payload = await this._fetchJson(`/v1/tracks/search?${query.toString()}`, { signal });
    const results = payload?.Body?.results;
    return Array.isArray(results) ? results : [];
  }

  clearCache() {
    this.cache.clear();
  }
}
