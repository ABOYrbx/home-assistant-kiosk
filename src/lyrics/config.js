// Pure Lovelace card configuration validation. Structural errors are thrown by
// setConfig (Home Assistant shows them on the card); missing secrets are a
// setup state on the stage, never a thrown error or a blank screen.

const OFFSET_LIMIT = 2000;
const OFFSET_STEP = 50;

function snapOffset(value) {
  const clamped = Math.min(OFFSET_LIMIT, Math.max(-OFFSET_LIMIT, value));
  return Math.round(clamped / OFFSET_STEP) * OFFSET_STEP;
}

function isHttpUrl(value) {
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:') && Boolean(url.hostname);
  } catch {
    return false;
  }
}

export function validateConfig(raw) {
  const input = raw && typeof raw === 'object' ? raw : {};
  const problems = [];
  const setup = [];

  if (typeof input.ma_url !== 'string' || input.ma_url.trim() === '') {
    setup.push('ma_url is missing. Add the Music Assistant server URL.');
  } else if (!isHttpUrl(input.ma_url.trim())) {
    problems.push('ma_url must be an http(s) URL.');
  }

  if (typeof input.ma_token !== 'string' || input.ma_token.trim() === '') {
    setup.push('ma_token is missing. Create a long-lived token in Music Assistant -> Settings -> Profile.');
  }

  const spicyKey = typeof input.spicy_key === 'string' ? input.spicy_key.trim() : '';
  if (spicyKey === '' || spicyKey === 'sl_pk_replace_me') {
    setup.push('spicy_key is missing. Use your publishable Spicy Lyrics key (sl_pk_...).');
  } else if (!/^sl_pk_[A-Za-z0-9_-]+$/.test(spicyKey)) {
    setup.push('spicy_key must be a publishable key starting with sl_pk_. A secret key must never ship to a browser.');
  }

  const offsetRaw = input.offset_ms ?? 0;
  const offset = typeof offsetRaw === 'number' ? offsetRaw : Number(offsetRaw);
  if (!Number.isFinite(offset)) {
    problems.push('offset_ms must be a number of milliseconds.');
  }

  const player = typeof input.player === 'string' ? input.player.trim() : '';
  const backPath = typeof input.back_path === 'string' && input.back_path.trim() !== ''
    ? input.back_path.trim() : '/lovelace/0';
  if (!backPath.startsWith('/')) problems.push('back_path must start with "/".');
  if (input.fullscreen !== undefined && typeof input.fullscreen !== 'boolean') {
    problems.push('fullscreen must be true or false.');
  }

  return {
    ok: problems.length === 0,
    problems,
    setup,
    config: {
      maUrl: typeof input.ma_url === 'string' ? input.ma_url.trim() : '',
      maToken: typeof input.ma_token === 'string' ? input.ma_token.trim() : '',
      player,
      spicyKey,
      offsetMs: Number.isFinite(offset) ? snapOffset(offset) : 0,
      backPath,
      fullscreen: input.fullscreen !== false,
    },
  };
}
