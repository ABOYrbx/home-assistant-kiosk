import test from 'node:test';
import assert from 'node:assert/strict';
import {
  parseLyrics, lyricsAt, formatTime, parseSpotifyId, isExactResult, attributionFor,
} from './parse.js';

function syllablePayload() {
  return {
    Body: {
      Type: 'Syllable',
      StartTime: 1,
      EndTime: 6,
      source: 'spicy_lyrics',
      UploadAttribution: {
        Uploader: { username: 'uploader', url: 'https://example.org/u/1' },
        Maker: { username: 'maker', url: 'https://example.org/u/2' },
      },
      Content: [
        {
          StartTime: 1, EndTime: 3,
          Lead: {
            Syllables: [
              { Text: 'Hel', StartTime: 1, EndTime: 1.5, IsPartOfWord: false },
              { Text: 'lo', StartTime: 1.5, EndTime: 2, IsPartOfWord: true },
              { Text: ' ', StartTime: 2, EndTime: 2.1, IsPartOfWord: true },
              { Text: 'world', StartTime: 2.1, EndTime: 3, IsPartOfWord: false },
            ],
          },
          Background: [{ Syllables: [{ Text: 'oh', StartTime: 2, EndTime: 3, IsPartOfWord: false }] }],
        },
        {
          StartTime: 4, EndTime: 6,
          Lead: { Syllables: [{ Text: 'again', StartTime: 4, EndTime: 6, IsPartOfWord: false }] },
        },
      ],
    },
    Status: 200,
  };
}

test('parseLyrics normalizes syllable data and groups words', () => {
  const model = parseLyrics(syllablePayload());
  assert.equal(model.type, 'Syllable');
  assert.equal(model.startMs, 1000);
  assert.equal(model.endMs, 6000);
  assert.equal(model.lines.length, 2);
  assert.equal(model.lines[0].text, 'Hello world');
  assert.deepEqual(model.lines[0].syllables.map((s) => s.wordIndex), [0, 0, 0, 1]);
  assert.equal(model.background.length, 1);
  assert.equal(model.background[0].background, true);
});

test('lyricsAt follows the active syllable with defensive timing', () => {
  const model = parseLyrics(syllablePayload());
  assert.equal(lyricsAt(model, 500).index, -1);
  const first = lyricsAt(model, 1250);
  assert.equal(first.index, 0);
  assert.equal(first.syllableIndex, 0);
  assert.ok(first.fill > 0.4 && first.fill < 0.6);
  const second = lyricsAt(model, 2250);
  assert.equal(second.syllableIndex, 3);
  assert.equal(lyricsAt(model, 5000).index, 1);
  assert.equal(lyricsAt(model, 9000).index, 1);
});

test('line and static lyrics do not invent word timings', () => {
  const line = parseLyrics({
    Body: {
      Type: 'Line', source: 'spotify',
      Content: [{ Text: 'one line', StartTime: 1, EndTime: 2 }, { Text: 'next', StartTime: 2, EndTime: 4 }],
    },
  });
  assert.equal(line.type, 'Line');
  assert.equal(line.lines[0].syllables.length, 0);
  const at = lyricsAt(line, 1500);
  assert.equal(at.type, 'Line');
  assert.equal(at.fill, 0.5);
  assert.equal(at.syllableIndex, -1);

  const stat = parseLyrics({ Body: { Type: 'Static', source: 'apple_music', Content: [{ Text: 'plain' }] } });
  assert.equal(lyricsAt(stat, 1500).static, true);
  assert.equal(lyricsAt(stat, 1500).index, -1);
  assert.equal(parseLyrics({ Body: { Content: [] } }), null);
});

test('zero and negative durations stay defensive', () => {
  const model = parseLyrics({
    Body: {
      Type: 'Syllable', source: 'spicy_lyrics',
      Content: [{
        Lead: { Syllables: [{ Text: 'bad', StartTime: 2, EndTime: 1 }, { Text: '', StartTime: -1, EndTime: -1 }] },
      }],
    },
  });
  assert.equal(model.lines[0].syllables[0].endMs, 2000);
  assert.equal(model.lines[0].syllables[0].startMs, 2000);
  assert.equal(model.lines[0].syllables.length, 1);
  assert.equal(lyricsAt(model, 2000).fill, 1);
});

test('formatTime and Spotify id parsing', () => {
  assert.equal(formatTime(0), '0:00');
  assert.equal(formatTime(65000), '1:05');
  assert.equal(formatTime(-1000), '0:00');
  const id = '4PTG3Z6ehGkBFwjybzWkR8';
  assert.equal(parseSpotifyId(`spotify://track/${id}`), id);
  assert.equal(parseSpotifyId(`spotify:track:${id}`), id);
  assert.equal(parseSpotifyId(`https://open.spotify.com/track/${id}?si=abc`), id);
  assert.equal(parseSpotifyId(id), id);
  assert.equal(parseSpotifyId('library://track/12'), null);
  assert.equal(parseSpotifyId(''), null);
});

test('only exact tiers with matching metadata are auto-selected', () => {
  const track = { title: 'Song', artist: 'Artist', durationMs: 180000 };
  assert.equal(isExactResult({ tier: 'exact', title: 'Song', artists: ['Artist'], duration: 180 }, track), true);
  assert.equal(isExactResult({ tier: 'likely', title: 'Song', artists: ['Artist'], duration: 180 }, track), false);
  assert.equal(isExactResult({ tier: 'exact', title: 'Other', artists: ['Artist'], duration: 180 }, track), false);
  assert.equal(isExactResult({ tier: 'exact', title: 'Song', artists: ['Artist'], duration: 150 }, track), false);
});

test('attribution is conditional on the source', () => {
  const community = attributionFor(parseLyrics(syllablePayload()));
  assert.equal(community.community, true);
  assert.equal(community.providerLabel, 'Lyrics from Spicy Lyrics');
  assert.match(community.text, /Spicy Lyrics/);
  assert.match(community.text, /uploader/);
  assert.match(community.text, /maker/);
  assert.equal(community.uploader.url, 'https://example.org/u/1');
  const commercial = attributionFor({ source: 'spotify', lines: [] });
  assert.equal(commercial.community, false);
  assert.equal(commercial.uploader, null);
  assert.equal(commercial.text, 'Lyrics from spotify');
});
