// Normalizes Spicy Lyrics API payloads into a small, DOM-free model.
// The model keeps the API's text verbatim: punctuation, spelling and spacing
// are never rewritten or invented.

const SYNC_TYPES = new Set(['Syllable', 'Line', 'Static']);
const TIERS = new Set(['exact', 'likely', 'version', 'other_artist', 'related']);

/** Convert API seconds to an integer millisecond value, or null when absent. */
export function secondsToMs(value) {
  const seconds = Number(value);
  if (!Number.isFinite(seconds)) return null;
  return Math.max(0, Math.round(seconds * 1000));
}

/** Format milliseconds as m:ss with tabular-friendly digits. */
export function formatTime(ms) {
  if (!Number.isFinite(ms) || ms < 0) return '--:--';
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = String(totalSeconds % 60).padStart(2, '0');
  return `${minutes}:${seconds}`;
}

/** Only allow http(s) links from API data to reach the DOM. */
export function safeUrl(value) {
  if (typeof value !== 'string') return null;
  const url = value.trim();
  if (!/^https?:\/\/[^\s]+$/i.test(url)) return null;
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:' ? parsed.href : null;
  } catch {
    return null;
  }
}

function validSpan(startMs, endMs) {
  return (
    Number.isFinite(startMs) &&
    Number.isFinite(endMs) &&
    endMs > startMs
  );
}

function textOf(syllable) {
  return typeof syllable?.Text === 'string' ? syllable.Text : '';
}

function normalizeAttribution(body) {
  const source = typeof body?.source === 'string' && body.source ? body.source : 'unknown';
  const person = (value) => {
    if (!value || typeof value !== 'object') return null;
    const username = typeof value.username === 'string' ? value.username : '';
    const url = safeUrl(value.url);
    return username || url ? { username, url } : null;
  };
  return {
    source,
    uploader: person(body?.UploadAttribution?.Uploader),
    maker: person(body?.UploadAttribution?.Maker),
  };
}

/**
 * Group consecutive IsPartOfWord syllables into words and decide where a
 * visible space is needed. The API's own whitespace is preserved: a space is
 * inserted only when neither side of a word boundary already carries one.
 */
function buildWords(syllables, texts) {
  const words = [];
  let previousText = '';

  for (let index = 0; index < syllables.length; index += 1) {
    const raw = syllables[index];
    const segment = {
      text: texts[index],
      startMs: secondsToMs(raw?.StartTime),
      endMs: secondsToMs(raw?.EndTime),
      partOfWord: Boolean(raw?.IsPartOfWord),
      timed: false,
    };
    segment.timed = validSpan(segment.startMs, segment.endMs);

    if (segment.partOfWord && words.length > 0) {
      words[words.length - 1].syllables.push(segment);
    } else {
      const startsWithSpace = /^\s/.test(segment.text);
      const needsSpace =
        words.length > 0 && !/\s$/.test(previousText) && !startsWithSpace;
      words.push({ syllables: [segment], leadingSpace: needsSpace });
    }
    previousText = segment.text;
  }

  return words;
}

function normalizeBackground(entries) {
  return entries.map((entry) => {
    const syllables = Array.isArray(entry?.Syllables) ? entry.Syllables : [];
    const texts = syllables.map(textOf);
    const words = buildWords(syllables, texts);
    const startMs = secondsToMs(entry?.StartTime);
    const endMs = secondsToMs(entry?.EndTime);
    const segments = words.flatMap((word) => word.syllables);
    const timed = segments.length > 0 && segments.every((segment) => segment.timed);
    return {
      text: texts.join(''),
      words,
      segments,
      startMs,
      endMs,
      timed: timed || validSpan(startMs, endMs),
    };
  });
}

function normalizeLine(entry, type) {
  const lead =
    entry?.Lead && typeof entry.Lead === 'object'
      ? entry.Lead
      : entry && typeof entry === 'object' && Array.isArray(entry.Syllables)
        ? entry
        : {};
  const syllables = Array.isArray(lead.Syllables) ? lead.Syllables : [];
  const texts = syllables.map(textOf);
  const text = texts.join('');
  let startMs = secondsToMs(lead.StartTime);
  let endMs = secondsToMs(lead.EndTime);

  let words = [];
  let segments = [];

  if (type === 'Syllable') {
    words = buildWords(syllables, texts);
    segments = words.flatMap((word) => word.syllables);
    // A line is only syllable-timed when every syllable carries a usable span.
    // Mixed or missing timings fall back to line-level fill instead of inventing
    // word timings.
    if (!segments.length || !segments.every((segment) => segment.timed)) {
      for (const segment of segments) segment.timed = false;
    }
  } else {
    segments = [
      {
        text,
        startMs,
        endMs,
        partOfWord: false,
        timed: type === 'Line' && validSpan(startMs, endMs),
      },
    ];
  }

  if (!validSpan(startMs, endMs)) {
    const timed = segments.filter((segment) => segment.timed);
    if (timed.length > 0) {
      startMs = startMs ?? timed[0].startMs;
      endMs = endMs ?? timed[timed.length - 1].endMs;
    }
  }
  if (Number.isFinite(startMs) && Number.isFinite(endMs) && endMs <= startMs) {
    endMs = null;
  }

  const background = Array.isArray(entry?.Background)
    ? normalizeBackground(entry.Background)
    : [];

  return {
    text,
    words,
    segments,
    background,
    startMs,
    endMs,
    timed: Number.isFinite(startMs) && Number.isFinite(endMs),
    isEmpty: text.trim().length === 0,
  };
}

/**
 * Normalize a Spicy Lyrics API response.
 * @param {unknown} payload Full API response ({ Body, Status, Type }).
 * @returns {{id: string, type: string, lines: object[], startMs: number|null,
 *   endMs: number|null, source: string, attribution: object, hasContent: boolean}}
 */
export function normalizeLyrics(payload) {
  if (!payload || typeof payload !== 'object') {
    throw new TypeError('Lyrics response is not an object');
  }
  const body = payload.Body && typeof payload.Body === 'object' ? payload.Body : payload;
  const requestedType = typeof body.Type === 'string' ? body.Type : 'Static';
  const type = SYNC_TYPES.has(requestedType) ? requestedType : 'Static';
  const content = Array.isArray(body.Content) ? body.Content : [];
  const lines = content
    .filter((entry) => entry && typeof entry === 'object')
    .map((entry) => normalizeLine(entry, type));
  const attribution = normalizeAttribution(body);

  return {
    id: typeof body.id === 'string' ? body.id : '',
    type,
    lines,
    startMs: secondsToMs(body.StartTime),
    endMs: secondsToMs(body.EndTime),
    source: attribution.source,
    attribution,
    hasContent: lines.some((line) => !line.isEmpty),
  };
}

/** Human-readable attribution, always shown next to the lyrics. */
export function attributionFor(attribution, { demo = false } = {}) {
  if (demo || attribution?.source === 'demo_fixture') {
    return {
      text: 'Source: Demo fixture · original placeholder text',
      links: [],
    };
  }

  switch (attribution?.source) {
    case 'spicy_lyrics': {
      const links = [];
      const uploaderUrl = safeUrl(attribution.uploader?.url);
      const makerUrl = safeUrl(attribution.maker?.url);
      if (uploaderUrl && attribution.uploader.username) {
        links.push({ label: attribution.uploader.username, url: uploaderUrl });
      }
      if (makerUrl && attribution.maker.username) {
        links.push({ label: attribution.maker.username, url: makerUrl });
      }
      return {
        text: links.length
          ? 'Community sync · via the Spicy Lyrics API · uploader and maker:'
          : 'Community sync · via the Spicy Lyrics API',
        links,
      };
    }
    case 'apple_music':
      return { text: 'Lyrics courtesy of Apple Music · via the Spicy Lyrics API', links: [] };
    case 'spotify':
      return { text: 'Lyrics courtesy of Spotify · via the Spicy Lyrics API', links: [] };
    default:
      return {
        text: `Source: ${attribution?.source || 'unknown'} · via the Spicy Lyrics API`,
        links: [],
      };
  }
}

/** Normalize a single search hit. Tiers that are not exact need an explicit tap. */
export function normalizeSearchResult(item) {
  if (!item || typeof item !== 'object') return null;
  const ids = item.ids && typeof item.ids === 'object' ? item.ids : {};
  const id =
    typeof item.id === 'string' && item.id
      ? item.id
      : typeof ids.spotify === 'string' && ids.spotify
        ? ids.spotify
        : typeof ids.isrc === 'string' && ids.isrc
          ? ids.isrc
          : typeof ids.apple_music === 'string' && ids.apple_music
            ? ids.apple_music
            : '';
  if (!id) return null;

  const artists = Array.isArray(item.artists)
    ? item.artists.map((artist) => String(artist)).join(', ')
    : String(item.artists ?? '');
  const tier = typeof item.tier === 'string' && TIERS.has(item.tier) ? item.tier : 'other';

  return {
    id,
    ids,
    title: String(item.title ?? 'Untitled'),
    artists,
    album: String(item.album ?? ''),
    durationMs: secondsToMs(item.duration),
    artwork: safeUrl(item.artwork),
    tier,
    variants: Array.isArray(item.variants) ? item.variants : [],
  };
}
