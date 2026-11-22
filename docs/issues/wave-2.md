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
- [ ] `AppController` applies `AppEvent` values and updates Slint models/properties on the UI thread.
- [ ] Coalesce updates and cap redraws at four per second; clock is a separate one-second timer.
- [ ] Fullscreen through `slint::Window::set_fullscreen(true)`.

### Touch and focus

- [ ] Interactive elements have a minimum 44 × 44 logical-pixel hit area.
- [ ] Pressed/active visual state; focus ring for development builds.
- [ ] Tap detection for tiles; no undocumented long-press action.
- [ ] Tests with `i-slint-backend-testing`: tap changes page, swipe threshold, too-small targets fail.

### Theme

- [ ] `ui/theme.slint` with color, spacing, radius, and type-scale tokens.
- [ ] Dark theme first; light theme structure prepared.
- [ ] Bundled SVG icons for weather conditions; no runtime icon downloads.
- [ ] Verify text contrast on background and tiles.

## Acceptance gate

- Desktop build starts and shows four pages.
- Swipe and tab navigation work.
- Shell tests pass in `i-slint-backend-testing`.
- No widget implements real Home Assistant data yet.

## Out of scope

- Clock, weather, tiles, media, departures, and calendar logic.
- Kiosk display setup.

## Notes

- Slint 1.19 has no cursor-visibility API. Cursor hiding is deployment-specific and must not be faked in `kiosk.rs`.
- Do not add continuous animations; the Pi Zero 2 W target is a software renderer.
