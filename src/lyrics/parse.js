// Pure, DOM-free helpers shared by the lyrics prototype and the live HA card.
// The Spicy API uses seconds and PascalCase payload keys; everything here is
// normalized to milliseconds and camelCase before it reaches the renderer.

const SPOTIFY_ID = /^[A-Za-z0-9]{22}$/;

export const SYNC_TYPES = Object.freeze(['Syllable', 'Line', 'Static']);

function finite(value, fallback = 0) {
  const n = typeof value === 'number' ? value : Number(value);
  return Number.isFinite(n) ? n : fallback;
}

export function toMs(seconds) {
  const n = finite(seconds, 0);
  return n > 0 ? n * 1000 : 0;
}

function normalizeType(value) {
  return SYNC_TYPES.find((t) => t.toLowerCase() === String(value ?? '').toLowerCase()) ?? 'Static';
}

function cleanText(value) {
  return typeof value === 'string' ? value : '';
}

/** Parse one `Syllables[]` array, keeping zero/negative durations defensive. */
export function parseSyllables(value) {
  if (!Array.isArray(value)) return [];
  let word = -1;
  return value.map((raw) => {
    const item = raw && typeof raw === 'object' ? raw : {};
    const isPartOfWord = item.IsPartOfWord === true;
    if (!isPartOfWord) word += 1;
    const startMs = toMs(item.StartTime);
    const endMs = Math.max(startMs, toMs(item.EndTime));
    return {
      text: cleanText(item.Text),
      startMs,
      endMs,
      isPartOfWord,
      wordIndex: word,
    };
  }).filter((s) => s.text.length > 0 || s.endMs > s.startMs);
}

function lineBounds(entry, syllables) {
  let startMs = toMs(entry.StartTime);
  let endMs = toMs(entry.EndTime);
  for (const s of syllables) {
    if (startMs === 0 || s.startMs < startMs) startMs = s.startMs;
    if (s.endMs > endMs) endMs = s.endMs;
  }
  return { startMs, endMs: Math.max(startMs, endMs) };
}

function parseContentLine(entry, { background = false } = {}) {
  const item = entry && typeof entry === 'object' ? entry : {};
  const syllables = parseSyllables(item.Syllables ?? item.Lead?.Syllables);
  const { startMs, endMs } = lineBounds(item, syllables);
  const text = cleanText(item.Text ?? item.Lead?.Text) ||
    syllables.map((s) => s.text).join('');
  return {
    text,
    startMs,
    endMs,
    syllables,
    background,
    transliteratedText: cleanText(item.TransliteratedText),
    translatedText: cleanText(item.TranslatedText),
  };
}

/**
 * Normalize a `GET /v1/lyrics/{id}` response body into the single model the
 * renderer understands. Returns null when there is no usable content.
 */
export function parseLyrics(payload) {
  const body = payload?.Body ?? payload;
  if (!body || typeof body !== 'object') return null;
  const content = Array.isArray(body.Content) ? body.Content : [];
  if (content.length === 0) return null;

  const type = normalizeType(body.Type);
  const lines = content
    .map((entry) => parseContentLine(entry))
    .filter((line) => line.text.length > 0 || line.syllables.length > 0);
  if (lines.length === 0) return null;

  // Background vocals are rendered smaller and dimmer; drop them when they
  // carry no timing, because inventing one would be dishonest.
  const background = [];
  for (const entry of content) {
    const raw = entry && typeof entry === 'object' ? entry : {};
    if (!Array.isArray(raw.Background) || raw.Background.length === 0) continue;
    for (const item of raw.Background) {
      const line = parseContentLine(item, { background: true });
      if (line.syllables.length > 0 || line.endMs > line.startMs) background.push(line);
    }
  }

  return {
    type,
    startMs: toMs(body.StartTime),
    endMs: toMs(body.EndTime),
    source: cleanText(body.source) || 'unknown',
    ids: body.ids && typeof body.ids === 'object' ? body.ids : {},
    attribution: body.UploadAttribution ?? null,
    songwriters: Array.isArray(body.SongWriters) ? body.SongWriters.filter(cleanText) : [],
    lines,
    background,
  };
}

function findActiveLineIndex(lines, positionMs) {
  let found = -1;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (line.startMs <= positionMs && positionMs < line.endMs) return i;
    if (line.startMs <= positionMs) found = i;
  }
  return found;
}

function fillRatio(positionMs, startMs, endMs) {
  const span = endMs - startMs;
  if (span <= 0) return positionMs >= startMs ? 1 : 0;
  return Math.min(1, Math.max(0, (positionMs - startMs) / span));
}

/**
 * Resolve what the renderer should show at `positionMs` (already offset).
 * Never invents syllable timing for line/static data.
 */
export function lyricsAt(model, positionMs) {
  if (!model) return { type: 'Static', index: -1, line: null, fill: 0, syllableIndex: -1, static: true };
  const pos = finite(positionMs, 0);
  if (model.type === 'Static') {
    return { type: 'Static', index: -1, line: null, fill: 0, syllableIndex: -1, static: true };
  }
  const index = findActiveLineIndex(model.lines, pos);
  const line = index >= 0 ? model.lines[index] : null;
  if (!line) return { type: model.type, index: -1, line: null, fill: 0, syllableIndex: -1, static: false };
  if (model.type === 'Syllable') {
    let syllableIndex = -1;
    for (let i = 0; i < line.syllables.length; i += 1) {
      const s = line.syllables[i];
      if (pos >= s.startMs && pos < s.endMs) { syllableIndex = i; break; }
      if (pos >= s.endMs) syllableIndex = i;
    }
    const s = syllableIndex >= 0 ? line.syllables[syllableIndex] : null;
    return {
      type: model.type,
      index,
      line,
      syllableIndex,
      fill: s ? fillRatio(pos, s.startMs, s.endMs) : 0,
      static: false,
    };
  }
  return {
    type: model.type,
    index,
    line,
    syllableIndex: -1,
    fill: fillRatio(pos, line.startMs, line.endMs),
    static: false,
  };
}

export function formatTime(ms) {
  const total = Math.max(0, Math.floor(finite(ms, 0) / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = String(total % 60).padStart(2, '0');
  return `${minutes}:${seconds}`;
}

/** Extract a Spotify track id from the URI shapes Music Assistant produces. */
export function parseSpotifyId(uri) {
  if (typeof uri !== 'string' || uri.length === 0) return null;
  const value = uri.trim();
  const patterns = [
    /^spotify:\/\/track\/([A-Za-z0-9]{22})(?:\?.*)?$/i,
    /^spotify:track:([A-Za-z0-9]{22})$/i,
    /^https?:\/\/open\.spotify\.com\/track\/([A-Za-z0-9]{22})(?:[/?#].*)?$/i,
  ];
  for (const pattern of patterns) {
    const match = value.match(pattern);
    if (match) return match[1];
  }
  if (SPOTIFY_ID.test(value)) return value;
  return null;
}

function sameText(a, b) {
  if (!a || !b) return true; // unknown on one side cannot disprove a match
  return a.trim().toLocaleLowerCase() === b.trim().toLocaleLowerCase();
}

/** Only tier "exact" may be auto-selected; metadata guards a mislabeled tier. */
export function isExactResult(result, track) {
  if (!result || result.tier !== 'exact') return false;
  if (!sameText(result.title, track?.title)) return false;
  const artists = Array.isArray(result.artists) ? result.artists.join(', ') : '';
  if (!sameText(artists, track?.artist)) return false;
  const a = Number(result.duration);
  const b = Number(track?.durationMs) / 1000;
  if (Number.isFinite(a) && Number.isFinite(b) && b > 0 && Math.abs(a - b) > 2) return false;
  return true;
}

export function attributionFor(model) {
  const source = model?.source || 'unknown';
  const allowed = (value) => typeof value === 'string' && /^https?:\/\//i.test(value) ? value : null;
  const attribution = model?.attribution && typeof model.attribution === 'object'
    ? model.attribution : {};
  const uploader = attribution.Uploader ?? null;
  const maker = attribution.Maker ?? null;
  const community = source === 'spicy_lyrics';
  const parts = [`Lyrics from ${source === 'spicy_lyrics' ? 'Spicy Lyrics' : source}`];
  if (community && uploader?.username) parts.push(`uploaded by ${uploader.username}`);
  if (community && maker?.username) parts.push(`made by ${maker.username}`);
  const providerLabel = parts[0];
  return {
    source,
    community,
    providerLabel,
    text: `${parts.join(' \u00b7 ')}`,
    uploader: community && uploader ? { name: uploader.username || '', url: allowed(uploader.url) } : null,
    maker: community && maker ? { name: maker.username || '', url: allowed(maker.url) } : null,
  };
}
