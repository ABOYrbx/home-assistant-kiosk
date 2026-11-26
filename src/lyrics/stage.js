// Single lyric stage renderer, shared by the design prototype and the live HA
// card. It owns DOM only: a caller drives `render(positionMs)` from a clock.
// No Music Assistant, Home Assistant, or network code lives here.

import { STAGE_CSS } from './stage.css.js';
import { lyricsAt, attributionFor, formatTime } from './parse.js';

const ICONS = {
  back: '<path d="M15 18 9 12l6-6"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 9 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.6 9a1.7 1.7 0 0 0-.34-1.88l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09A1.7 1.7 0 0 0 15 4.6a1.7 1.7 0 0 0 1.88-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.4 9v.03a1.7 1.7 0 0 0 1.56 1.03H21a2 2 0 1 1 0 4h-.09A1.7 1.7 0 0 0 19.4 15Z"/>',
  play: '<path d="M8 5.5 19 12 8 18.5z" fill="currentColor" stroke="none"/>',
  pause: '<path d="M9 5v14M15 5v14"/>',
  previous: '<path d="M18 5v14L8 12z" fill="currentColor" stroke="none"/><path d="M6 5v14"/>',
  next: '<path d="M6 5v14l10-7z" fill="currentColor" stroke="none"/><path d="M18 5v14"/>',
  volume: '<path d="M4 9v6h4l5 4V5L8 9H4z"/><path d="M16 9a4 4 0 0 1 0 6"/>',
  mute: '<path d="M4 9v6h4l5 4V5L8 9H4z"/><path d="m17 9 4 6m0-6-4 6"/>',
};

function icon(name) {
  return `<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">${ICONS[name] ?? ''}</svg>`;
}

function button(action, label, iconName = '') {
  return `<button class="icon" type="button" data-action="${action}" aria-label="${label}" title="${label}">${iconName ? icon(iconName) : ''}</button>`;
}

function textButton(action, label) {
  return `<button type="button" data-action="${action}">${label}</button>`;
}

const TEMPLATE = `
<style>${STAGE_CSS}</style>
<div class="stage">
  <div class="bg is-empty" aria-hidden="true"></div>
  <div class="scrim" aria-hidden="true"></div>
  <div class="vignette" aria-hidden="true"></div>
  <header class="top">
    ${button('back', 'Back to dashboard', 'back')}
    <div class="now">
      <div class="cover" aria-hidden="true"></div>
      <div class="now-text">
        <span class="now-title"></span>
        <span class="now-artist"></span>
      </div>
    </div>
    <span class="player-name"></span>
    <span class="badge" data-state="connecting">Connecting</span>
    ${button('settings', 'Lyrics settings', 'settings')}
  </header>
  <main class="lyrics" aria-label="Lyrics">
    <div class="status" hidden aria-live="polite"></div>
    <div class="lines"></div>
    <div class="attribution" hidden></div>
  </main>
  <footer class="controls">
    <div class="seek-row">
      <span class="time elapsed">0:00</span>
      <input class="seek" type="range" min="0" max="1000" value="0" step="1" aria-label="Seek position" />
      <span class="time duration">0:00</span>
    </div>
    <div class="transport">
      ${button('previous', 'Previous track', 'previous')}
      ${button('play_pause', 'Play', 'play')}
      ${button('next', 'Next track', 'next')}
      <span class="spacer"></span>
      ${button('mute', 'Mute', 'volume')}
      <input class="volume" type="range" min="0" max="100" value="100" step="1" aria-label="Volume" />
    </div>
  </footer>
  <div class="sheet" role="dialog" aria-label="Lyrics settings" aria-modal="true" hidden>
    <h2>Lyrics settings</h2>
    <div class="sheet-row">
      <span>Lyrics offset</span>
      <span>
        ${button('offset_minus', 'Decrease lyrics offset', 'back')}
        <span class="offset-value">0 ms</span>
        ${button('offset_plus', 'Increase lyrics offset', 'next')}
      </span>
    </div>
    <label>Spic publishable key
      <input class="key-input" type="password" autocomplete="off" spellcheck="false" placeholder="sl_pk_..." aria-label="Spicy publishable key" />
    </label>
    <p class="sheet-note">The key stays in this browser. Add the kiosk origin to the key's allowlist.</p>
    <div class="sheet-actions">
      ${textButton('save_key', 'Save key')}
      ${textButton('clear_key', 'Clear key')}
      ${textButton('clear_cache', 'Clear cache')}
      ${textButton('close_settings', 'Close settings')}
    </div>
  </div>
  <div class="sr-only live" aria-live="polite"></div>
</div>
`;

export class LyricsStage {
  constructor(root, { onControl = () => {}, fullscreen = true } = {}) {
    if (!root) throw new Error('LyricsStage requires a root element');
    this.root = root;
    this.options = { onControl };
    this.root.innerHTML = TEMPLATE;
    if (fullscreen && this.root.host instanceof HTMLElement) {
      this.root.host.classList.add('stage--fullscreen');
    }

    this._model = null;
    this._lineRefs = [];
    this._bgRefs = [];
    this._activeIndex = -1;
    this._lastFill = -1;
    this._lastElapsed = -1;
    this._lastDuration = -1;
    this._playing = false;
    this._pending = null;
    this._caps = { play: true, previous: true, next: true, seek: true, volume: true, mute: true };
    this._hideTimer = null;
    this._retry = null;

    this._onClick = (event) => {
      const target = event.target.closest?.('[data-action]');
      if (!target || target.disabled) return;
      this._emit(target.dataset.action);
    };
    this._onInput = (event) => {
      const el = event.target;
      if (el.classList.contains('seek')) {
        this._q('.elapsed').textContent = formatTime(Number(el.value) * 1000);
        this._emit('seek_preview', { ms: Number(el.value) * 1000 });
      } else if (el.classList.contains('volume')) {
        this._emit('volume', { level: Number(el.value) });
      } else if (el.classList.contains('key-input')) {
        this._emit('key_input', { key: el.value });
      }
    };
    this._onChange = (event) => {
      const el = event.target;
      if (el.classList.contains('seek')) this._emit('seek', { ms: Number(el.value) * 1000 });
      if (el.classList.contains('volume')) this._emit('volume_commit', { level: Number(el.value) });
    };
    this._onKeydown = (event) => {
      const sheet = this._q('.sheet');
      if (event.key === 'Escape' && !sheet.hidden) {
        this.toggleSettings(false);
        event.preventDefault();
        return;
      }
      const tag = event.target.tagName?.toLowerCase();
      if (tag === 'input' || tag === 'textarea') return;
      if (event.key === ' ' || event.key === 'Spacebar') {
        this._emit('play_pause');
        event.preventDefault();
      } else if (event.key === 'ArrowRight') {
        this._emit('seek', { ms: this._lastElapsed + 5000 });
        event.preventDefault();
      } else if (event.key === 'ArrowLeft') {
        this._emit('seek', { ms: Math.max(0, this._lastElapsed - 5000) });
        event.preventDefault();
      } else if (event.key === 'Tab' && !sheet.hidden) {
        const focusable = [...sheet.querySelectorAll('button, input')];
        const index = focusable.indexOf(this.root.activeElement);
        const next = event.shiftKey ? (index <= 0 ? focusable.length - 1 : index - 1)
          : (index + 1) % focusable.length;
        if (focusable[next]) { focusable[next].focus(); event.preventDefault(); }
      }
    };
    this._onPointer = () => this.showControls();

    this.root.addEventListener('click', this._onClick);
    this.root.addEventListener('input', this._onInput);
    this.root.addEventListener('change', this._onChange);
    this.root.addEventListener('keydown', this._onKeydown);
    this.root.addEventListener('pointerdown', this._onPointer);
  }

  _q(selector) {
    return this.root.querySelector(selector);
  }

  _emit(action, detail = {}) {
    this.showControls();
    this.options.onControl(action, detail);
  }

  /** Toggle the bottom bar; auto-hide only while playing. */
  showControls() {
    this._q('.controls').classList.remove('is-hidden');
    clearTimeout(this._hideTimer);
    if (this._playing) {
      this._hideTimer = setTimeout(() => this._q('.controls').classList.add('is-hidden'), 5000);
    }
  }

  setTrack(track) {
    const title = String(track?.title ?? '');
    const artist = String(track?.artist ?? '');
    const album = String(track?.album ?? '');
    this._q('.now-title').textContent = title || 'No track selected';
    this._q('.now-artist').textContent = artist ? (album ? `${artist} \u00b7 ${album}` : artist) : '';
    const cover = String(track?.imageUrl ?? '');
    const bg = this._q('.bg');
    if (cover) {
      bg.style.backgroundImage = `url("${cover.replace(/["\\]/g, '')}")`;
      bg.classList.remove('is-empty');
      this._q('.cover').style.backgroundImage = `url("${cover.replace(/["\\]/g, '')}")`;
    } else {
      bg.style.backgroundImage = '';
      bg.classList.add('is-empty');
      this._q('.cover').style.backgroundImage = '';
    }
  }

  setPlayer(name, capabilities = {}) {
    this._q('.player-name').textContent = String(name ?? '');
    this._caps = { ...this._caps, ...capabilities };
    this._applyCapabilities();
  }

  _applyCapabilities() {
    const map = {
      play_pause: this._caps.play,
      previous: this._caps.previous,
      next: this._caps.next,
      mute: this._caps.mute,
    };
    for (const [action, enabled] of Object.entries(map)) {
      const el = this.root.querySelector(`[data-action="${action}"]`);
      if (el) el.disabled = !enabled || Boolean(this._pending);
    }
    const seek = this._q('.seek');
    seek.disabled = !this._caps.seek || Boolean(this._pending);
    const volume = this._q('.volume');
    volume.disabled = !this._caps.volume || Boolean(this._pending);
  }

  setConnection(state) {
    const labels = {
      connecting: 'Connecting', authenticating: 'Authenticating', connected: 'Connected',
      reconnecting: 'Reconnecting', offline: 'Offline', auth_failed: 'Token rejected',
      stopped: 'Offline',
    };
    const badge = this._q('.badge');
    badge.dataset.state = state;
    badge.textContent = labels[state] ?? state;
  }

  setStale(stale) {
    const badge = this._q('.badge');
    if (stale && badge.dataset.state === 'connected') {
      badge.dataset.state = 'stale';
      badge.textContent = 'Position stale';
    } else if (!stale && badge.dataset.state === 'stale') {
      this.setConnection('connected');
    }
  }

  setPlaying(playing) {
    this._playing = Boolean(playing);
    const buttonEl = this.root.querySelector('[data-action="play_pause"]');
    buttonEl.innerHTML = icon(playing ? 'pause' : 'play');
    buttonEl.setAttribute('aria-label', playing ? 'Pause' : 'Play');
    buttonEl.title = playing ? 'Pause' : 'Play';
    this.showControls();
  }

  setVolume(level, muted) {
    const value = Math.min(100, Math.max(0, Number(level) || 0));
    const volume = this._q('.volume');
    if (this.root.activeElement !== volume) volume.value = String(value);
    const buttonEl = this.root.querySelector('[data-action="mute"]');
    buttonEl.innerHTML = icon(muted ? 'mute' : 'volume');
    buttonEl.setAttribute('aria-label', muted ? 'Unmute' : 'Mute');
    buttonEl.title = muted ? 'Unmute' : 'Mute';
  }

  setDuration(ms) {
    this._q('.duration').textContent = formatTime(ms);
    this._q('.seek').max = String(Math.max(1, Math.floor(ms / 1000)));
  }

  setPending(action) {
    this._pending = action;
    for (const el of this.root.querySelectorAll('[data-action]')) {
      const pending = el.dataset.action === action;
      el.classList.toggle('pending', pending);
      el.setAttribute('aria-busy', pending ? 'true' : 'false');
    }
    this._applyCapabilities();
    if (action) this._announce(`${action} pending`);
  }

  setOffset(ms) {
    this._q('.offset-value').textContent = `${Math.round(ms)} ms`;
  }

  setKey(value) {
    this._q('.key-input').value = String(value ?? '');
  }

  toggleSettings(open) {
    const sheet = this._q('.sheet');
    const next = open === undefined ? sheet.hidden : Boolean(open);
    sheet.hidden = !next;
    if (next) {
      this._q('.key-input').focus();
    } else {
      this.root.querySelector('[data-action="settings"]').focus();
    }
  }

  _announce(message) {
    this._q('.live').textContent = message;
  }

  /** Replace the lyric window. Text is always set via textContent. */
  setLyrics(model) {
    this._model = model;
    this._activeIndex = -1;
    this._lastFill = -1;
    this._lineRefs = [];
    this._bgRefs = [];
    const linesEl = this._q('.lines');
    const attributionEl = this._q('.attribution');
    linesEl.replaceChildren();

    if (!model) {
      attributionEl.hidden = true;
      return;
    }

    if (model.type === 'Static') {
      for (const line of model.lines) {
        const p = document.createElement('p');
        p.className = 'line is-active';
        p.style.filter = 'none';
        p.textContent = line.text;
        linesEl.append(p);
      }
      const note = document.createElement('p');
      note.className = 'static-note';
      note.textContent = 'Not synced';
      linesEl.append(note);
    } else {
      for (const line of model.lines) {
        const p = document.createElement('p');
        p.className = 'line';
        const segments = [];
        if (model.type === 'Syllable' && line.syllables.length > 0) {
          for (const syllable of line.syllables) {
            const segment = document.createElement('span');
            segment.className = 'segment';
            const text = document.createElement('span');
            text.textContent = syllable.text;
            const fill = document.createElement('span');
            fill.className = 'fill';
            fill.setAttribute('aria-hidden', 'true');
            fill.textContent = syllable.text;
            segment.append(text, fill);
            p.append(segment);
            segments.push({ el: segment, fill, startMs: syllable.startMs, endMs: syllable.endMs });
          }
        } else {
          const segment = document.createElement('span');
          segment.className = 'segment';
          const text = document.createElement('span');
          text.textContent = line.text;
          const fill = document.createElement('span');
          fill.className = 'fill';
          fill.setAttribute('aria-hidden', 'true');
          fill.textContent = line.text;
          segment.append(text, fill);
          p.append(segment);
          segments.push({ el: segment, fill, startMs: line.startMs, endMs: line.endMs });
        }
        linesEl.append(p);
        this._lineRefs.push({ el: p, segments, startMs: line.startMs, endMs: line.endMs });
      }
      for (const line of model.background) {
        const p = document.createElement('p');
        p.className = 'line is-background';
        p.textContent = line.text;
        linesEl.append(p);
        this._bgRefs.push({ el: p, startMs: line.startMs, endMs: line.endMs });
      }
    }

    attributionEl.replaceChildren();
    const attr = attributionFor(model);
    const text = document.createElement('span');
    text.textContent = attr.providerLabel;
    attributionEl.append(text);
    for (const person of [attr.uploader, attr.maker]) {
      if (!person?.name) continue;
      attributionEl.append(document.createTextNode(' \u00b7 '));
      if (person.url) {
        const link = document.createElement('a');
        link.href = person.url;
        link.target = '_blank';
        link.rel = 'noopener noreferrer';
        link.textContent = person.name;
        attributionEl.append(link);
      } else {
        const span = document.createElement('span');
        span.textContent = person.name;
        attributionEl.append(span);
      }
    }
    attributionEl.hidden = false;
    this.setStatus(null);
  }

  setStatus(status) {
    const el = this._q('.status');
    const lines = this._q('.lines');
    const attribution = this._q('.attribution');
    if (!status) {
      el.hidden = true;
      el.replaceChildren();
      lines.hidden = false;
      return;
    }
    el.hidden = false;
    lines.hidden = true;
    attribution.hidden = true;
    el.dataset.kind = status.kind ?? 'info';
    el.replaceChildren();
    if (status.kind === 'loading') {
      const skeleton = document.createElement('div');
      skeleton.className = 'skeleton';
      for (let i = 0; i < 3; i += 1) skeleton.append(document.createElement('span'));
      el.append(skeleton);
    }
    const p = document.createElement('p');
    p.textContent = status.message ?? '';
    el.append(p);
    this._retry = typeof status.retry === 'function' ? status.retry : null;
    if (this._retry) {
      const retry = document.createElement('button');
      retry.type = 'button';
      retry.textContent = 'Retry';
      retry.dataset.action = 'retry';
      el.append(retry);
    }
    this._announce(p.textContent);
  }

  /** Per-frame work: classes only on line change, one CSS variable per frame. */
  render(positionMs, displayPositionMs = positionMs) {
    const pos = Number(positionMs) || 0;
    const displayed = Number(displayPositionMs) || 0;
    if (!this._model) {
      this._renderClock(displayed);
      return;
    }
    const state = lyricsAt(this._model, pos);
    if (state.index !== this._activeIndex && this._model.type !== 'Static') {
      this._activeIndex = state.index;
      this._lineRefs.forEach((ref, i) => {
        const distance = state.index < 0 ? i : Math.abs(i - state.index);
        ref.el.classList.toggle('is-active', i === state.index);
        ref.el.classList.toggle('is-near', distance === 1);
        ref.el.classList.toggle('is-far', distance > 4);
        ref.segments.forEach((segment) => segment.el.classList.remove('is-active'));
      });
    }
    if (state.line) {
      const activeSegment = state.type === 'Syllable'
        ? state.line.syllables[state.syllableIndex] : null;
      const fill = state.type === 'Syllable' && activeSegment
        ? Math.min(1, Math.max(0, (pos - activeSegment.startMs) / Math.max(1, activeSegment.endMs - activeSegment.startMs)))
        : state.fill;
      const percent = Math.round(fill * 100);
      if (percent !== this._lastFill) {
        this._lastFill = percent;
        this._lineRefs[state.index]?.segments.forEach((segment, i) => {
          if (state.type === 'Syllable') {
            const value = pos >= segment.endMs ? 100 : (pos >= segment.startMs && i === state.syllableIndex ? percent : 0);
            segment.fill.style.setProperty('--fill', `${value}%`);
            segment.el.classList.toggle('is-active', value > 0);
          } else {
            segment.fill.style.setProperty('--fill', `${percent}%`);
          }
        });
      }
    }
    for (const ref of this._bgRefs) {
      const fill = pos >= ref.endMs ? 100 : (pos >= ref.startMs ? Math.round(((pos - ref.startMs) / Math.max(1, ref.endMs - ref.startMs)) * 100) : 0);
      ref.el.style.setProperty('--fill', `${Math.min(100, fill)}%`);
    }
    this._renderClock(displayed);
  }

  _renderClock(displayed) {
    const elapsed = Math.floor(displayed / 1000);
    if (elapsed === this._lastElapsed) return;
    this._lastElapsed = elapsed;
    this._q('.elapsed').textContent = formatTime(displayed);
    const seek = this._q('.seek');
    if (this.root.activeElement !== seek) seek.value = String(Math.min(Number(seek.max), elapsed));
  }

  setPosition(ms, durationMs) {
    if (Number.isFinite(durationMs) && durationMs > 0) this.setDuration(durationMs);
    this.render(ms);
  }

  destroy() {
    clearTimeout(this._hideTimer);
    this.root.removeEventListener('click', this._onClick);
    this.root.removeEventListener('input', this._onInput);
    this.root.removeEventListener('change', this._onChange);
    this.root.removeEventListener('keydown', this._onKeydown);
    this.root.removeEventListener('pointerdown', this._onPointer);
  }
}

export { STAGE_CSS };
