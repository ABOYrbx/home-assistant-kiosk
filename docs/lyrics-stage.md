# Lyrics stage design prototype

A standalone, fullscreen lyrics stage for reviewing a Spicy Lyrics-style
experience in the browser. It is **not** the production kiosk, not a Music
Assistant integration, and not a Home Assistant card. Nothing here changes the
Rust application, the Slint UI, or the implementation plan.

## Run it

The page uses the existing Vite setup on port 5914:

```bash
npm install
npm run dev
# open http://localhost:5914/lyrics.html
npm run build   # builds both index.html and lyrics.html
npm test        # node --test src/lyrics/*.test.js
python3 scripts/check-docs.py
```

The Overview at `/index.html` is unchanged. Open `/lyrics.html` directly; it is
not linked from the Overview.

## Review links

| Query | Effect |
| --- | --- |
| `?fixture=syllable` | Word-level fill, background vocals, default view |
| `?fixture=line` | Line-level fill; no invented word timings |
| `?fixture=static` | Plain text, "Not synced" note, no active line |
| `?fixture=long` | Many lines, long words, a long line and an RTL line |
| `?fixture=empty` | No lyrics state |
| `?fixture=error` | Rate-limit error with a retry action |
| `?track=<spotify-id-or-isrc>` | Load a real track through the API |
| `?cover=light` / `?cover=dark` | Contrast test with a light or dark cover |

All fixtures are original placeholder text with a locally authored SVG cover.
No real song lyrics, copyrighted artwork, or production data is committed.

## Key and origin setup

The API needs a **publishable** key (`sl_pk_…`). A secret (`sl_sk_…`) key must
never be used in a browser.

1. Create an application in the Spicy Lyrics developer dashboard and copy the
   publishable key.
2. Add the browser origins to the key's allowlist: `http://localhost:5914` for
   local development and the exact LAN or preview origin used for review.
3. Open Settings (gear), paste the key, and choose **Save key**.

The key is stored only in `localStorage` under `lyrics-stage:spicy-key`. It is
never logged, never placed in a URL, and never committed. **Clear key** removes
it. With no key the page falls back to fixture mode and shows a setup notice;
search and `?track=` require a key.

Cached lyrics live in `localStorage` for 24 hours, capped at 30 entries and
1 MB with least-recently-used eviction. There is no prefetch or bulk download;
**Clear cached lyrics** empties the cache.

## API, sources and attribution

Requests go only to `https://api.spicylyrics.org`, plus the artwork URL returned
by the API and local assets. There is no font or icon CDN.

- `GET /v1/lyrics/{id}` returns `Body.Type` of `Syllable`, `Line` or `Static`.
- `GET /v1/tracks/search` returns candidates with a `tier`. Only `exact` is
  auto-selected; `likely`, `version`, `other_artist` and `related` require a tap.
- Source and attribution are always visible beside real lyrics. Community
  syncs link the uploader and maker; Apple Music and Spotify are credited by
  name. The API and sync data are not re-exposed.

The service is free but rate-limited, with no uptime guarantee. Each viewer
needs their own key. A publishable key is designed for browser clients with an
origin allowlist. Reviewed documentation: <https://developers.spicylyrics.org/docs>.

Error states are distinct and actionable: missing key, 401, 403 origin,
404 no lyrics, 429 with Retry-After, timeout, server failure, malformed
response, and offline. Every state offers Retry. Loading shows a skeleton and a
"Still loading" line with Retry after 10 seconds, never an endless spinner.
When the browser goes offline, the clock pauses and cached lyrics are not shown
as live.

## Design exception

[`DESIGN.md`](../DESIGN.md) and [`tokens.css`](../tokens.css) define the light,
touch-first kiosk theme. This stage intentionally uses a dark cinema surface:
it is viewed in a dim room, the cover supplies all colour, and the lyrics are
the only sharp element. The exception is scoped to `--stage-*` tokens in
[`lyrics.css`](../lyrics.css). Spacing, type steps, radii, motion durations and
44 px touch targets still come from `tokens.css`. The dark surface does not
change the adopted kiosk design system.

Colour is cover-derived only: no new brand accent. Semantic colour is reserved
for errors. Body and muted text are kept at AA contrast over the scrim; use
`?cover=light` to verify the brightest case.

Two deliberate choices that a design detector flags: Inter is the adopted
font token with a system-sans fallback, and `background-clip: text` carries a
gradient that encodes sung progress rather than decoration. The gradient has a
solid-colour fallback under `@supports`.

## Architecture

- `src/lyrics/parse.js` — DOM-free normalization of API payloads.
- `src/lyrics/clock.js` — demo clock with `position`, `duration`, `playing`,
  `offsetMs`, `play`, `pause`, `seek` and `setOffset`. A future player adapter
  can replace it without touching the stage.
- `src/lyrics/api.js` — fetch client, error mapping, key store and cache.
- `src/lyrics/fixtures.js` — original placeholder fixtures.
- `src/lyrics/stage.js` — DOM rendering, lyric window, controls, settings and
  attribution. Per-frame work changes only text and `--fill` variables; the
  cover blur is composited and the animation loop suspends in hidden tabs.

Reduced motion disables cover drift, shimmer and line transitions. Controls
auto-hide after five seconds while playing and return on tap, pointer move or
keyboard focus. Space and arrow keys work outside inputs; Escape closes the
settings sheet, which traps focus and returns it to the gear.

## Limitations

- No audio playback. The timeline is a demo clock; there is no real queue, so
  previous/next and volume controls are deliberately absent.
- No Spotify login, Web API, Canvas, music video or DRM, and no private
  endpoints.
- No Home Assistant, Music Assistant, `hass`, media-player or service calls.
- No Rust, Slint, `Cargo.*` or `PLAN.md` changes.
- Background vocals are rendered smaller and dimmer when their timing exists;
  untimed background text is shown without a fill.
- Very long static lyric sets are not paged; this prototype is for visual
  review, not production playback.

## Follow-ups (not in this issue)

- A real playback clock adapter (Music Assistant, Home Assistant or audio).
- A licensed local video loop or a separate Canvas legal/technical spike.
- A Home Assistant custom card or a kiosk screen integration.
- A native Slint lyrics view under the software-renderer constraints.
