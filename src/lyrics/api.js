// Spicy Lyrics API client. Native fetch only; no runtime dependencies.
// The publishable key is passed in memory and never placed in a URL or a log.

import { normalizeSearchResult, secondsToMs } from './parse.js';

export const API_BASE = 'https://api.spicylyrics.org';
export const DEFAULT_TIMEOUT_MS = 10000;
export const CACHE_KEY = 'lyrics-stage:spicy-key';
export const CACHE_TTL_MS = 24 * 60 * 60 * 1000;
const CACHE_PREFIX = 'lyrics-stage:cache:';
const CACHE_INDEX = 'lyrics-stage:cache:index';

export class LyricsApiError extends Error {
  constructor(code, message, { status = null, retryAfterMs = null } = {}) {
    super(message);
    this.name = 'LyricsApiError';
    this.code = code;
    this.status = status;
    this.retryAfterMs = retryAfterMs;
  }
}

/** Parse Retry-After as seconds or an HTTP date, clamped to a sane range. */
export function parseRetryAfter(value) {
  if (value == null) return null;
  const text = String(value).trim();
  if (/^\d+$/.test(text)) {
    return clampSeconds(Number(text));
  }
  const date = Date.parse(text);
  if (!Number.isFinite(date)) return null;
  return clampSeconds((date - Date.now()) / 1000);
}

function clampSeconds(seconds) {
  if (!Number.isFinite(seconds) || seconds < 0) return null;
  return Math.min(3600, Math.ceil(seconds));
}

export function isPublishableKey(value) {
  return typeof value === 'string' && /^sl_pk_[A-Za-z0-9_-]+$/.test(value.trim());
}

function isOnline(override) {
  if (typeof override === 'boolean') return override;
  if (typeof navigator !== 'undefined' && 'onLine' in navigator) return navigator.onLine !== false;
  return true;
}

function responseHeader(response, name) {
  if (response?.headers?.get) {
    return (
      response.headers.get(name) ??
      response.headers.get(name.toLowerCase()) ??
      response.headers.get(name.toUpperCase()) ??
      null
    );
  }
  if (response?.headers && typeof response.headers === 'object') {
    return response.headers[name] ?? response.headers[name.toLowerCase()] ?? null;
  }
  return null;
}

async function request(path, { key, signal, timeoutMs, fetchImpl, online }) {
  if (!isOnline(online)) {
    throw new LyricsApiError('offline', 'Offline — lyrics unavailable. Reconnect and retry.');
  }
  if (!isPublishableKey(key)) {
    throw new LyricsApiError(
      'missing_key',
      'Add a publishable Spicy Lyrics key in Settings to load real lyrics.',
    );
  }

  const controller = new AbortController();
  const onAbort = () => controller.abort(signal.reason);
  if (signal) signal.addEventListener('abort', onAbort, { once: true });
  const timeoutId = setTimeout(
    () => controller.abort(new Error('request timeout')),
    timeoutMs,
  );

  let response;
  try {
    response = await fetchImpl(`${API_BASE}${path}`, {
      method: 'GET',
      headers: { Authorization: `Bearer ${key.trim()}`, Accept: 'application/json' },
      signal: controller.signal,
    });
  } catch (error) {
    if (signal?.aborted) throw new LyricsApiError('cancelled', 'Request cancelled.');
    if (controller.signal.aborted && !isOnline(online)) {
      throw new LyricsApiError('offline', 'Offline — lyrics unavailable. Reconnect and retry.');
    }
    if (controller.signal.aborted) {
      throw new LyricsApiError('timeout', 'The lyrics request timed out. Retry.');
    }
    throw new LyricsApiError('offline', 'Network request failed. Check the connection and retry.', {
      cause: error,
    });
  } finally {
    clearTimeout(timeoutId);
    if (signal) signal.removeEventListener('abort', onAbort);
  }

  if (!response?.ok) {
    const status = response?.status ?? 0;
    const retryAfter = parseRetryAfter(responseHeader(response, 'Retry-After')) ?? null;
    switch (status) {
      case 401:
        throw new LyricsApiError(
          'unauthorized',
          'Key missing or not accepted. Check the publishable key in Settings.',
          { status },
        );
      case 403:
        throw new LyricsApiError(
          'forbidden',
          'Origin not allowed for this key. Add this origin in the developer dashboard.',
          { status },
        );
      case 404:
        throw new LyricsApiError('not_found', 'No lyrics for this track.', { status });
      case 429: {
        const seconds = retryAfter ?? 1;
        throw new LyricsApiError('rate_limited', `Rate limited, retry in ${seconds} s.`, {
          status,
          retryAfterMs: seconds * 1000,
        });
      }
      default:
        throw new LyricsApiError(
          'server',
          `The lyrics service returned ${status || 'an unknown error'}. Retry.`,
          { status },
        );
    }
  }

  try {
    return await response.json();
  } catch {
    throw new LyricsApiError('invalid_response', 'The lyrics service returned an unreadable response.');
  }
}

/** GET /v1/lyrics/{id}. Returns the raw payload for the parser. */
export async function getLyrics(id, options = {}) {
  const {
    key,
    fetchImpl = globalThis.fetch,
    signal,
    timeoutMs = DEFAULT_TIMEOUT_MS,
    online,
  } = options;
  const trackId = encodeURIComponent(String(id ?? '').trim());
  if (!trackId) throw new LyricsApiError('invalid_id', 'No track id was provided.');
  return request(`/v1/lyrics/${trackId}`, { key, fetchImpl, signal, timeoutMs, online });
}

/** GET /v1/tracks/search. Returns normalized hits. */
export async function searchTracks(params = {}, options = {}) {
  const {
    key,
    fetchImpl = globalThis.fetch,
    signal,
    timeoutMs = DEFAULT_TIMEOUT_MS,
    online,
  } = options;
  const query = new URLSearchParams();
  for (const field of ['title', 'artist', 'album', 'duration', 'limit']) {
    const value = params[field];
    if (value !== undefined && value !== null && String(value).trim() !== '') {
      query.set(field, String(value).trim());
    }
  }
  const payload = await request(`/v1/tracks/search?${query.toString()}`, {
    key,
    fetchImpl,
    signal,
    timeoutMs,
    online,
  });

  const candidates = Array.isArray(payload)
    ? payload
    : Array.isArray(payload?.Body)
      ? payload.Body
      : Array.isArray(payload?.tracks)
        ? payload.tracks
        : Array.isArray(payload?.Results)
          ? payload.Results
          : [];
  return candidates
    .map(normalizeSearchResult)
    .filter(Boolean);
}

/** Map any caught error to a short, actionable message for the stage. */
export function errorMessage(error) {
  if (error && typeof error === 'object' && typeof error.code === 'string') {
    if (error.code === 'rate_limited') {
      const seconds = Math.max(1, Math.ceil((error.retryAfterMs ?? 1000) / 1000));
      return `Rate limited, retry in ${seconds} s.`;
    }
    if (typeof error.message === 'string' && error.message) return error.message;
  }
  return 'Something went wrong. Retry, or choose a demo fixture.';
}

// --- Cache: memory + localStorage, TTL 24 h, LRU, 30 entries / 1 MB. --------

function memoryStorage() {
  const values = new Map();
  return {
    getItem: (key) => (values.has(key) ? values.get(key) : null),
    setItem: (key, value) => values.set(key, String(value)),
    removeItem: (key) => values.delete(key),
  };
}

function defaultStorage() {
  try {
    if (typeof localStorage !== 'undefined') {
      const probe = 'lyrics-stage:probe';
      localStorage.setItem(probe, '1');
      localStorage.removeItem(probe);
      return localStorage;
    }
  } catch {
    // Private browsing or a full quota: fall back to memory only.
  }
  return memoryStorage();
}

export class LyricsCache {
  constructor({
    storage = defaultStorage(),
    ttlMs = CACHE_TTL_MS,
    maxEntries = 30,
    maxBytes = 1024 * 1024,
    now = () => Date.now(),
  } = {}) {
    this.storage = storage;
    this.ttlMs = ttlMs;
    this.maxEntries = maxEntries;
    this.maxBytes = maxBytes;
    this.now = now;
    this.encoder = new TextEncoder();
    this.entries = this._loadIndex();
    this._evict();
  }

  _loadIndex() {
    try {
      const raw = this.storage.getItem(CACHE_INDEX);
      const parsed = raw ? JSON.parse(raw) : [];
      if (!Array.isArray(parsed)) return new Map();
      const entries = new Map();
      for (const item of parsed) {
        if (
          item &&
          typeof item.id === 'string' &&
          Number.isFinite(item.savedAt) &&
          Number.isFinite(item.bytes)
        ) {
          entries.set(item.id, {
            savedAt: item.savedAt,
            bytes: item.bytes,
          });
        }
      }
      return entries;
    } catch {
      return new Map();
    }
  }

  _persistIndex() {
    try {
      const list = [...this.entries.entries()].map(([id, entry]) => ({ id, ...entry }));
      this.storage.setItem(CACHE_INDEX, JSON.stringify(list));
    } catch {
      // A failed write must never break the stage.
    }
  }

  _key(id) {
    return `${CACHE_PREFIX}${encodeURIComponent(id)}`;
  }

  _fresh(entry) {
    return this.now() - entry.savedAt < this.ttlMs;
  }

  _evict() {
    const expired = [...this.entries.entries()].filter(([, entry]) => !this._fresh(entry));
    for (const [id] of expired) {
      this.entries.delete(id);
      try {
        this.storage.removeItem(this._key(id));
      } catch {
        // Ignore storage failures.
      }
    }

    while (this.entries.size > this.maxEntries || this._totalBytes() > this.maxBytes) {
      const oldestId = this.entries.keys().next().value;
      if (oldestId === undefined) break;
      this.entries.delete(oldestId);
      try {
        this.storage.removeItem(this._key(oldestId));
      } catch {
        // Ignore storage failures.
      }
    }
    this._persistIndex();
  }

  _totalBytes() {
    let total = 0;
    for (const entry of this.entries.values()) total += entry.bytes;
    return total;
  }

  get(id) {
    const entry = this.entries.get(id);
    if (!entry) return null;
    if (!this._fresh(entry)) {
      this.entries.delete(id);
      try {
        this.storage.removeItem(this._key(id));
      } catch {
        // Ignore storage failures.
      }
      this._persistIndex();
      return null;
    }
    // Move to the end so the entry is last to be evicted.
    this.entries.delete(id);
    this.entries.set(id, entry);
    this._persistIndex();
    try {
      const raw = this.storage.getItem(this._key(id));
      return raw ? JSON.parse(raw) : null;
    } catch {
      return null;
    }
  }

  set(id, payload) {
    let serialized;
    try {
      serialized = JSON.stringify(payload);
    } catch {
      return;
    }
    const bytes = this.encoder.encode(serialized).length;
    if (bytes > this.maxBytes) return;

    try {
      this.storage.setItem(this._key(id), serialized);
    } catch {
      return;
    }
    this.entries.delete(id);
    this.entries.set(id, { savedAt: this.now(), bytes });
    this._evict();
  }

  clear() {
    for (const id of this.entries.keys()) {
      try {
        this.storage.removeItem(this._key(id));
      } catch {
        // Ignore storage failures.
      }
    }
    this.entries.clear();
    this._persistIndex();
  }

  get size() {
    return this.entries.size;
  }
}

/** Persist the publishable key without ever logging or echoing it. */
export const keyStore = {
  read() {
    try {
      return typeof localStorage !== 'undefined' ? localStorage.getItem(CACHE_KEY) ?? '' : '';
    } catch {
      return '';
    }
  },
  write(key) {
    try {
      if (typeof localStorage === 'undefined') return false;
      if (key) localStorage.setItem(CACHE_KEY, key.trim());
      else localStorage.removeItem(CACHE_KEY);
      return true;
    } catch {
      return false;
    }
  },
};

export const SEARCH_DEFAULTS = Object.freeze({ limit: 8, duration: secondsToMs(0) });
