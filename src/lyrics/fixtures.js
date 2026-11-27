// Original placeholder fixtures. No real song lyrics, no copyrighted artwork.
// Every fixture uses the public API's response shape so the parser and the
// renderer follow the same path as a real track.

const seconds = (ms) => Number((ms / 1000).toFixed(3));

function syllableSlices(phrase, startMs, endMs) {
  const words = phrase.split(/\s+/).filter(Boolean);
  const span = (endMs - startMs) / Math.max(1, words.length);
  const syllables = [];

  words.forEach((word, index) => {
    const wordStart = startMs + index * span;
    const wordEnd = wordStart + span;
    const trailing = index < words.length - 1 ? ' ' : '';

    if (word.length > 6) {
      const split = Math.ceil(word.length / 2);
      const middle = wordStart + span * 0.55;
      syllables.push({
        Text: word.slice(0, split),
        StartTime: seconds(wordStart),
        EndTime: seconds(middle),
        IsPartOfWord: false,
      });
      syllables.push({
        Text: word.slice(split) + trailing,
        StartTime: seconds(middle),
        EndTime: seconds(wordEnd),
        IsPartOfWord: true,
      });
    } else {
      syllables.push({
        Text: word + trailing,
        StartTime: seconds(wordStart),
        EndTime: seconds(wordEnd),
        IsPartOfWord: false,
      });
    }
  });

  return syllables;
}

function syllableLine(phrase, startMs, endMs, background = []) {
  return {
    Type: 'Vocal',
    Lead: {
      Syllables: syllableSlices(phrase, startMs, endMs),
      StartTime: seconds(startMs),
      EndTime: seconds(endMs),
    },
    Background: background.map(([text, from, to]) => ({
      Syllables: syllableSlices(text, from, to),
      StartTime: seconds(from),
      EndTime: seconds(to),
    })),
  };
}

function plainSyllables(phrase) {
  const words = phrase.split(/\s+/).filter(Boolean);
  return words.map((word, index) => ({
    Text: word + (index < words.length - 1 ? ' ' : ''),
    IsPartOfWord: false,
  }));
}

function lineContent(phrase, startMs, endMs) {
  return {
    Type: 'Vocal',
    Lead: {
      Syllables: plainSyllables(phrase),
      StartTime: seconds(startMs),
      EndTime: seconds(endMs),
    },
  };
}

function staticContent(phrase) {
  return { Type: 'Vocal', Lead: { Syllables: plainSyllables(phrase) } };
}

function envelope({ id, type, lines, startMs, endMs }) {
  return {
    Body: {
      id,
      ids: { spotify: '', isrc: '', apple_music: '' },
      Type: type,
      StartTime: seconds(startMs),
      EndTime: seconds(endMs),
      Content: lines,
      source: 'demo_fixture',
      UploadAttribution: null,
    },
    Status: 200,
    Type: 'object',
  };
}

const SHORT_LINES = [
  'The window holds the morning light',
  'A quiet hum beneath the street',
  'We trace the lines the rain has drawn',
  'And let the city keep its time',
  'Hold on, the amber hour is here',
  'Hold on, the shadows lean away',
  'No map, no signal, only sound',
  'The floorboards know the way we move',
  'So turn the page and leave it open',
  'The quiet part is where we stay',
];

const LINE_DURATION_MS = 3200;
const LINE_GAP_MS = 120;

function timedLines(phrases, { from = 1200, backgroundOn = [] } = {}) {
  return phrases.map((phrase, index) => {
    const start = from + index * (LINE_DURATION_MS + LINE_GAP_MS);
    const end = start + LINE_DURATION_MS;
    const background = backgroundOn.includes(index)
      ? [[`echo of the same refrain`, start + 400, end - 200]]
      : [];
    return syllableLine(phrase, start, end, background);
  });
}

const syllableLines = timedLines(SHORT_LINES, { backgroundOn: [3, 4, 5] });
const lineLines = SHORT_LINES.map((phrase, index) => {
  const start = 1200 + index * (LINE_DURATION_MS + LINE_GAP_MS);
  return lineContent(phrase, start, start + LINE_DURATION_MS);
});

const STATIC_LINES = [
  'No timings here, just words on a quiet page',
  'Read them slowly, let the room settle',
  'This is a placeholder, not a real song',
];

const LONG_PHRASES = [
  'The corridor keeps a second heartbeat',
  'Every doorway holds a different weather',
  'We count the tiles beneath the station clock',
  'And name the shapes the ceiling makes',
  'A paper moon above the sleeping street',
  'The signal flickers, then decides to stay',
  'Nothing here is finished, nothing lost',
  'We keep the windows open for the sound',
  'الضوء في النافذة يبقى هنا',
  'The electromagnetically quiet afternoon',
  'A longer line that wanders past the usual edge of the frame and keeps on going without asking permission from the room',
  'So leave the lamp on low and let it be',
];

const longLines = Array.from({ length: 48 }, (_, index) => {
  const phrase = LONG_PHRASES[index % LONG_PHRASES.length];
  const start = 1000 + index * 3400;
  return syllableLine(phrase, start, start + 3100);
});

const DEMO_META = {
  title: 'Amber Hour',
  artist: 'Demo Signal',
  album: 'Placeholder Sessions',
  artwork: './cover-placeholder.svg',
};

function fixture(payload, meta, startMs) {
  return { payload, meta, startMs };
}

export const FIXTURES = {
  syllable: fixture(
    envelope({
      id: 'demo-syllable',
      type: 'Syllable',
      lines: syllableLines,
      startMs: 1200,
      endMs: 1200 + SHORT_LINES.length * (LINE_DURATION_MS + LINE_GAP_MS),
    }),
    { ...DEMO_META, title: 'Amber Hour', durationMs: 35000 },
    8600,
  ),

  line: fixture(
    envelope({
      id: 'demo-line',
      type: 'Line',
      lines: lineLines,
      startMs: 1200,
      endMs: 1200 + SHORT_LINES.length * (LINE_DURATION_MS + LINE_GAP_MS),
    }),
    { ...DEMO_META, title: 'Slow Return', durationMs: 35000 },
    8600,
  ),

  static: fixture(
    envelope({
      id: 'demo-static',
      type: 'Static',
      lines: STATIC_LINES.map((phrase) => staticContent(phrase)),
      startMs: 0,
      endMs: 0,
    }),
    { title: 'Page Without a Clock', artist: 'Demo Signal', album: 'Placeholder Sessions', artwork: null, durationMs: 30000 },
    0,
  ),

  long: fixture(
    envelope({
      id: 'demo-long',
      type: 'Syllable',
      lines: longLines,
      startMs: 1000,
      endMs: 1000 + 48 * 3400,
    }),
    { ...DEMO_META, title: 'A Very Long Evening', durationMs: 165000 },
    62000,
  ),

  empty: fixture(
    envelope({ id: 'demo-empty', type: 'Syllable', lines: [], startMs: 0, endMs: 0 }),
    DEMO_META,
    0,
  ),
};

/** The error fixture is an error object, not a payload. */
export const FIXTURE_ERRORS = Object.freeze({
  error: Object.freeze({
    code: 'rate_limited',
    status: 429,
    retryAfterMs: 12000,
    message: 'Rate limited, retry in 12 s.',
  }),
});

export const FIXTURE_IDS = Object.freeze([...Object.keys(FIXTURES), 'error']);

export function hasFixture(name) {
  return typeof name === 'string' && (name === 'error' || Object.hasOwn(FIXTURES, name));
}

/** @returns {{payload: object, meta: object, startMs: number}|null} */
export function getFixture(name) {
  if (name === 'error') throw FIXTURE_ERRORS.error;
  return Object.hasOwn(FIXTURES, name) ? FIXTURES[name] : null;
}
