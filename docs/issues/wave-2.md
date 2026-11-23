# Wave 2 — UI shell: navigation, touch, theme

**Spec:** [PLAN.md](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/PLAN.md), section 8, Wave 2.
**Depends on:** Wave 1 (state types and event bridge).
**Blocks:** Wave 3.
**Labels:** `implementation`, `wave-2`.

## Context

Create the Slint application shell: four pages, tab and swipe navigation, touch targets, and placeholder theme tokens. The shell must run on desktop and be testable without a physical display.

## Checklist

### Shell and navigation

- [ ] `ui/main.slint`: window, page host, tab bar for Overview/Media/Agenda/Status.
- [ ] Horizontal swipe with axis lock and threshold (at least 20% of width or 60 logical pixels); tab bar remains the fallback.
- [ ] Page swipe starts only from non-interactive background/edge zones, never from a slider, scrollable list, button, or tile.
- [ ] Long lists use vertical `Flickable`; page swipe and list scrolling do not conflict.
- [ ] `AppController` applies `AppEvent` values and updates Slint models/properties on the UI thread.
- [ ] Coalesce updates and cap redraws at four per second; clock is a separate one-second timer.
- [ ] Fullscreen through `slint::Window::set_fullscreen(true)`.

### Touch and focus

- [ ] Interactive elements have a minimum 44 × 44 logical-pixel hit area.
- [ ] Pressed/active visual state; focus ring for development builds.
- [ ] Tap detection for tiles; no undocumented long-press action.
- [ ] Tests with `i-slint-backend-testing`: tap changes page, swipe threshold, gesture from a slider does not change page, vertical scroll does not change page, too-small targets fail.
- [ ] Verify 800 × 480 and 1024 × 600 layouts; long text elides or wraps without pushing controls off-screen.

### Theme

- [ ] `ui/theme.slint` with color, spacing, radius, and type-scale tokens.
- [ ] Dark theme first; light theme structure prepared.
- [ ] Bundled SVG icons for weather conditions; no runtime icon downloads.
- [ ] Verify text contrast on background and tiles.

## Acceptance gate

- Desktop build starts and shows four pages.
- Swipe and tab navigation work; swipe does not start from a slider or list.
- Shell tests pass in `i-slint-backend-testing`, including gesture arbitration and both target layouts.
- No widget implements real Home Assistant data yet.

## Out of scope

- Clock, weather, tiles, media, departures, and calendar logic.
- Kiosk display setup.

## Notes

- Slint 1.19 has no cursor-visibility API. Cursor hiding is deployment-specific and must not be faked in `kiosk.rs`.
- Do not add continuous animations; the Pi Zero 2 W target is a software renderer.

---

## Subtask issues

The wave issue is the epic. Every checkbox above, except the commit step, is a separate issue:

### 2.1 Slint shell and navigation

- [ ] #41 — [W2.1.1] ui/main.slint: window, page host, tab bar for Overview/Media/Agenda/Status.
- [ ] #42 — [W2.1.2] Horizontal swipe with axis lock and a threshold (at least 20% of width or 60 logical pi...
- [ ] #43 — [W2.1.3] Page swipe starts only from non-interactive background/edge zones, never from a slider,...
- [ ] #44 — [W2.1.4] Long lists use vertical Flickable (or equivalent); page swipe and list scrolling do not...
- [ ] #45 — [W2.1.5] AppController applies AppEvent values and updates Slint models/properties on the UI thr...
- [ ] #46 — [W2.1.6] Coalesce state updates and cap redraws at four per second; the clock is a separate one-...
- [ ] #47 — [W2.1.7] Set fullscreen through slint::Window::set_fullscreen(true).

### 2.2 Touch and focus states

- [ ] #48 — [W2.2.1] All interactive elements have a minimum 44 × 44 logical-pixel hit area.
- [ ] #49 — [W2.2.2] Pressed/active visual state; focus ring for keyboard/mouse development builds.
- [ ] #50 — [W2.2.3] Tap detection for tiles and long-press reserved for future use (do not ship an undocume...
- [ ] #51 — [W2.2.4] Test with i-slint-backend-testing: tap changes page; swipe threshold; gesture from a sl...
- [ ] #52 — [W2.2.5] Verify 800 × 480 and 1024 × 600 layouts; long text elides or wraps without pushing cont...

### 2.3 Theme tokens

- [ ] #53 — [W2.3.1] ui/theme.slint with color, spacing, radius, and type-scale tokens.
- [ ] #54 — [W2.3.2] Dark theme first; light theme structure prepared but not required to be polished.
- [ ] #55 — [W2.3.3] Bundled SVG icons for weather conditions; no runtime icon downloads.
- [ ] #56 — [W2.3.4] Verify contrast for text on background and on tiles.
