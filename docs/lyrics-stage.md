# Lyrics stage: design prototype and live Home Assistant card

This document covers both layers of the lyrics stage:

1. The shared stage core (`stage.js`, `stage.css.js`, `parse.js`, `api.js`), first
   specified by issue #279 as a standalone design prototype.
2. The live Home Assistant / Music Assistant card added by issue #280
   (`card.js`, `ma.js`, `live.js`, `config.js`), built on that same core.

There is one lyric renderer and one Spicy API client. The card does not contain
a second copy. Both layers are JavaScript only; the native Rust/Slint app is
unchanged.

## What it does

The card is a plain custom element, `lyrics-stage-card`, loaded as a Lovelace
resource. Home Assistant supplies the dashboard shell and the authenticated
`hass` object; the card does not import HA frontend internals, does not need a
HA token, and does not call HA services. Music Assistant (MA) is the source of
truth for playback (player, track, cover, duration, queue time, transport
commands). The Spicy Lyrics API is the source for lyrics. There is no backend
service and no token in a URL.

## Install

### 1. Build the bundle

```bash
npm install
npm run build:card
```

The library build writes `dist/lyrics-stage-card.js`. The normal page build
(`npm run build`) is independent and still produces `dist/index.html`.

### 2. Copy the bundle into Home Assistant

Copy the built file into the HA config `www` directory, which HA serves at
`/local/`:

```bash
mkdir -p <ha-config>/www/lyrics-stage
cp dist/lyrics-stage-card.js <ha-config>/www/lyrics-stage/lyrics-stage-card.js
```

Add it as a Lovelace resource (Settings -> Dashboards -> Resources, or in
`configuration.yaml` / the dashboard's `resources:` block):

```yaml
url: /local/lyrics-stage/lyrics-stage-card.js
type: module
```

Reload the browser after changing resources. The stage survives a browser
reload and a Home Assistant restart: on connect it re-authenticates with MA,
restores the pinned player and offset from `localStorage`, and reloads lyrics
for the current track.

### 3. Create a Music Assistant long-lived token

In Music Assistant: **Settings -> Profile -> Long-lived access tokens**, create
a token with a 365-day lifetime. Store it in a private HA secret or the kiosk's
private configuration, never in the repository. Rotate it by creating a new
token, updating the secret, and reloading the dashboard. A rejected token puts
the card into `auth_failed`; it does not retry with a bad token.

### 4. Get a Spicy publishable key

Create an application in the Spicy Lyrics developer dashboard and copy the
**publishable** key (`sl_pk_...`). Add the kiosk origin to that key's allowlist:
the origin of the HA dashboard URL the kiosk loads, and `http://localhost:5914`
for local preview. A secret key (`sl_sk_...`) must never ship to a browser and
is rejected by the card. The key is quota-bound and is still not committed;
with a publishable key in a browser, the origin allowlist is the security
boundary.

The key can come from the card config or from the local settings field (the
gear button). A key entered in the settings field is stored only in this
browser under `lyrics-stage:spicy-key`.

### 5. Add a panel view

```yaml
views:
  - title: Lyrics
    path: lyrics
    panel: true
    cards:
      - type: custom:lyrics-stage-card
        ma_url: http://music-assistant.local:8095
        ma_token: !secret ma_long_lived_token
        player: Living Room
        spicy_key: sl_pk_replace_me
        offset_ms: 300
        back_path: /lovelace/0
        fullscreen: true
```

Replace every placeholder. If the dashboard has no secret support, keep the
MA token and Spicy key in the kiosk's private local configuration or enter the
Spicy key in the settings field; do not paste them into a public dashboard
file. The `ma_url` may be `https://` when MA is behind a proxy; TLS validation
is handled by the browser.

Configuration fields:

| Field | Required | Meaning |
| --- | --- | --- |
| `ma_url` | yes | MA base URL (`http(s)://host:port`). The client derives `ws(s)://host:port/ws`. |
| `ma_token` | yes | MA long-lived token. Never logged. |
| `player` | no | MA player id or exact display name. If omitted, see selection below. |
| `spicy_key` | yes | Publishable Spicy key, or use the settings field. |
| `offset_ms` | no | Lyrics offset, `-2000..2000`, snapped to 50 ms. Per-browser overrides are saved by the gear button. |
| `back_path` | no | Fallback path when there is no browser history. Default `/lovelace/0`. |
| `fullscreen` | no | Cover the viewport in the panel view. Default `true`. |

Structural errors (bad URL, non-boolean `fullscreen`, invalid `back_path`,
non-numeric offset) are thrown by `setConfig` and shown by HA. Missing secrets
are an honest setup state on the stage, not a blank card.

### 6. Open it from "now playing"

Add a tap action to the existing now-playing card, for example:

```yaml
tap_action:
  action: navigate
  navigation_path: /lovelace/lyrics
```

The card itself never hijacks navigation. Auto-opening on playback is a
non-goal: use an HA automation or the existing card's auto-switch if you want
it. If the now-playing card's source is available, embedding the stage as a
tab there is preferred over adding a second fullscreen view; that is a change
to the owner's dashboard, not to this repository.

## Player and playback

- If `player` is set, that exact id or name is pinned. If the player is
  unavailable, off, or missing, the card shows an honest idle state and does
  **not** silently switch.
- If `player` is omitted, the first playing/buffering player is chosen;
  otherwise the last active player. The result is persisted in
  `lyrics-stage:player`, which acts as the pinned choice until the configured
  name changes.
- The selected player's name is visible in the stage header.
- Current media comes from MA `current_media` (with the queue item as the
  duration fallback). A track change is detected by uri plus title, artist,
  and duration. On change the old lyrics are cleared, a loading overlay is
  shown, and the new lookup starts; stale lyrics are never shown as current.
- Position is anchored to MA's queue time and interpolated with
  `performance.now()` while playing. Pause freezes; play, seek, track change,
  and reconnect re-anchor. MA's own clock correction and `resync()` are used
  on connect and when the tab becomes visible. A fresh MA anchor closer than
  150 ms is accepted without a jump; a larger difference re-syncs.
- The offset is applied **only** to the lyric lookup. The time label always
  shows MA's real position.
- Commands (play/pause, previous, next, seek, volume, mute) are marked pending
  and only confirmed when the matching MA event arrives. A command that is
  not confirmed within five seconds is reported as unconfirmed, never as a
  success. Volume changes are debounced (150 ms) and the final value is sent
  on release. Unsupported controls are disabled from the player model.
- When MA disconnects, the last frame stays visible with an Offline badge and
  a frozen position; lyrics do not advance. A Position stale badge appears if
  the anchor is older than 30 seconds during playback. Reconnecting resyncs.

## Lyrics

- Lyrics are fetched on track change only, cached for 24 hours, limited to 30
  entries / 1 MB with LRU eviction, and persisted in `localStorage` under
  `lyrics-stage:cache:v1`. There is no prefetch and no bulk download; Clear
  cache is in the settings sheet.
- If the MA media uri is a Spotify track (`spotify://track/<id>`,
  `spotify:track:<id>`, or `https://open.spotify.com/track/<id>`), the id is
  used directly. Other providers trigger a Spicy search by title, artist,
  album, and duration; only `tier: "exact"` with matching metadata is accepted
  automatically. No Spotify Web API call is made for metadata.
- Syllable sync fills per syllable; Line sync fills over the line's duration;
  Static shows plain text labelled "Not synced". Background vocals are
  smaller and dimmer when they have timing. Zero/negative durations are
  defensive, and long lines wrap.
- Attribution is always visible with the lyrics: the provider, and for community
  syncs the uploader and maker as links (only when those fields exist).
- Distinct states: missing key, 401, 403 origin, 404 no lyrics, 429 with
  Retry-After, network/offline, malformed response, and server errors. Each
  has a message and a Retry action where retrying can help. "No synced lyrics
  for this track" keeps the cover, title, and artist on screen.

## Visual and accessibility notes

The stage is a dark cinema surface, an intentional exception to the light
kiosk theme documented in `DESIGN.md`. Its tokens mirror `tokens.css` but are
declared inside `stage.css.js` because a shadow root cannot load the project
stylesheet. Override `--stage-*` custom properties to retheme. Typography uses
the token scale with fluid lyric sizes, and time labels use tabular numerals.

All controls are at least 44 x 44 px, labelled, keyboard reachable (Space
toggles play, arrows seek, Escape closes settings), and have a visible
`:focus-visible` ring. The lyric area announces status through a live region.
`prefers-reduced-motion` disables background drift, shimmer, and transitions.
The animation loop stops when the tab is hidden. There are no continuous
animations other than the 40 s+ background drift and the lyric fill.

Supported layouts: 1920 x 1200, 1024 x 600, 800 x 480, and 390 x 844. At short
heights the lyric size and padding step down; at phone width the volume slider
is hidden. No horizontal scrolling. In WebKitGTK the `clip-path` fill is used
rather than relying on `background-clip: text`.

## Security

- The MA token and Spicy key stay in the private HA configuration or this
  browser's `localStorage`. They are never committed, logged, or placed in a
  URL, query string, or screenshot.
- MA is contacted only at the configured origin over WS/WSS. Spicy requests go
  only to `https://api.spicylyrics.org` with the key in a Bearer header. No
  third-party image host receives the key or token; MA cover URLs are used as
  given.
- Exception text and URLs are redacted before display (`redactUrl`,
  `safeError`); opaque ids longer than 32 characters are masked. The MA
  client's logger is silent by default.
- Attribution links open in a new tab with `noopener noreferrer`.
- Artwork and lyrics shown in tests or documentation are placeholders, never
  copyrighted material.

## Tests and verification

```bash
npm test                 # node --test, pure logic; no live MA or Spicy calls
npm run build            # existing Vite page build
npm run build:card       # library build -> dist/lyrics-stage-card.js
python3 scripts/check-docs.py
cargo fmt --check        # no Rust changes; still passes
```

Unit tests cover position math and offset, track-change detection, Spotify id
parsing, capability mapping, player selection, config validation, lyric
selection and sync types, cache limits, error mapping, URL redaction, and
command confirmation. Integration behaviour must be checked with a real HA +
MA + kiosk setup: track change, pause, seek, volume, idle, offline/reconnect,
no lyrics, and offset. Record the result in the pull request without secrets,
personal URLs, entity ids, or copyrighted artwork/lyrics.

Gate table to report for a change to the stage: overflow, tokens,
accessibility, semantics, layout — at each of the supported breakpoints, in
WebKit/Safari and one Chromium browser.

## Limitations and non-goals

- No Rust, Slint, `Cargo.*`, `src/**/*.rs`, `ui/`, `PLAN.md`, `tokens.css`,
  `index.html`, or `styles.css` changes.
- No HA long-lived token in the browser, no backend, no token in a URL.
- No Music Assistant server plugin, Spotify login, Spotify Web API, Canvas,
  music videos, or DRM.
- No HACS packaging or release workflow; auto-open is not built in.
- No native Slint lyrics view (post-v1) and no extension-shop work.
- Cross-origin WebSocket access is the kiosk's responsibility. If a reverse
  proxy blocks WSS, document it and decide on a HA-only `media_player`
  fallback separately; it is out of scope here.
- Spicy Lyrics is free but rate-limited with no uptime guarantee. Each user
  needs their own key.
