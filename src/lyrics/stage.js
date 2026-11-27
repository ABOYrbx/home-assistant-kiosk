// Stage controller. Owns DOM, state and interaction; imports DOM-free modules
// for parsing, timing, API access and fixtures.

import { Clock, CLOCK_LIMITS } from './clock.js';
import {
  LyricsCache,
  errorMessage,
  getLyrics,
  isPublishableKey,
  keyStore,
  searchTracks,
} from './api.js';
import { attributionFor, formatTime, normalizeLyrics, safeUrl } from './parse.js';
import { getFixture, hasFixture } from './fixtures.js';

// All user-facing strings live here so localization does not require touching
// the rendering code.
const STRINGS = {
  demoBadge: 'Demo · no real playback',
  idleTitle: 'Search for a track',
  idleHint: 'Add a publishable key in Settings to load real lyrics, or review a demo below.',
  loadingTitle: 'Loading lyrics',
  loadingHint: 'Reading the sync from the Spicy Lyrics API.',
  retry: 'Retry',
  syncNote: 'Not synced — this source has no timing data',
  emptyTitle: 'No lyrics for this track',
  emptyHint: 'The selected source has no lyrics here. Choose a demo or search for another track.',
  noTrackTitle: 'No track selected',
  offlineTitle: 'Offline — lyrics unavailable',
  offlineHint: 'The connection is down. Reconnect, then retry; cached lyrics are not shown as live.',
  backOnline: 'Back online. Retry to load lyrics.',
  keyNeeded: 'Key needed',
  keyNeededHint: 'Add a publishable Spicy Lyrics key in Settings, or review a demo fixture.',
  invalidKey: 'Use a publishable key that starts with sl_pk_.',
  keySaved: 'Key saved in this browser only.',
  keyCleared: 'Key cleared. Choose a demo or add a key to search.',
  searchNeedTerm: 'Enter a title or an artist to search.',
  searchNeedKey: 'Add a publishable key before searching.',
  searchWorking: 'Searching…',
  searchNoResults: 'No matches. Try fewer filters.',
  searchExact: 'Exact match loaded.',
  offsetSaved: 'Offset stored in this browser.',
  cacheCleared: 'Cached lyrics cleared.',
  notSyncedNote: 'Static sync: no timing data, so no line is animated.',
  retryGeneric: 'Retry, or choose a demo fixture.',
};

const ERROR_TITLES = Object.freeze({
  missing_key: STRINGS.keyNeeded,
  unauthorized: 'Key not accepted',
  forbidden: 'Origin not allowed',
  not_found: 'No lyrics found',
  rate_limited: 'Rate limited',
  offline: STRINGS.offlineTitle,
  timeout: 'Request timed out',
  server: 'Service unavailable',
  invalid_response: 'Unreadable response',
  cancelled: 'Request cancelled',
  invalid_id: 'No track id',
});

const OFFSET_KEY = 'lyrics-stage:offset';
const AUTO_HIDE_MS = 5000;
const LOADING_LATE_MS = 10000;
const SEEK_STEP_MS = 5000;

const byId = (id) => document.getElementById(id);

function readOffset() {
  try {
    return Number(localStorage.getItem(OFFSET_KEY)) || 0;
  } catch {
    return 0;
  }
}

function writeOffset(value) {
  try {
    localStorage.setItem(OFFSET_KEY, String(value));
  } catch {
    // Non-persistent offset still works for this session.
  }
}

function artworkUrl(value) {
  if (typeof value !== 'string' || !value.trim()) return null;
  const candidate = value.trim();
  // Fixtures ship a local asset; API artwork must be http(s).
  if ((candidate.startsWith('./') || candidate.startsWith('/')) && !candidate.includes('://')) {
    return candidate;
  }
  return safeUrl(candidate);
}

function announce(message) {
  if (message) byId('live-status').textContent = message;
}

function setStatus(node, message, isError = false) {
  node.textContent = message;
  node.classList.toggle('sheet__status--error', Boolean(isError) && Boolean(message));
}

/** Build the lyric DOM once per track; per-frame work only changes --fill. */
function appendSegment(parent, segment, dimmed) {
  const span = document.createElement('span');
  span.className = dimmed ? 'seg seg--dim' : 'seg';
  span.style.setProperty('--fill', '0');

  const base = document.createElement('span');
  base.className = 'seg__base';
  base.textContent = segment.text;

  const ink = document.createElement('span');
  ink.className = 'seg__ink';
  ink.setAttribute('aria-hidden', 'true');
  ink.textContent = segment.text;

  span.append(base, ink);
  parent.appendChild(span);
  return { span, segment };
}

function appendSegments(parent, line, dimmed) {
  const targets = [];
  if (line.words.length > 0) {
    for (const word of line.words) {
      if (word.leadingSpace) parent.appendChild(document.createTextNode(' '));
      const wordNode = document.createElement('span');
      wordNode.className = 'lyric-word';
      for (const segment of word.syllables) {
        targets.push(appendSegment(wordNode, segment, dimmed));
      }
      parent.appendChild(wordNode);
    }
  } else {
    for (const segment of line.segments) {
      targets.push(appendSegment(parent, segment, dimmed));
    }
  }
  return targets;
}

function fillFor(position, startMs, endMs) {
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) return 0;
  return Math.min(1, Math.max(0, (position - startMs) / (endMs - startMs)));
}

export function start() {
  const el = {
    stage: byId('stage'),
    idle: byId('idle'),
    idleClock: byId('idle-clock'),
    idleDate: byId('idle-date'),
    idleHint: byId('idle-hint'),
    lyricsView: byId('lyrics-view'),
    lineList: byId('line-list'),
    syncNote: byId('sync-note'),
    attribution: byId('attribution'),
    statePanel: byId('state-panel'),
    skeleton: byId('skeleton'),
    stateContent: byId('state-content'),
    stateTitle: byId('state-title'),
    stateMessage: byId('state-message'),
    stateRetry: byId('state-retry'),
    stateLate: byId('state-late'),
    lateRetry: byId('late-retry'),
    meta: byId('track-meta'),
    cover: byId('cover'),
    metaCover: byId('meta-cover'),
    metaTitle: byId('meta-title'),
    metaArtist: byId('meta-artist'),
    metaAlbum: byId('meta-album'),
    controls: byId('controls'),
    scrub: byId('scrub'),
    timeCurrent: byId('time-current'),
    timeTotal: byId('time-total'),
    playToggle: byId('play-toggle'),
    iconPlay: document.querySelector('.icon-play'),
    iconPause: document.querySelector('.icon-pause'),
    restart: byId('restart'),
    settingsOpen: byId('settings-open'),
    settingsClose: byId('settings-close'),
    settingsSheet: byId('settings-sheet'),
    sheetBackdrop: byId('sheet-backdrop'),
    keyInput: byId('key-input'),
    keySave: byId('key-save'),
    keyClear: byId('key-clear'),
    keyStatus: byId('key-status'),
    searchTitle: byId('search-title-input'),
    searchArtist: byId('search-artist'),
    searchAlbum: byId('search-album'),
    searchDuration: byId('search-duration'),
    searchSubmit: byId('search-submit'),
    searchStatus: byId('search-status'),
    results: byId('results'),
    offset: byId('offset'),
    offsetValue: byId('offset-value'),
    offsetReset: byId('offset-reset'),
    cacheClear: byId('cache-clear'),
  };

  const cache = new LyricsCache();
  el.demoBadge = byId('demo-badge');
  el.demoBadge.textContent = STRINGS.demoBadge;

  const state = {
    mode: 'idle',
    normalized: null,
    meta: {},
    demo: false,
    fixture: null,
    key: keyStore.read(),
    loadId: 0,
    lineNodes: [],
    activeIndex: -1,
    lastLoad: null,
    retryAction: null,
    sheetOpen: false,
    coverTest: null,
    lastFocus: null,
    controlsTimer: null,
    loadingTimer: null,
    wasPlaying: false,
    scrubbing: false,
    searchController: null,
    small: typeof matchMedia === 'function' ? matchMedia('(max-width: 800px)') : null,
  };

  const clock = new Clock({
    duration: 0,
    offsetMs: readOffset(),
    onUpdate: renderClock,
  });

  // --- Mode and state panels -----------------------------------------------

  function setMode(mode) {
    state.mode = mode;
    const populated = mode === 'populated';
    el.idle.hidden = mode !== 'idle';
    el.lyricsView.hidden = !populated;
    el.statePanel.hidden = !(['loading', 'error', 'empty'].includes(mode));
    el.controls.hidden = !populated;
    el.meta.hidden = !populated;
    el.stateContent.hidden = !(mode === 'error' || mode === 'empty');
    el.skeleton.hidden = mode !== 'loading';
    el.stateLate.hidden = true;
    el.statePanel.classList.toggle('state-panel--error', mode === 'error');
    if (!populated) el.stage.classList.remove('stage--controls-hidden');
  }

  function showIdle(message) {
    clearTimeout(state.loadingTimer);
    clearTimeout(state.controlsTimer);
    state.loadingTimer = null;
    state.controlsTimer = null;
    clock.pause();
    state.normalized = null;
    state.lineNodes = [];
    state.activeIndex = -1;
    state.lastLoad = null;
    el.lineList.replaceChildren();
    el.stage.classList.remove('stage--offline');
    el.idleHint.textContent = message || STRINGS.idleHint;
    setMode('idle');
    updateIdleClock();
  }

  function showLoading() {
    clearTimeout(state.loadingTimer);
    el.stateTitle.textContent = STRINGS.loadingTitle;
    el.stateMessage.textContent = STRINGS.loadingHint;
    el.stateRetry.hidden = true;
    setMode('loading');
    state.loadingTimer = setTimeout(() => {
      if (state.mode === 'loading') {
        el.stateLate.hidden = false;
        announce('Still loading lyrics.');
      }
    }, LOADING_LATE_MS);
  }

  function showError(error, retryAction = retryLast) {
    clearTimeout(state.loadingTimer);
    const code = error?.code ?? 'server';
    const title = ERROR_TITLES[code] ?? 'Could not load lyrics';
    const message = errorMessage(error) || STRINGS.retryGeneric;
    el.stateTitle.textContent = title;
    el.stateMessage.textContent = message;
    el.stateRetry.textContent = STRINGS.retry;
    el.stateRetry.hidden = false;
    state.retryAction = retryAction;
    setMode('error');
    el.stage.classList.toggle('stage--offline', code === 'offline');
    if (code === 'offline' || code === 'unauthorized' || code === 'forbidden') clock.pause();
    announce(`${title}. ${message}`);
  }

  function showEmptyLyrics() {
    clearTimeout(state.loadingTimer);
    el.stateTitle.textContent = STRINGS.emptyTitle;
    el.stateMessage.textContent = STRINGS.emptyHint;
    el.stateRetry.textContent = 'Choose another track';
    el.stateRetry.hidden = false;
    state.retryAction = () => showIdle();
    setMode('empty');
    announce(STRINGS.emptyTitle);
  }

  // --- Track loading --------------------------------------------------------

  function applyMeta(meta) {
    state.meta = meta || {};
    el.metaTitle.textContent = state.meta.title || 'Unknown title';
    el.metaArtist.textContent = state.meta.artist || 'Unknown artist';
    el.metaAlbum.textContent = state.meta.album || '';
    el.metaAlbum.hidden = !state.meta.album;
    const url = state.coverTest ? null : artworkUrl(state.meta.artwork);
    if (url) {
      el.cover.src = url;
      el.metaCover.src = url;
      el.metaCover.hidden = false;
      el.stage.classList.remove('stage--no-cover');
    } else {
      el.cover.removeAttribute('src');
      el.metaCover.removeAttribute('src');
      el.metaCover.hidden = true;
      el.stage.classList.add('stage--no-cover');
    }
  }

  function renderAttribution(normalized, demo) {
    el.attribution.replaceChildren();
    const info = attributionFor(normalized.attribution, { demo });
    el.attribution.appendChild(document.createTextNode(info.text));
    for (const link of info.links) {
      const anchor = document.createElement('a');
      anchor.href = link.url;
      anchor.target = '_blank';
      anchor.rel = 'noopener noreferrer';
      anchor.textContent = link.label;
      el.attribution.appendChild(anchor);
    }
  }

  function finishLoad(startMs) {
    clearTimeout(state.loadingTimer);
    state.loadingTimer = null;
    el.stage.classList.remove('stage--offline');
    state.demo = state.fixture !== null;
    applyMeta(state.meta);
    renderAttribution(state.normalized, state.demo);
    el.syncNote.hidden = state.normalized.type !== 'Static';
    if (state.normalized.type === 'Static') el.syncNote.textContent = STRINGS.notSyncedNote;

    const duration =
      Number(state.meta.durationMs) > 0
        ? Number(state.meta.durationMs)
        : state.normalized.endMs || 180000;
    clock.setDuration(duration);
    clock.seek(startMs || 0);
    clock.play();
    syncPlayButton();
    el.timeTotal.textContent = formatTime(duration);
    setMode('populated');
    revealControls();
    renderClock(clock);
    announce(`Lyrics loaded for ${state.meta.title || 'this track'}. Demo clock, no audio.`);
  }

  async function loadFixture(name) {
    if (!hasFixture(name)) return;
    const token = ++state.loadId;
    state.fixture = name;
    state.lastLoad = { type: 'fixture', name };
    showLoading();
    try {
      const selected = getFixture(name); // the 'error' fixture throws here
      if (token !== state.loadId) return;
      const normalized = normalizeLyrics(selected.payload);
      if (!normalized.hasContent) {
        showEmptyLyrics();
        return;
      }
      state.normalized = normalized;
      buildLines(normalized);
      applyMeta(selected.meta);
      finishLoad(selected.startMs);
      const url = new URL(location.href);
      url.searchParams.set('fixture', name);
      url.searchParams.delete('track');
      history.replaceState(null, '', url);
    } catch (error) {
      if (token === state.loadId) showError(error);
    }
  }

  async function loadTrack(id, meta = {}) {
    const token = ++state.loadId;
    state.fixture = null;
    state.lastLoad = { type: 'track', id, meta };
    showLoading();

    if (!state.key) {
      showError(
        { code: 'missing_key', message: STRINGS.keyNeededHint },
        () => openSettings(true),
      );
      return;
    }

    try {
      let payload = cache.get(id);
      if (!payload) {
        payload = await getLyrics(id, { key: state.key });
        cache.set(id, payload);
      }
      if (token !== state.loadId) return;
      const normalized = normalizeLyrics(payload);
      if (!normalized.hasContent) {
        showEmptyLyrics();
        return;
      }
      state.normalized = normalized;
      state.meta = {
        title: meta.title,
        artist: meta.artist,
        album: meta.album,
        artwork: meta.artwork,
        durationMs: meta.durationMs,
      };
      buildLines(normalized);
      finishLoad(0);
      const url = new URL(location.href);
      url.searchParams.set('track', id);
      url.searchParams.delete('fixture');
      history.replaceState(null, '', url);
    } catch (error) {
      if (token === state.loadId) showError(error);
    }
  }

  function retryLast() {
    if (!state.lastLoad) {
      openSettings();
      return;
    }
    if (state.lastLoad.type === 'fixture') loadFixture(state.lastLoad.name);
    else loadTrack(state.lastLoad.id, state.lastLoad.meta);
  }

  // --- Rendering ------------------------------------------------------------

  function buildLines(normalized) {
    const fragment = document.createDocumentFragment();
    state.lineNodes = normalized.lines.map((line, index) => {
      const node = document.createElement('p');
      node.className = 'lyric-line';
      node.dataset.index = String(index);
      node.setAttribute('dir', 'auto');

      if (line.isEmpty) {
        const rest = document.createElement('span');
        rest.className = 'lyric-line__rest';
        rest.setAttribute('aria-hidden', 'true');
        rest.textContent = '♪';
        node.appendChild(rest);
        const label = document.createElement('span');
        label.className = 'sr-only';
        label.textContent = 'instrumental break';
        node.appendChild(label);
      } else {
        const fills = appendSegments(node, line, false);
        for (const background of line.background) {
          const layer = document.createElement('span');
          layer.className = 'lyric-line__background';
          layer.setAttribute('dir', 'auto');
          fills.push(...appendSegments(layer, background, true));
          node.appendChild(layer);
        }
        return { node, line, fills };
      }
      return { node, line, fills: [] };
    });
    for (const entry of state.lineNodes) fragment.appendChild(entry.node);
    el.lineList.dataset.sync = normalized.type === 'Static' ? 'static' : 'synced';
    el.lineList.replaceChildren(fragment);
    state.activeIndex = -1;
  }

  function updateWindow(activeIndex) {
    const total = state.lineNodes.length;
    if (state.normalized?.type === 'Static') {
      for (const entry of state.lineNodes) {
        entry.node.hidden = false;
        entry.node.classList.remove('lyric-line--active', 'lyric-line--near', 'lyric-line--far');
      }
      return;
    }
    const size = state.small?.matches ? 3 : 5;
    const half = Math.floor(size / 2);
    const start = Math.min(Math.max(activeIndex - half, 0), Math.max(0, total - size));
    const end = Math.min(total, start + size);

    state.lineNodes.forEach((entry, index) => {
      const distance = Math.abs(index - activeIndex);
      entry.node.hidden = index < start || index >= end;
      entry.node.classList.toggle('lyric-line--active', index === activeIndex);
      entry.node.classList.toggle('lyric-line--near', distance === 1);
      entry.node.classList.toggle('lyric-line--far', distance > 1);
    });
  }

  function findActiveIndex(position) {
    const lines = state.normalized?.lines ?? [];
    let found = 0;
    for (let index = 0; index < lines.length; index += 1) {
      const line = lines[index];
      if (Number.isFinite(line.startMs) && line.startMs <= position) found = index;
      if (
        Number.isFinite(line.endMs) &&
        position >= line.startMs &&
        position < line.endMs
      ) {
        return index;
      }
    }
    return found;
  }

  function updateFills(position) {
    const type = state.normalized?.type;
    if (type === 'Static') return;

    state.lineNodes.forEach((entry) => {
      if (entry.node.hidden || !Number.isFinite(entry.line.startMs)) return;

      const useSyllables = type === 'Syllable' && entry.line.segments.every((s) => s.timed);
      if (useSyllables && entry.line.words.length > 0) {
        for (const { span, segment } of entry.fills) {
          if (!segment.timed) continue;
          span.style.setProperty('--fill', String(fillFor(position, segment.startMs, segment.endMs)));
        }
      } else {
        // Line-level fill: never invent word timings.
        const fill = fillFor(position, entry.line.startMs, entry.line.endMs);
        for (const { span } of entry.fills) span.style.setProperty('--fill', String(fill));
      }
    });
  }

  function syncPlayButton() {
    const playing = clock.playing;
    // SVGElement does not implement the HTML hidden IDL property in every
    // browser; toggle the attribute so the CSS rule applies reliably.
    el.iconPlay.toggleAttribute('hidden', playing);
    el.iconPause.toggleAttribute('hidden', !playing);
    el.playToggle.setAttribute('aria-label', playing ? 'Pause' : 'Play');
  }

  function revealControls() {
    clearTimeout(state.controlsTimer);
    state.controlsTimer = null;
    el.stage.classList.remove('stage--controls-hidden');
    if (state.mode !== 'populated' || !clock.playing || state.sheetOpen) return;
    const active = document.activeElement;
    if (active && el.controls.contains(active)) return;
    state.controlsTimer = setTimeout(() => {
      state.controlsTimer = null;
      if (state.mode !== 'populated' || !clock.playing || state.sheetOpen) return;
      const focused = document.activeElement;
      if (focused && el.controls.contains(focused)) return;
      el.stage.classList.add('stage--controls-hidden');
    }, AUTO_HIDE_MS);
  }

  function renderClock(current) {
    if (state.wasPlaying !== current.playing) {
      state.wasPlaying = current.playing;
      syncPlayButton();
      revealControls();
    }
    if (state.mode !== 'populated') return;

    const position = current.effectivePosition;
    const duration = current.duration;
    el.timeCurrent.textContent = formatTime(position);
    el.scrub.style.setProperty('--fill', duration > 0 ? String(position / duration) : '0');
    if (!state.scrubbing) {
      el.scrub.value = String(duration > 0 ? Math.round((position / duration) * 1000) : 0);
    }

    if (state.normalized?.type === 'Static') return;
    const activeIndex = findActiveIndex(position);
    if (activeIndex !== state.activeIndex) {
      state.activeIndex = activeIndex;
      updateWindow(activeIndex);
    }
    updateFills(position);
  }

  // --- Settings sheet -------------------------------------------------------

  function focusableInSheet() {
    return [...el.settingsSheet.querySelectorAll(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    )].filter((node) => !node.hidden && node.getClientRects().length > 0);
  }

  function openSettings(focusKey = false) {
    if (state.sheetOpen) return;
    state.sheetOpen = true;
    state.lastFocus = document.activeElement;
    el.settingsSheet.hidden = false;
    el.sheetBackdrop.hidden = false;
    el.settingsOpen.setAttribute('aria-expanded', 'true');
    for (const child of el.stage.children) {
      if (child !== el.settingsSheet && child !== el.sheetBackdrop) child.inert = true;
    }
    revealControls();
    const target = focusKey ? el.keyInput : el.settingsClose;
    target.focus();
  }

  function closeSettings() {
    if (!state.sheetOpen) return;
    state.sheetOpen = false;
    el.settingsSheet.hidden = true;
    el.sheetBackdrop.hidden = true;
    el.settingsOpen.setAttribute('aria-expanded', 'false');
    for (const child of el.stage.children) child.inert = false;
    revealControls();
    (state.lastFocus && state.lastFocus.focus) ? state.lastFocus.focus() : el.settingsOpen.focus();
  }

  function trapFocus(event) {
    const focusables = focusableInSheet();
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function saveKey() {
    const value = el.keyInput.value.trim();
    if (!isPublishableKey(value)) {
      setStatus(el.keyStatus, STRINGS.invalidKey, true);
      return;
    }
    if (!keyStore.write(value)) {
      setStatus(el.keyStatus, 'This browser blocked local storage.', true);
      return;
    }
    state.key = value;
    el.keyInput.value = '';
    setStatus(el.keyStatus, STRINGS.keySaved);
    announce(STRINGS.keySaved);
  }

  function clearKey() {
    keyStore.write('');
    state.key = '';
    el.keyInput.value = '';
    setStatus(el.keyStatus, STRINGS.keyCleared);
    if (!state.demo || state.mode === 'loading') showIdle(STRINGS.keyCleared);
  }

  function renderResults(hits) {
    el.results.replaceChildren();
    for (const hit of hits) {
      const item = document.createElement('li');
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'result-row';

      const title = document.createElement('span');
      title.className = 'result-row__title';
      title.textContent = hit.title;
      const meta = document.createElement('span');
      meta.className = 'result-row__meta';
      meta.textContent = [hit.artists, hit.album, hit.durationMs ? formatTime(hit.durationMs) : '']
        .filter(Boolean)
        .join(' · ');
      const tier = document.createElement('span');
      tier.className = 'result-row__tier';
      tier.textContent = hit.tier === 'exact' ? 'Exact match' : `Similar result (${hit.tier})`;

      button.append(title, meta, tier);
      button.addEventListener('click', () => {
        loadTrack(hit.id, {
          title: hit.title,
          artist: hit.artists,
          album: hit.album,
          artwork: hit.artwork,
          durationMs: hit.durationMs,
        });
        closeSettings();
      });
      item.appendChild(button);
      el.results.appendChild(item);
    }
  }

  async function runSearch() {
    const title = el.searchTitle.value.trim();
    const artist = el.searchArtist.value.trim();
    const album = el.searchAlbum.value.trim();
    const rawDuration = Number(el.searchDuration.value);

    if (!title && !artist) {
      setStatus(el.searchStatus, STRINGS.searchNeedTerm, true);
      return;
    }
    if (!state.key) {
      setStatus(el.searchStatus, STRINGS.searchNeedKey, true);
      openSettings(true);
      return;
    }

    el.searchController?.abort();
    el.searchController = new AbortController();
    setStatus(el.searchStatus, STRINGS.searchWorking);
    el.results.replaceChildren();
    try {
      const hits = await searchTracks(
        {
          title,
          artist,
          album,
          duration: Number.isFinite(rawDuration) && rawDuration > 0 ? rawDuration : undefined,
          limit: 8,
        },
        { key: state.key, signal: el.searchController.signal },
      );
      renderResults(hits);
      if (hits.length === 0) {
        setStatus(el.searchStatus, STRINGS.searchNoResults);
        return;
      }
      const exact = hits.find((hit) => hit.tier === 'exact');
      if (exact) {
        setStatus(el.searchStatus, STRINGS.searchExact);
        loadTrack(exact.id, {
          title: exact.title,
          artist: exact.artists,
          album: exact.album,
          artwork: exact.artwork,
          durationMs: exact.durationMs,
        });
        closeSettings();
      } else {
        setStatus(el.searchStatus, 'No exact match. Tap a result to load it.');
      }
    } catch (error) {
      if (error?.code === 'cancelled') return;
      setStatus(el.searchStatus, errorMessage(error), true);
    }
  }

  function syncOffset(value) {
    const next = Math.min(
      CLOCK_LIMITS.OFFSET_LIMIT_MS,
      Math.max(-CLOCK_LIMITS.OFFSET_LIMIT_MS, Number(value) || 0),
    );
    clock.setOffset(next);
    el.offset.value = String(clock.offsetMs);
    el.offsetValue.textContent = `${clock.offsetMs > 0 ? '+' : ''}${clock.offsetMs} ms`;
    writeOffset(clock.offsetMs);
  }

  // --- Idle clock and environment ------------------------------------------

  function updateIdleClock() {
    const now = new Date();
    const time = new Intl.DateTimeFormat(undefined, {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    });
    const date = new Intl.DateTimeFormat(undefined, {
      weekday: 'short',
      day: 'numeric',
      month: 'short',
    });
    el.idleClock.textContent = time.format(now);
    el.idleClock.setAttribute('datetime', now.toISOString());
    el.idleDate.textContent = date.format(now);
  }

  // --- Events ---------------------------------------------------------------

  function bindEvents() {
    el.settingsOpen.addEventListener('click', () => openSettings());
    el.settingsClose.addEventListener('click', closeSettings);
    el.sheetBackdrop.addEventListener('click', closeSettings);

    el.stateRetry.addEventListener('click', () => state.retryAction?.());
    el.lateRetry.addEventListener('click', () => state.retryAction?.());

    el.playToggle.addEventListener('click', () => {
      clock.toggle();
      syncPlayButton();
      revealControls();
    });
    el.restart.addEventListener('click', () => {
      clock.restart();
      if (!clock.playing) clock.play();
      renderClock(clock);
      revealControls();
    });

    el.scrub.style.setProperty('--fill', '0');
    el.scrub.addEventListener('input', () => {
      state.scrubbing = true;
      const fraction = Number(el.scrub.value) / 1000;
      clock.seek(fraction * clock.duration);
      el.scrub.style.setProperty('--fill', String(fraction));
      renderClock(clock);
    });
    const endScrub = () => {
      state.scrubbing = false;
      renderClock(clock);
    };
    el.scrub.addEventListener('change', endScrub);
    el.scrub.addEventListener('pointerup', endScrub);

    document.querySelectorAll('[data-fixture]').forEach((button) => {
      button.addEventListener('click', () => {
        loadFixture(button.dataset.fixture);
        if (state.sheetOpen) closeSettings();
      });
    });

    el.keySave.addEventListener('click', saveKey);
    el.keyClear.addEventListener('click', clearKey);
    el.searchSubmit.addEventListener('click', runSearch);
    el.offset.addEventListener('input', () => syncOffset(el.offset.value));
    el.offsetReset.addEventListener('click', () => syncOffset(0));
    el.cacheClear.addEventListener('click', () => {
      cache.clear();
      setStatus(byId('cache-status'), STRINGS.cacheCleared);
      announce(STRINGS.cacheCleared);
    });

    el.stage.addEventListener('pointermove', revealControls);
    el.stage.addEventListener('pointerdown', revealControls);
    el.stage.addEventListener('focusin', revealControls);

    document.addEventListener('keydown', (event) => {
      if (state.sheetOpen) {
        if (event.key === 'Escape') {
          event.preventDefault();
          closeSettings();
        } else if (event.key === 'Tab') {
          trapFocus(event);
        }
        return;
      }
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest('input, textarea, select, button, a, [contenteditable="true"]')
      ) {
        return;
      }
      if (state.mode !== 'populated') return;
      if (event.code === 'Space') {
        event.preventDefault();
        clock.toggle();
        syncPlayButton();
        revealControls();
      } else if (event.key === 'ArrowRight') {
        event.preventDefault();
        clock.seek(clock.position + SEEK_STEP_MS);
      } else if (event.key === 'ArrowLeft') {
        event.preventDefault();
        clock.seek(clock.position - SEEK_STEP_MS);
      } else if (event.key === 'Home') {
        event.preventDefault();
        clock.restart();
      }
    });

    document.addEventListener('visibilitychange', () => {
      if (document.hidden) clock.suspend();
      else clock.resume();
    });

    window.addEventListener('offline', () => {
      if (state.mode === 'populated' || state.mode === 'loading') {
        showError(
          { code: 'offline', message: STRINGS.offlineHint },
          retryLast,
        );
      }
    });
    window.addEventListener('online', () => announce(STRINGS.backOnline));

    el.small?.addEventListener?.('change', () => {
      if (state.mode === 'populated') {
        state.activeIndex = -1;
        renderClock(clock);
      }
    });
  }

  // --- Boot -----------------------------------------------------------------

  bindEvents();
  syncOffset(readOffset());
  updateIdleClock();
  setInterval(updateIdleClock, 1000);
  setMode('idle');

  const params = new URLSearchParams(location.search);
  const coverTest = params.get('cover');
  state.coverTest = coverTest === 'light' || coverTest === 'dark' ? coverTest : null;
  if (coverTest === 'light') el.stage.classList.add('stage--light-cover');
  if (coverTest === 'dark') el.stage.classList.add('stage--dark-cover');

  const fixture = params.get('fixture');
  const track = params.get('track');
  if (fixture && hasFixture(fixture)) {
    loadFixture(fixture);
  } else if (track) {
    loadTrack(track);
  } else {
    showIdle();
  }

  return { clock, state };
}
