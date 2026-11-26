// Pure timing, player-selection and pending-action logic for the live card.
// No DOM, no network, no Music Assistant imports: fully unit-testable.

const STALE_AFTER_MS = 30_000;

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function nowMs() {
  return typeof performance !== 'undefined' && performance.now
    ? performance.now() : Date.now();
}

/**
 * Position anchored to MA's queue time and extrapolated with performance.now().
 * Interpolation stops on pause, freeze, and disconnect; every reconnect or seek
 * re-anchors. The offset is applied only when reading the lyric position.
 */
export class PositionAnchor {
  constructor({ now = nowMs } = {}) {
    this.now = now;
    this.anchorMs = 0;
    this.anchorAt = this.now();
    this.playing = false;
    this.frozen = false;
    this.durationMs = 0;
    this.offsetMs = 0;
  }

  sync(ms, { playing = this.playing, at = this.now() } = {}) {
    const duration = this.durationMs > 0 ? this.durationMs : Infinity;
    this.anchorMs = clamp(Number(ms) || 0, 0, duration);
    this.anchorAt = at;
    this.playing = Boolean(playing);
  }

  setPlaying(playing, at = this.now()) {
    const position = this.positionMs(at);
    this.playing = Boolean(playing);
    this.anchorMs = position;
    this.anchorAt = at;
  }

  setDuration(ms) {
    this.durationMs = Number(ms) > 0 ? Number(ms) : 0;
    this.anchorMs = clamp(this.anchorMs, 0, this.durationMs || Infinity);
  }

  setOffset(ms) {
    const value = Number(ms);
    this.offsetMs = Number.isFinite(value) ? clamp(Math.round(value), -2000, 2000) : 0;
  }

  freeze(at = this.now()) {
    if (this.frozen) return;
    this.anchorMs = this.positionMs(at);
    this.anchorAt = at;
    this.frozen = true;
  }

  unfreeze(at = this.now()) {
    if (!this.frozen) return;
    this.frozen = false;
    this.anchorAt = at;
  }

  positionMs(at = this.now()) {
    if (!this.playing || this.frozen) return this.anchorMs;
    const delta = Math.max(0, at - this.anchorAt);
    const duration = this.durationMs > 0 ? this.durationMs : Infinity;
    return clamp(this.anchorMs + delta, 0, duration);
  }

  /** Position used for the lyric lookup, after the user offset. */
  lyricMs(at = this.now()) {
    const duration = this.durationMs > 0 ? this.durationMs : Infinity;
    return clamp(this.positionMs(at) - this.offsetMs, 0, duration);
  }

  isStale(at = this.now()) {
    return this.playing && !this.frozen && (at - this.anchorAt) > STALE_AFTER_MS;
  }
}

function norm(value) {
  return typeof value === 'string' ? value.trim().toLocaleLowerCase() : '';
}

/** A new uri, title, artist, or duration on either side means a new track. */
export function detectTrackChange(previous, next) {
  if (!previous || !next) return Boolean(next && !previous);
  if (norm(previous.uri) !== norm(next.uri)) return true;
  if (previous.title && next.title && norm(previous.title) !== norm(next.title)) return true;
  if (previous.artist && next.artist && norm(previous.artist) !== norm(next.artist)) return true;
  const a = Number(previous.durationMs);
  const b = Number(next.durationMs);
  if (a > 0 && b > 0 && Math.abs(a - b) > 1000) return true;
  return false;
}

/** Map MA player/queue snapshots into the track shape the stage consumes. */
export function trackFromMedia(media, queue = null) {
  const item = media ?? queue?.current_item?.media_item ?? null;
  if (!item) return null;
  const durationSeconds = Number(queue?.current_item?.duration ?? item.duration);
  return {
    uri: String(item.uri ?? ''),
    title: String(item.title ?? item.name ?? ''),
    artist: String(item.artist ?? ''),
    album: String(item.album ?? ''),
    imageUrl: String(item.image_url ?? ''),
    durationMs: Number.isFinite(durationSeconds) && durationSeconds > 0 ? durationSeconds * 1000 : 0,
  };
}

function isPlaying(player) {
  return ['playing', 'buffering'].includes(norm(player?.playback_state));
}

/** Pin the configured player; otherwise prefer playing, then last active. */
export function resolvePlayer(players, { configured = '', persistedId = '' } = {}) {
  const list = Array.isArray(players) ? players : [...(players?.values?.() ?? [])];
  const available = list.filter((p) => p && p.available !== false && p.enabled !== false && p.hide_in_ui !== true);
  if (configured) {
    const needle = norm(configured);
    return available.find((p) => norm(p.player_id) === needle
      || norm(p.name) === needle || norm(p.display_name) === needle) ?? null;
  }
  if (persistedId) {
    const pinned = available.find((p) => norm(p.player_id) === norm(persistedId));
    if (pinned) return pinned;
  }
  const playing = available.filter(isPlaying);
  if (playing.length > 0) return playing[0];
  const active = available.filter((p) => p.current_media || p.powered !== false || isPlaying(p));
  return active.length > 0 ? active[active.length - 1] : (available[0] ?? null);
}

/** Only disable a control when the server model clearly withholds it. */
export function mapCapabilities(player, queue = null, positionMs = 0) {
  const available = Boolean(player && player.available !== false && player.powered !== false);
  const hasQueue = Boolean(queue?.queue_id);
  return {
    play: available && hasQueue,
    previous: available && hasQueue,
    next: available && hasQueue,
    seek: available && hasQueue && Number(queue?.current_item?.duration ?? player?.current_media?.duration) > 0,
    volume: available && typeof player?.volume_level === 'number',
    mute: available && typeof player?.volume_muted === 'boolean',
  };
}

/** Commands are pending until the confirming MA event arrives (or it times out). */
export class PendingActions {
  constructor({ now = nowMs, timeoutMs = 5000 } = {}) {
    this.now = now;
    this.timeoutMs = timeoutMs;
    this.actions = new Map();
  }

  begin(action) {
    this.actions.set(action, this.now() + this.timeoutMs);
  }

  end(action) {
    this.actions.delete(action);
  }

  has(action) {
    const deadline = this.actions.get(action);
    if (deadline === undefined) return false;
    if (this.now() >= deadline) { this.actions.delete(action); return false; }
    return true;
  }

  get busy() {
    for (const action of [...this.actions.keys()]) this.has(action);
    return this.actions.size > 0;
  }

  clear() {
    this.actions.clear();
  }
}
