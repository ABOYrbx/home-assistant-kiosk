import test from 'node:test';
import assert from 'node:assert/strict';
import {
  normalizeLyrics,
  normalizeSearchResult,
  attributionFor,
  formatTime,
  secondsToMs,
  safeUrl,
} from './parse.js';
import { getFixture } from './fixtures.js';

test('syllable payload is normalized with grouped words and preserved spacing', () => {
  const payload = {
    Body: {
      Type: 'Syllable',
      StartTime: 1.5,
      EndTime: 4,
      source: 'spicy_lyrics',
      Content: [
        {
          Lead: {
            StartTime: 1.5,
            EndTime: 4,
            Syllables: [
              { Text: 'mor', StartTime: 1.5, EndTime: 2.2, IsPartOfWord: false },
              { Text: 'ning ', StartTime: 2.2, EndTime: 2.8, IsPartOfWord: true },
              { Text: 'light', StartTime: 2.9, EndTime: 4, IsPartOfWord: false },
            ],
          },
        },
      ],
    },
  };

  const result = normalizeLyrics(payload);
  assert.equal(result.type, 'Syllable');
  assert.equal(result.lines.length, 1);
  const line = result.lines[0];
  assert.equal(line.text, 'morning light');
  assert.equal(line.words.length, 2);
  assert.equal(line.words[1].leadingSpace, false); // the API text already has it
  assert.deepEqual(
    line.words[0].syllables.map((segment) => segment.partOfWord),
    [false, true],
  );
  assert.equal(line.words[0].syllables[0].startMs, 1500);
  assert.equal(line.words[0].syllables[1].endMs, 2800);
  assert.equal(line.words[0].syllables[0].timed, true);
  assert.equal(line.timed, true);
  assert.equal(result.startMs, 1500);
});

test('line payload uses line timings and never invents word timings', () => {
  const payload = {
    Body: {
      Type: 'Line',
      Content: [
        {
          Lead: {
            StartTime: 2,
            EndTime: 5,
            Syllables: [{ Text: 'slow ', IsPartOfWord: false }, { Text: 'line' }],
          },
        },
      ],
    },
  };
  const line = normalizeLyrics(payload).lines[0];
  assert.equal(line.segments.length, 1);
  assert.equal(line.segments[0].text, 'slow line');
  assert.equal(line.segments[0].startMs, 2000);
  assert.equal(line.segments[0].endMs, 5000);
  assert.equal(line.segments[0].timed, true);
  assert.equal(line.words.length, 0);
});

test('static payload has no timings and no active line', () => {
  const payload = {
    Body: {
      Type: 'Static',
      Content: [{ Lead: { Syllables: [{ Text: 'plain words' }] } }],
    },
  };
  const result = normalizeLyrics(payload);
  assert.equal(result.type, 'Static');
  assert.equal(result.lines[0].timed, false);
  assert.equal(result.lines[0].segments[0].timed, false);
});

test('zero, negative and missing durations are not treated as timed', () => {
  const payload = {
    Body: {
      Type: 'Syllable',
      Content: [
        {
          Lead: {
            StartTime: 3,
            EndTime: 2.5,
            Syllables: [
              { Text: 'a', StartTime: 1, EndTime: 1, IsPartOfWord: false },
              { Text: 'b', StartTime: 5, EndTime: 4, IsPartOfWord: false },
              { Text: 'c', IsPartOfWord: false },
            ],
          },
        },
      ],
    },
  };
  const line = normalizeLyrics(payload).lines[0];
  assert.equal(line.timed, false);
  assert.ok(line.segments.every((segment) => !segment.timed));
});

test('background vocals keep their own timing', () => {
  const payload = getFixture('syllable').payload;
  const result = normalizeLyrics(payload);
  const withBackground = result.lines.find((line) => line.background.length > 0);
  assert.ok(withBackground);
  assert.equal(withBackground.background[0].timed, true);
  assert.ok(withBackground.background[0].text.length > 0);
});

test('empty fixture reports no content', () => {
  const result = normalizeLyrics(getFixture('empty').payload);
  assert.equal(result.hasContent, false);
});

test('attribution is explicit and unsafe URLs are dropped', () => {
  const spicy = attributionFor({
    source: 'spicy_lyrics',
    uploader: { username: 'demo-uploader', url: 'javascript:alert(1)' },
    maker: { username: 'demo-maker', url: 'https://example.org/maker' },
  });
  assert.equal(spicy.links.length, 1);
  assert.equal(spicy.links[0].label, 'demo-maker');
  assert.match(attributionFor({ source: 'apple_music' }).text, /Apple Music/);
  assert.match(attributionFor({ source: 'spotify' }).text, /Spotify/);
  assert.match(attributionFor({ source: 'demo_fixture' }).text, /Demo fixture/);
});

test('search results keep exact tier and ignore unusable hits', () => {
  const exact = normalizeSearchResult({
    id: 'track-1',
    title: 'Title',
    artists: ['One', 'Two'],
    album: 'Album',
    duration: 123.4,
    artwork: 'https://example.org/art.jpg',
    tier: 'exact',
  });
  assert.equal(exact.tier, 'exact');
  assert.equal(exact.artists, 'One, Two');
  assert.equal(exact.durationMs, 123400);
  assert.equal(normalizeSearchResult({ tier: 'likely' }), null);
  assert.equal(normalizeSearchResult({ id: 'x', artwork: 'javascript:alert(1)' }).artwork, null);
});

test('time and URL helpers are defensive', () => {
  assert.equal(formatTime(0), '0:00');
  assert.equal(formatTime(61500), '1:01');
  assert.equal(formatTime(Number.NaN), '--:--');
  assert.equal(secondsToMs(undefined), null);
  assert.equal(secondsToMs(-1), 0);
  assert.equal(safeUrl('https://example.org/a'), 'https://example.org/a');
  assert.equal(safeUrl('ftp://example.org'), null);
});
