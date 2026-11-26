// Thin, replaceable boundary around music-assistant-client. It maps the live
// player/queue state into plain snapshots, confirms commands from events, and
// makes sure neither the token nor the authenticated URL can reach a log.

import { MusicAssistantClient } from 'music-assistant-client';

const SILENT_LOGGER = {
  debug() {}, info() {}, warn() {}, error() {},
};

/** http -> ws, https -> wss, with the MA WebSocket path. Used for docs/tests. */
export function deriveWsUrl(baseUrl) {
  try {
    const url = new URL(baseUrl);
    url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    url.pathname = '/ws';
    url.search = '';
    url.hash = '';
    return url.toString();
  } catch {
    return '';
  }
}

/** Strip userinfo and query/hash from any URL before displaying or logging it. */
export function redactUrl(value) {
  try {
    const url = new URL(value);
    url.username = '';
    url.password = '';
    url.search = '';
    url.hash = '';
    return url.toString();
  } catch {
    return '<invalid-url>';
  }
}

const SECRET_PATTERN = /(?:sl_[ps]k_[A-Za-z0-9_-]+|[A-Za-z0-9_-]{32,})/g;

/** Never print a token, key, or opaque id from an exception. */
export function safeError(error) {
  const message = String(error?.message ?? error ?? 'Unknown error');
  return message.replace(SECRET_PATTERN, '<redacted>');
}

export class MaAdapter {
  constructor({
    url,
    token,
    clientFactory = (options) => new MusicAssistantClient(options),
    logger = SILENT_LOGGER,
  } = {}) {
    this.url = String(url ?? '');
    this.client = clientFactory({ url: this.url, token: String(token ?? ''), logger });
    this.playerId = '';
    this.unsubscribers = [];
    this.listeners = new Set();
    this.connectionListeners = new Set();
    this.connection = 'stopped';
    this.started = false;
  }

  start() {
    if (this.started) return;
    this.started = true;
    this.unsubscribers.push(
      this.client.onStateChange((state) => {
        this.connection = state;
        for (const listener of this.connectionListeners) listener(state);
        this._notify();
      }),
    );
    for (const event of ['player_added', 'player_updated', 'player_removed',
      'queue_added', 'queue_updated', 'queue_items_updated', 'queue_time_updated',
      'media_item_updated']) {
      this.unsubscribers.push(this.client.on(event, () => this._notify()));
    }
    this.client.start();
  }

  stop() {
    for (const unsubscribe of this.unsubscribers.splice(0)) {
      try { unsubscribe(); } catch { /* ignore */ }
    }
    this.listeners.clear();
    this.connectionListeners.clear();
    if (this.started) this.client.stop();
    this.started = false;
  }

  onSnapshot(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  onConnection(listener) {
    this.connectionListeners.add(listener);
    return () => this.connectionListeners.delete(listener);
  }

  _notify() {
    for (const listener of this.listeners) listener(this.snapshot());
  }

  selectPlayerId(id) {
    this.playerId = String(id ?? '');
    this._notify();
  }

  snapshot() {
    const players = [...(this.client.players?.values?.() ?? [])];
    const queues = [...(this.client.queues?.values?.() ?? [])];
    const player = players.find((p) => p.player_id === this.playerId) ?? null;
    const queue = player
      ? queues.find((q) => q.queue_id === (player.active_source ?? player.player_id)) ?? null
      : null;
    const elapsedSeconds = Number(queue?.elapsed_time ?? player?.elapsed_time ?? 0);
    return {
      connection: this.connection,
      connected: this.client.isConnected === true,
      players,
      queues,
      player,
      queue,
      elapsedMs: Number.isFinite(elapsedSeconds) ? elapsedSeconds * 1000 : 0,
      playerName: player?.display_name ?? player?.name ?? '',
    };
  }

  /**
   * Register event listeners first, then send the command, then wait for the
   * server event. A command that is never confirmed is reported as such.
   */
  async _confirmed(send, predicate, events = ['queue_updated', 'player_updated', 'queue_time_updated']) {
    let resolveResult;
    const result = new Promise((resolve) => { resolveResult = resolve; });
    let done = false;
    let subscriptions = [];
    let timer = null;
    const finish = (confirmed) => {
      if (done) return;
      done = true;
      for (const off of subscriptions) { try { off(); } catch { /* ignore */ } }
      if (timer !== null) clearTimeout(timer);
      resolveResult({ confirmed });
    };
    subscriptions = events.map((event) => this.client.on(event, (data, objectId) => {
      if (predicate(data, objectId)) finish(true);
    }));
    timer = setTimeout(() => finish(false), 5000);
    try {
      await send();
    } catch (error) {
      finish(false);
      throw error;
    }
    return result;
  }

  async playPause(queueId, beforeState = '') {
    return this._confirmed(
      () => this.client.playPause(queueId),
      (data) => {
        const state = data?.state ?? data?.playback_state;
        return Boolean(state) && state !== beforeState;
      },
    );
  }

  async next(queueId, beforeUri = '') {
    return this._confirmed(
      () => this.client.next(queueId),
      (data) => {
        const uri = data?.current_item?.media_item?.uri ?? data?.current_media?.uri;
        return Boolean(uri) && uri !== beforeUri;
      },
    );
  }

  async previous(queueId, beforeUri = '') {
    return this._confirmed(
      () => this.client.previous(queueId),
      (data) => {
        const uri = data?.current_item?.media_item?.uri ?? data?.current_media?.uri;
        return Boolean(uri) && uri !== beforeUri;
      },
    );
  }

  async seek(queueId, ms, beforeMs = 0) {
    const target = Math.max(0, ms) / 1000;
    return this._confirmed(
      () => this.client.seek(queueId, target),
      (data, objectId) => {
        const elapsed = typeof data === 'number' ? data : Number(data?.elapsed_time);
        return (objectId === queueId || objectId === null || objectId === undefined)
          && Number.isFinite(elapsed) && Math.abs(elapsed - target) < 3
          && Math.abs(target - beforeMs / 1000) >= 3;
      },
    );
  }

  async setVolume(playerId, level) {
    return this._confirmed(
      () => this.client.setVolume(playerId, level),
      (data) => Number(data?.volume_level) === level,
      ['player_updated'],
    );
  }

  async setMute(playerId, muted) {
    return this._confirmed(
      () => this.client.setMute(playerId, muted),
      (data) => data?.volume_muted === muted,
      ['player_updated'],
    );
  }

  async resync(queueId) {
    if (typeof this.client.resync === 'function') await this.client.resync(queueId);
  }
}
