// Home Assistant Lovelace card: lyrics-stage-card.
// HA supplies the dashboard shell and the `hass` object; Music Assistant is the
// source of truth for playback, Spicy is the source for lyrics. Nothing here
// imports HA frontend internals, stores a HA token, or navigates on its own.

import { LyricsStage } from './stage.js';
import { SpicyClient } from './api.js';
import { validateConfig } from './config.js';
import { parseSpotifyId, isExactResult } from './parse.js';
import { MaAdapter, safeError } from './ma.js';
import {
  PositionAnchor, detectTrackChange, trackFromMedia, resolvePlayer,
  mapCapabilities, PendingActions,
} from './live.js';

const STORAGE = {
  key: 'lyrics-stage:spicy-key',
  offset: 'lyrics-stage:offset-ms',
  player: 'lyrics-stage:player',
};

function readLocal(name) {
  try { return localStorage.getItem(name) ?? ''; } catch { return ''; }
}

function writeLocal(name, value) {
  try { localStorage.setItem(name, String(value)); } catch { /* private mode */ }
}

function removeLocal(name) {
  try { localStorage.removeItem(name); } catch { /* private mode */ }
}

export class LyricsStageCard extends HTMLElement {
  constructor() {
    super();
    this._config = null;
    this._hass = null;
    this._running = false;
    this._disposed = false;
    this._raf = 0;
    this._volumeTimer = null;
    this._fetchController = null;
    this._retry = null;
    this._track = null;
    this._lastAnchorMs = 0;
    this._navigated = false;
  }

  setConfig(config) {
    const result = validateConfig(config);
    if (!result.ok) throw new Error(`Invalid lyrics-stage-card config: ${result.problems.join('; ')}`);
    this._config = result.config;
    if (this.isConnected) this._restart();
  }

  // HA calls this on every state change. Keep it cheap and side-effect free.
  set hass(hass) {
    this._hass = hass;
  }

  connectedCallback() {
    if (!this.shadowRoot) {
      const shadow = this.attachShadow({ mode: 'open' });
      this._stage = new LyricsStage(shadow, {
        fullscreen: this._config?.fullscreen ?? true,
        onControl: (action, detail) => this._onControl(action, detail),
      });
    }
    if (this._config) this._restart();
  }

  disconnectedCallback() {
    this._teardown();
  }

  getCardSize() {
    return 1;
  }

  static get stubConfig() {
    return {
      type: 'custom:lyrics-stage-card',
      ma_url: 'http://music-assistant.local:8095',
      ma_token: '',
      spicy_key: '',
      player: '',
      offset_ms: 0,
      back_path: '/lovelace/0',
      fullscreen: true,
    };
  }

  _teardown() {
    this._running = false;
    cancelAnimationFrame(this._raf);
    clearTimeout(this._volumeTimer);
    this._fetchController?.abort();
    this._fetchController = null;
    this._unsubscribeSnapshot?.();
    this._unsubscribeConnection?.();
    document.removeEventListener('visibilitychange', this._onVisibility);
    this._ma?.stop();
    this._ma = null;
    this._pending?.clear();
    if (this._stage) {
      this._stage.setPending(null);
      this._stage.setPlaying(false);
    }
  }

  _restart() {
    this._teardown();
    this._running = true;
    if (this._stage) {
      this._stage.root.host.classList.toggle('stage--fullscreen', this._config.fullscreen);
    }

    const key = this._config.spicyKey || readLocal(STORAGE.key);
    const offset = Number(readLocal(STORAGE.offset));
    const offsetMs = Number.isFinite(offset) ? offset : this._config.offsetMs;
    this._api = new SpicyClient({ key });
    this._anchor = new PositionAnchor();
    this._anchor.setOffset(offsetMs);
    this._pending = new PendingActions();
    this._stage.setOffset(offsetMs);
    this._stage.setKey(key.startsWith('sl_pk_') ? key : '');
    this._stage.setTrack(null);
    this._stage.setLyrics(null);
    this._stage.setConnection(this._config.maToken ? 'connecting' : 'offline');

    this._retry = null;
    this._track = null;
    this._lastAnchorMs = 0;

    // A key saved in this browser satisfies the config requirement too.
    const setup = this._api.hasKey
      ? this._config.setup.filter((line) => !line.includes('spicy_key'))
      : [...this._config.setup, 'Lyrics key is missing. Open the settings gear to add one.'];
    if (setup.length > 0) {
      this._stage.setStatus({ kind: 'setup', message: setup.join(' ') });
      return;
    }

    this._ma = new MaAdapter({ url: this._config.maUrl, token: this._config.maToken });
    this._unsubscribeSnapshot = this._ma.onSnapshot((snapshot) => this._onSnapshot(snapshot));
    this._unsubscribeConnection = this._ma.onConnection((state) => this._onConnection(state));
    this._onVisibility = () => this._onVisible();
    document.addEventListener('visibilitychange', this._onVisibility);
    this._ma.start();
    this._tick();
  }

  _onConnection(state) {
    if (!this._running) return;
    const online = state === 'connected';
    if (online) {
      this._anchor.unfreeze();
      const snapshot = this._ma.snapshot();
      this._anchor.sync(snapshot.elapsedMs, { playing: this._isPlaying(snapshot.player) });
      this._ma.resync(snapshot.queue?.queue_id).catch(() => {});
    } else if (state !== 'stopped') {
      this._anchor.freeze();
    }
    this._stage.setConnection(state === 'connected' ? 'connected' : state);
    this._stage.setStale(false);
  }

  _isPlaying(player) {
    return ['playing', 'buffering'].includes(String(player?.playback_state ?? '').toLowerCase());
  }

  _onSnapshot(snapshot) {
    if (!this._running) return;
    const player = resolvePlayer(snapshot.players, {
      configured: this._config.player,
      persistedId: readLocal(STORAGE.player),
    });
    if (!player) {
      this._stage.setPlayer('', { play: false, previous: false, next: false, seek: false, volume: false, mute: false });
      this._stage.setStatus({ kind: 'idle', message: 'Selected player is unavailable or off.' });
      this._stage.setPlaying(false);
      return;
    }
    if (player.player_id !== this._ma.playerId) {
      this._ma.selectPlayerId(player.player_id);
      writeLocal(STORAGE.player, player.player_id);
    }

    const queue = snapshot.queue;
    const playing = this._isPlaying(player);
    const track = trackFromMedia(player.current_media, queue);
    const capabilities = mapCapabilities(player, queue, this._anchor.positionMs());
    this._stage.setPlayer(player.display_name ?? player.name, capabilities);
    this._stage.setVolume(player.volume_level ?? 0, player.volume_muted === true);
    this._stage.setPlaying(playing);
    const durationMs = Math.max(track?.durationMs ?? 0, (queue?.current_item?.duration ?? 0) * 1000);
    if (durationMs > 0) {
      this._anchor.setDuration(durationMs);
      this._stage.setDuration(durationMs);
    }

    // Keep latency below 150 ms by resyncing whenever MA sends a fresh anchor.
    const fresh = snapshot.elapsedMs;
    if (!this._anchor.frozen && Math.abs(fresh - this._anchor.positionMs()) > 150) {
      this._anchor.sync(fresh, { playing });
    } else if (this._anchor.playing !== playing) {
      this._anchor.setPlaying(playing);
    }

    if (detectTrackChange(this._track, track)) this._onTrackChange(track);
  }

  _onTrackChange(track) {
    this._fetchController?.abort();
    this._fetchController = new AbortController();
    this._track = track;
    this._anchor.sync(0, { playing: this._anchor.playing });
    this._stage.setTrack(track);
    this._stage.setLyrics(null);
    this._lastAnchorMs = 0;

    if (!track || !track.title) {
      this._stage.setStatus({ kind: 'idle', message: 'Waiting for playback. Nothing is playing right now.' });
      this._retry = null;
      return;
    }
    this._stage.setStatus({ kind: 'loading', message: 'Loading lyrics...' });
    this._retry = () => this._loadLyrics(track);
    this._loadLyrics(track);
  }

  async _loadLyrics(track) {
    const signal = this._fetchController?.signal;
    const key = this._track === track ? track : null;
    if (!key) return; // a newer track won the race
    try {
      let id = parseSpotifyId(track.uri);
      if (!id) {
        const duration = track.durationMs > 0 ? track.durationMs / 1000 : 0;
        const results = await this._api.searchTracks({
          title: track.title, artist: track.artist, album: track.album, duration,
        }, { signal });
        const exact = results.find((result) => isExactResult(result, track));
        if (!exact) {
          this._stage.setStatus({ kind: 'no_lyrics', message: 'No synced lyrics for this track.' });
          return;
        }
        id = exact.id;
      }
      const model = await this._api.getLyrics(id, { signal });
      if (this._track !== track || !this._running) return;
      this._stage.setLyrics(model);
    } catch (error) {
      if (signal?.aborted || !this._running) return;
      const message = safeError(error);
      const retryable = !['not_found', 'missing_key', 'unauthorized', 'forbidden'].includes(error?.kind);
      this._stage.setStatus({
        kind: error?.kind ?? 'error',
        message,
        retry: retryable ? () => this._loadLyrics(track) : null,
      });
    }
  }

  _onVisible() {
    if (!this._running) return;
    if (document.hidden) {
      cancelAnimationFrame(this._raf);
      this._raf = 0;
      return;
    }
    const snapshot = this._ma?.snapshot();
    if (snapshot && !this._anchor.frozen) {
      this._anchor.sync(snapshot.elapsedMs, { playing: this._isPlaying(snapshot.player) });
      this._ma.resync(snapshot.queue?.queue_id).catch(() => {});
    }
    if (!this._raf) this._tick();
  }

  _tick() {
    if (!this._running || document.hidden) return;
    const position = this._anchor.positionMs();
    // One renderer call per frame: offset is applied before the lyric lookup,
    // while the clock label keeps showing MA's real position.
    this._stage.render(this._anchor.lyricMs(), position);
    this._stage.setStale(this._anchor.isStale());
    if (Math.abs(position - this._lastAnchorMs) >= 1000) this._lastAnchorMs = position;
    this._raf = requestAnimationFrame(() => this._tick());
  }

  _navigateBack() {
    if (this._navigated) return;
    this._navigated = true;
    if (typeof history !== 'undefined' && history.length > 1) {
      history.back();
    } else if (typeof location !== 'undefined') {
      location.assign(this._config.backPath);
    }
  }

  async _runCommand(action, work, detail = {}) {
    if (!this._ma || this._pending.has(action) || this._pending.busy) return;
    this._pending.begin(action);
    this._stage.setPending(action);
    try {
      const result = await work();
      if (!this._running) return;
      if (result?.confirmed === false) {
        this._stage.setStatus({ kind: 'error', message: `${action} was not confirmed by Music Assistant.`, retry: null });
      }
    } catch (error) {
      if (this._running) {
        this._stage.setStatus({ kind: 'error', message: safeError(error), retry: null });
      }
    } finally {
      this._pending.end(action);
      if (this._running) this._stage.setPending(null);
    }
  }

  _onControl(action, detail) {
    if (!this._running && !['back', 'settings'].includes(action)) return;
    const snapshot = this._ma?.snapshot();
    const player = snapshot?.player;
    const queueId = snapshot?.queue?.queue_id ?? player?.player_id;
    switch (action) {
      case 'back':
        this._navigateBack();
        break;
      case 'settings':
        this._stage.toggleSettings();
        break;
      case 'close_settings':
        this._stage.toggleSettings(false);
        break;
      case 'play_pause':
        this._runCommand(action, () => this._ma.playPause(queueId, String(player?.playback_state ?? '')));
        break;
      case 'next':
        this._runCommand(action, () => this._ma.next(queueId, this._track?.uri ?? ''));
        break;
      case 'previous':
        this._runCommand(action, () => this._ma.previous(queueId, this._track?.uri ?? ''));
        break;
      case 'seek':
        this._runCommand(action, () => this._ma.seek(queueId, detail.ms, this._anchor.positionMs()));
        break;
      case 'volume':
        // Debounced; the final value is sent on release via volume_commit.
        clearTimeout(this._volumeTimer);
        this._volumeTimer = setTimeout(() => {
          this._runCommand('volume_commit', () => this._ma.setVolume(player?.player_id, detail.level));
        }, 150);
        break;
      case 'volume_commit':
        clearTimeout(this._volumeTimer);
        this._runCommand('volume', () => this._ma.setVolume(player?.player_id, detail.level));
        break;
      case 'mute':
        this._runCommand(action, () => this._ma.setMute(player?.player_id, player?.volume_muted !== true));
        break;
      case 'offset_minus':
      case 'offset_plus': {
        const delta = action === 'offset_plus' ? 50 : -50;
        const value = Math.max(-2000, Math.min(2000, (Number(readLocal(STORAGE.offset)) || this._config.offsetMs) + delta));
        this._anchor.setOffset(value);
        this._stage.setOffset(value);
        writeLocal(STORAGE.offset, value);
        break;
      }
      case 'key_input':
        // Held only in the password field until Save is pressed.
        this._pendingKey = detail.key;
        break;
      case 'save_key': {
        const key = String(this._pendingKey ?? '').trim();
        if (/^sl_pk_[A-Za-z0-9_-]+$/.test(key)) {
          writeLocal(STORAGE.key, key);
          this._stage.setStatus(null);
          this._restart();
        } else {
          this._stage.setStatus({ kind: 'setup', message: 'Enter a publishable key starting with sl_pk_.' });
        }
        break;
      }
      case 'clear_key':
        removeLocal(STORAGE.key);
        this._stage.setKey('');
        this._stage.setStatus({ kind: 'setup', message: 'Key cleared. Add a publishable key to load lyrics.' });
        this._api = new SpicyClient({ key: '' });
        break;
      case 'clear_cache':
        this._api?.clearCache();
        if (this._track?.title) this._onTrackChange(this._track);
        break;
      case 'retry':
        this._retry?.();
        break;
      default:
        break;
    }
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('lyrics-stage-card')) {
  customElements.define('lyrics-stage-card', LyricsStageCard);
}
