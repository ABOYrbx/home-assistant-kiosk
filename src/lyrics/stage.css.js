// Stage styles live in one JS module so the shadow-root card and the design
// prototype share a single source of truth. Values mirror tokens.css; they are
// repeated here because a Home Assistant shadow root cannot load tokens.css.
// Override any --stage-* custom property to retheme the stage.

export const STAGE_CSS = `
:host {
  --stage-bg: #08090b;              /* tokens.css: near-black cinema exception */
  --stage-fg: #ffffff;
  --stage-muted: rgba(255, 255, 255, 0.72);
  --stage-dim: rgba(255, 255, 255, 0.46);
  --stage-line: rgba(255, 255, 255, 0.16);
  --stage-surface: rgba(12, 13, 16, 0.88);
  --stage-danger: #ff9b9b;
  --stage-text-xs: 12px;
  --stage-text-sm: 14px;
  --stage-text-base: 16px;
  --stage-text-lg: 18px;
  --stage-text-xl: 22px;
  --stage-text-2xl: 32px;
  --stage-text-3xl: 48px;
  --stage-text-4xl: 64px;
  --stage-1: 4px;
  --stage-2: 8px;
  --stage-3: 12px;
  --stage-4: 16px;
  --stage-6: 24px;
  --stage-8: 32px;
  --stage-12: 48px;
  --stage-radius: 8px;
  --stage-touch: 44px;
  --stage-fast: 140ms;
  --stage-base: 220ms;
  --stage-ease: cubic-bezier(0.2, 0, 0, 1);
  display: block;
  width: 100%;
  height: 100%;
  color: var(--stage-fg);
  font-family: Inter, system-ui, sans-serif;
  font-size: var(--stage-text-base);
  line-height: 1.55;
  background: var(--stage-bg);
}
:host(.stage--fullscreen) {
  position: fixed;
  inset: 0;
  z-index: 2147483647;
  width: 100vw;
  height: 100vh;
}
* { box-sizing: border-box; }
.stage {
  position: absolute;
  inset: 0;
  overflow: hidden;
  isolation: isolate;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  background: var(--stage-bg);
}
.bg {
  position: absolute;
  inset: -10%;
  z-index: -3;
  background-position: center;
  background-size: cover;
  filter: blur(48px) saturate(0.7) brightness(0.55);
  transform: scale(1.1);
  animation: stage-drift 48s linear infinite alternate;
}
.bg.is-empty {
  background-image: linear-gradient(140deg, #1b2030, #08090b 60%);
  animation: none;
}
@keyframes stage-drift {
  from { transform: scale(1.1) translate3d(-1.5%, -1%, 0); }
  to { transform: scale(1.16) translate3d(1.5%, 1%, 0); }
}
.scrim {
  position: absolute; inset: 0; z-index: -2;
  background: linear-gradient(180deg, rgba(0,0,0,0.62), rgba(0,0,0,0.45) 45%, rgba(0,0,0,0.78));
}
.vignette {
  position: absolute; inset: 0; z-index: -1;
  background: radial-gradient(ellipse at center, transparent 28%, rgba(0,0,0,0.72) 100%);
}
.top {
  display: flex; align-items: flex-start; gap: var(--stage-3);
  padding: var(--stage-4) var(--stage-6);
  min-height: var(--stage-touch);
}
.icon {
  flex: 0 0 auto;
  display: inline-flex; align-items: center; justify-content: center;
  width: var(--stage-touch); height: var(--stage-touch);
  padding: 0; border: 0; border-radius: var(--stage-radius);
  color: var(--stage-fg); background: rgba(255,255,255,0.06);
  cursor: pointer;
  transition: background var(--stage-fast) var(--stage-ease), opacity var(--stage-fast) var(--stage-ease);
}
.icon:hover { background: rgba(255,255,255,0.16); }
.icon:disabled { opacity: 0.32; cursor: default; }
.icon svg { width: 22px; height: 22px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
.icon:focus-visible, .seek:focus-visible, input:focus-visible, button:focus-visible, a:focus-visible {
  outline: 2px solid var(--stage-fg);
  outline-offset: 2px;
}
.now { display: flex; align-items: center; gap: var(--stage-3); min-width: 0; flex: 1 1 auto; }
.cover {
  width: 40px; height: 40px; flex: 0 0 auto;
  border-radius: var(--stage-radius);
  background: linear-gradient(140deg, #2a3142, #12141a) center / cover no-repeat;
}
.now-text { min-width: 0; display: flex; flex-direction: column; }
.now-title, .now-artist {
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 42ch;
}
.now-title { font-size: var(--stage-text-sm); font-weight: 600; }
.now-artist { font-size: var(--stage-text-xs); color: var(--stage-muted); }
.player-name {
  align-self: center;
  max-width: 20ch;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  font-size: var(--stage-text-xs); color: var(--stage-muted);
}
.player-name:empty { display: none; }
.badge {
  align-self: center;
  padding: var(--stage-1) var(--stage-2);
  border: 1px solid var(--stage-line); border-radius: var(--stage-radius);
  font-size: var(--stage-text-xs); color: var(--stage-muted);
  font-variant-numeric: tabular-nums;
}
.badge[data-state="offline"], .badge[data-state="auth_failed"], .badge[data-state="reconnecting"] { color: var(--stage-danger); border-color: var(--stage-danger); }
.badge[data-state="stale"] { color: #ffd79a; border-color: #ffd79a; }
.lyrics {
  position: relative;
  overflow: hidden;
  display: flex; flex-direction: column; justify-content: center;
  width: min(64ch, 100%);
  margin: 0 auto;
  padding: var(--stage-4) var(--stage-6);
}
.lines { display: flex; flex-direction: column; gap: var(--stage-3); }
.line {
  margin: 0;
  font-size: clamp(var(--stage-text-xl), 3.1vw, var(--stage-text-4xl));
  line-height: 1.18;
  letter-spacing: -0.02em;
  color: var(--stage-dim);
  opacity: 0.55;
  filter: blur(1.5px);
  transition: opacity var(--stage-base) var(--stage-ease), filter var(--stage-base) var(--stage-ease);
  overflow-wrap: anywhere;
}
.line.is-active { opacity: 1; filter: none; color: var(--stage-fg); font-weight: 600; }
.line.is-near { opacity: 0.75; filter: blur(0.6px); }
.line.is-far { display: none; }
.line.is-background { font-size: 0.7em; opacity: 0.5; }
.segment { position: relative; display: inline; color: var(--stage-dim); }
.segment.is-active { color: var(--stage-fg); }
.fill {
  position: absolute; inset: 0;
  display: inline;
  color: var(--stage-fg);
  clip-path: inset(0 calc(100% - var(--fill, 0%)) 0 0);
  pointer-events: none;
  white-space: pre;
}
.static-note { margin-top: var(--stage-4); font-size: var(--stage-text-xs); color: var(--stage-muted); }
.status {
  max-width: 60ch; margin: 0 auto; text-align: center;
  color: var(--stage-muted); font-size: var(--stage-text-lg);
}
.status[hidden] { display: none; }
.status[data-kind="error"] { color: var(--stage-danger); }
.status p { margin: 0 0 var(--stage-4); }
.status button {
  min-height: var(--stage-touch); padding: 0 var(--stage-4);
  border: 1px solid var(--stage-line); border-radius: var(--stage-radius);
  color: var(--stage-fg); background: rgba(255,255,255,0.08); cursor: pointer;
}
.skeleton { display: flex; flex-direction: column; gap: var(--stage-3); max-width: 52ch; margin: 0 auto; }
.skeleton span {
  display: block; height: 1.1em; border-radius: var(--stage-radius);
  background: linear-gradient(90deg, rgba(255,255,255,0.06), rgba(255,255,255,0.18), rgba(255,255,255,0.06));
  background-size: 200% 100%;
  animation: stage-shimmer 1.6s linear infinite;
}
@keyframes stage-shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
.attribution {
  margin-top: var(--stage-6);
  font-size: var(--stage-text-xs); color: var(--stage-muted);
  overflow-wrap: anywhere;
}
.attribution[hidden] { display: none; }
.attribution a { color: inherit; text-decoration: underline; text-underline-offset: 2px; }
.controls {
  display: flex; flex-direction: column; gap: var(--stage-2);
  padding: var(--stage-3) var(--stage-6) var(--stage-4);
  background: linear-gradient(0deg, rgba(0,0,0,0.65), transparent);
  transition: opacity var(--stage-base) var(--stage-ease);
}
.controls.is-hidden { opacity: 0; pointer-events: none; }
.transport { display: flex; align-items: center; gap: var(--stage-3); }
.transport .spacer { flex: 1 1 auto; }
.seek-row { display: flex; align-items: center; gap: var(--stage-3); }
.seek {
  flex: 1 1 auto; min-width: 0; height: var(--stage-touch);
  margin: 0; appearance: none; background: transparent; cursor: pointer;
}
.seek::-webkit-slider-runnable-track { height: 4px; border-radius: 2px; background: var(--stage-line); }
.seek::-moz-range-track { height: 4px; border-radius: 2px; background: var(--stage-line); }
.seek::-webkit-slider-thumb { appearance: none; width: 14px; height: 14px; margin-top: -5px; border-radius: 50%; background: var(--stage-fg); }
.seek::-moz-range-thumb { width: 14px; height: 14px; border: 0; border-radius: 50%; background: var(--stage-fg); }
.time { font-size: var(--stage-text-xs); color: var(--stage-muted); font-variant-numeric: tabular-nums; min-width: 8ch; }
.volume { width: 88px; height: var(--stage-touch); margin: 0; appearance: none; background: transparent; cursor: pointer; }
.volume::-webkit-slider-runnable-track { height: 4px; border-radius: 2px; background: var(--stage-line); }
.volume::-moz-range-track { height: 4px; border-radius: 2px; background: var(--stage-line); }
.volume::-webkit-slider-thumb { appearance: none; width: 14px; height: 14px; margin-top: -5px; border-radius: 50%; background: var(--stage-fg); }
.volume::-moz-range-thumb { width: 14px; height: 14px; border: 0; border-radius: 50%; background: var(--stage-fg); }
.pending { position: relative; }
.pending::after {
  content: ""; position: absolute; right: 4px; top: 4px;
  width: 6px; height: 6px; border-radius: 50%; background: #ffd79a;
}
.sheet {
  position: absolute; top: 0; right: 0; bottom: 0; z-index: 5;
  width: min(340px, 92vw); padding: var(--stage-6);
  background: var(--stage-surface); backdrop-filter: blur(6px);
  border-left: 1px solid var(--stage-line);
  display: flex; flex-direction: column; gap: var(--stage-4);
}
.sheet[hidden] { display: none; }
.sheet h2 { margin: 0; font-size: var(--stage-text-lg); }
.sheet label { display: flex; flex-direction: column; gap: var(--stage-2); font-size: var(--stage-text-sm); }
.sheet input[type="password"] {
  min-height: var(--stage-touch); padding: 0 var(--stage-3);
  border: 1px solid var(--stage-line); border-radius: var(--stage-radius);
  background: rgba(255,255,255,0.06); color: var(--stage-fg);
  font-size: var(--stage-text-base);
}
.sheet-row { display: flex; align-items: center; justify-content: space-between; gap: var(--stage-3); }
.sheet-row button {
  min-width: var(--stage-touch); min-height: var(--stage-touch);
  border: 1px solid var(--stage-line); border-radius: var(--stage-radius);
  background: rgba(255,255,255,0.06); color: var(--stage-fg); cursor: pointer;
}
.sheet-actions { display: flex; flex-wrap: wrap; gap: var(--stage-3); margin-top: auto; }
.sheet-actions button {
  min-height: var(--stage-touch); padding: 0 var(--stage-4);
  border: 1px solid var(--stage-line); border-radius: var(--stage-radius);
  background: rgba(255,255,255,0.08); color: var(--stage-fg); cursor: pointer;
}
.sheet-note { font-size: var(--stage-text-xs); color: var(--stage-muted); margin: 0; }
.sr-only {
  position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
  overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0;
}
@media (max-height: 520px) {
  .top { padding: var(--stage-2) var(--stage-4); }
  .lyrics { padding: var(--stage-2) var(--stage-4); }
  .lines { gap: var(--stage-2); }
  .line { font-size: clamp(var(--stage-text-lg), 2.6vw, var(--stage-text-2xl)); }
  .controls { padding: var(--stage-2) var(--stage-4); }
  .attribution { margin-top: var(--stage-2); }
}
@media (max-width: 480px) {
  .top { gap: var(--stage-2); padding: var(--stage-2) var(--stage-3); }
  .now-text { max-width: 52vw; }
  .now-title, .now-artist { max-width: 52vw; }
  .player-name, .badge { display: none; }
  .volume { display: none; }
  .time { min-width: 7ch; }
}
@media (prefers-reduced-motion: reduce) {
  .bg { animation: none; transform: none; }
  .skeleton span { animation: none; }
  .line, .controls, .icon { transition: none; }
}
`;
