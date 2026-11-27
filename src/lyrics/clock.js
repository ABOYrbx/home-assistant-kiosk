// A demo transport clock. It owns no DOM and no lyrics: a later player adapter
// can implement the same interface and the stage will not change.

const OFFSET_LIMIT_MS = 2000;
const OFFSET_STEP_MS = 50;

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

export const DEFAULT_SCHEDULER = {
  now() {
    return typeof performance !== 'undefined' && performance.now
      ? performance.now()
      : Date.now();
  },
  requestFrame(callback) {
    if (typeof requestAnimationFrame === 'function') return requestAnimationFrame(callback);
    return setTimeout(() => callback(DEFAULT_SCHEDULER.now()), 16);
  },
  cancelFrame(handle) {
    if (typeof cancelAnimationFrame === 'function') cancelAnimationFrame(handle);
    else clearTimeout(handle);
  },
};

/**
 * @typedef {{now: () => number, requestFrame: (cb: () => void) => *,
 *   cancelFrame: (handle: *) => void}} Scheduler
 */

export class Clock {
  #scheduler;
  #notify;

  /**
   * @param {{duration?: number, offsetMs?: number, scheduler?: Scheduler,
   *   onUpdate?: (clock: Clock) => void}} options
   */
  constructor({
    duration = 0,
    offsetMs = 0,
    scheduler = DEFAULT_SCHEDULER,
    onUpdate = () => {},
  } = {}) {
    this.#scheduler = scheduler;
    this.#notify = onUpdate;
    this._duration = Math.max(0, Number(duration) || 0);
    this._offsetMs = this._clampOffset(offsetMs);
    this._rawMs = 0;
    this._playing = false;
    this._suspended = false;
    this._anchorRaw = 0;
    this._anchorAt = 0;
    this._frame = null;
    this._tick = this._tick.bind(this);
  }

  /** Raw transport position in milliseconds (without the sync offset). */
  get position() {
    return this._rawMs;
  }

  /** Position used by the lyrics renderer: raw position plus sync offset. */
  get effectivePosition() {
    return clamp(this._rawMs + this._offsetMs, 0, this._duration);
  }

  get duration() {
    return this._duration;
  }

  get playing() {
    return this._playing;
  }

  get offsetMs() {
    return this._offsetMs;
  }

  get suspended() {
    return this._suspended;
  }

  _clampOffset(value) {
    const number = Number(value);
    if (!Number.isFinite(number)) return 0;
    return clamp(Math.round(number / OFFSET_STEP_MS) * OFFSET_STEP_MS, -OFFSET_LIMIT_MS, OFFSET_LIMIT_MS);
  }

  _emit() {
    this.#notify(this);
  }

  _anchor() {
    this._anchorRaw = this._rawMs;
    this._anchorAt = this.#scheduler.now();
  }

  _capture() {
    if (!this._playing || this._suspended) return;
    this._rawMs = clamp(
      this._anchorRaw + (this.#scheduler.now() - this._anchorAt),
      0,
      this._duration,
    );
  }

  _schedule() {
    if (!this._playing || this._suspended || this._frame !== null) return;
    this._frame = this.#scheduler.requestFrame(this._tick);
  }

  _cancel() {
    if (this._frame === null) return;
    this.#scheduler.cancelFrame(this._frame);
    this._frame = null;
  }

  _tick() {
    this._frame = null;
    if (!this._playing || this._suspended) return;

    const next = clamp(
      this._anchorRaw + (this.#scheduler.now() - this._anchorAt),
      0,
      this._duration,
    );
    if (this._duration > 0 && next >= this._duration) {
      this._rawMs = this._duration;
      this._playing = false;
      this._emit();
      return;
    }
    this._rawMs = next;
    this._emit();
    this._schedule();
  }

  setDuration(ms) {
    const value = Number(ms);
    this._duration = Number.isFinite(value) ? Math.max(0, value) : 0;
    this._rawMs = clamp(this._rawMs, 0, this._duration);
    this._anchor();
    this._emit();
  }

  play() {
    if (this._playing) return;
    if (this._duration > 0 && this._rawMs >= this._duration) this._rawMs = 0;
    this._playing = true;
    this._anchor();
    this._schedule();
    this._emit();
  }

  pause() {
    if (!this._playing) return;
    this._capture();
    this._playing = false;
    this._cancel();
    this._emit();
  }

  toggle() {
    if (this._playing) this.pause();
    else this.play();
  }

  seek(ms) {
    const value = Number(ms);
    this._rawMs = Number.isFinite(value) ? clamp(value, 0, this._duration) : 0;
    this._anchor();
    this._emit();
  }

  restart() {
    this.seek(0);
  }

  setOffset(ms) {
    const next = this._clampOffset(ms);
    if (next === this._offsetMs) return;
    this._offsetMs = next;
    this._emit();
  }

  /** Stop advancing without changing the playing state (hidden tab). */
  suspend() {
    if (this._suspended) return;
    this._capture();
    this._suspended = true;
    this._cancel();
    this._emit();
  }

  /** Resume after a suspension without jumping the playhead. */
  resume() {
    if (!this._suspended) return;
    this._suspended = false;
    if (this._playing) {
      this._anchor();
      this._schedule();
    }
    this._emit();
  }

  destroy() {
    this._playing = false;
    this._cancel();
    this.#notify = () => {};
  }
}

export const CLOCK_LIMITS = Object.freeze({
  OFFSET_LIMIT_MS,
  OFFSET_STEP_MS,
});
