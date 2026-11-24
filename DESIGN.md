# Home Assistant Kiosk Design System — Minimal

> **Status: adopted.** This file and [`tokens.css`](tokens.css) are the authoritative design system for the kiosk. The Vite prototype in [`index.html`](index.html) and [`styles.css`](styles.css) is the visual reference; the production UI is Slint.
>
> Adopted: 2026-11-23. Owner decision: use this design for the kiosk instead of a placeholder theme.

## 1. Visual theme

Stripped-back, light, high-contrast and touch-first. White and warm off-white surfaces, black as the only accent, one warm glow for the active light, thin borders, generous whitespace, and a clear type hierarchy. The design must stay legible at 800 × 480 on a Pi Zero 2 W and comfortable at 1280 px and above.

- **Style:** minimal, clean, bold
- **Surfaces:** white / warm off-white
- **Accent:** near-black
- **Semantic colours:** green, amber, red — used only for meaning
- **Density:** spacious on desktop, compact but readable on the 800 × 480 kiosk

## 2. Colour

All colours come from `tokens.css`. Do not introduce a hex value in a component.

| Token | Value | Use |
| --- | --- | --- |
| `--bg` | `#ffffff` | Page and card background |
| `--surface` | `#fafafa` | Secondary cards, selected rows |
| `--surface-warm` | `#f5f5f5` | Warm neutral panels |
| `--fg` | `#111111` | Primary text and icons |
| `--fg-2` | `#3a3a3a` | Secondary text |
| `--muted` | `#6b6b6b` | Meta text (AA on `--surface`) |
| `--border` | `#e2e2e2` | Card and control borders |
| `--border-soft` | `#eeeeee` | Dividers |
| `--accent` | `#111111` | Primary fill, active controls |
| `--accent-on` | `#ffffff` | Text on accent |
| `--success` | `#0f7a3a` | On / healthy state (AA on `--surface`) |
| `--warn` | `#8a5a12` | Unavailable / warning (AA on `--surface`) |
| `--danger` | `#c53030` | Error / destructive |

Rules:

- Text on a surface must reach **4.5:1**; large text and UI edges **3:1**.
- Never encode meaning by colour alone; pair it with a label, icon, or weight.
- Keep semantic colours under roughly 5% of a screen.
- The warm glow is decoration behind an active light; it is not a state indicator by itself.

## 3. Typography

- **Display and body:** `Inter`, falling back to `system-ui`, then the installed sans font.
- **Mono:** `SF Mono`, `ui-monospace`, `Menlo`, monospace.
- **Weights:** regular 400, medium 500, semibold 600. No faux bold.
- **Body line-height:** 1.55; display line-height: 1.08.
- **Measure:** 45–75 characters on wide text blocks.
- **Tabular numerals** for clock, timers, temperatures, prices, and aligned data.

Scale (from `tokens.css`):

| Token | Size |
| --- | --- |
| `--text-xs` | 12 px |
| `--text-sm` | 14 px |
| `--text-base` | 16 px |
| `--text-lg` | 18 px |
| `--text-xl` | 22 px |
| `--text-2xl` | 32 px |
| `--text-3xl` | 48 px |
| `--text-4xl` | 64 px |

The clock is the only display element that may use the 48/64 px steps on the kiosk. At 800 × 480, use the largest step that still fits the top bar without crowding the status pill.

## 4. Space, radius, elevation, motion

- Spacing: `4, 8, 12, 16, 20, 24, 32, 48` px.
- Radii: `2, 4, 8` px and a pill for switches/status.
- Elevation: mostly flat; a 1 px ring or a single soft raised shadow. No stacked shadows.
- Motion: `140 ms` for control feedback, `220 ms` for layout changes; standard easing.
- Honour `prefers-reduced-motion`; provide a static equivalent.
- Never animate to decorate. The clock updates once per second.

## 5. Layout and breakpoints

The prototype is a single Overview reference. The kiosk keeps four pages: Overview, Media, Agenda, Status. Wave 6 adds an optional fifth Shop page when `extensions.enabled = true`; it uses the same tokens and touch targets.

| Width | Layout |
| --- | --- |
| 800 × 480 | Compact kiosk: top bar, hero light card, and a dense tile/room grid; secondary content is paged or within one contained scroll region |
| 1024 × 600 | Two-column Overview; hero spans the leading column |
| 1280+ | Three-column Overview like the reference |
| 1440/2048 | Same structure, capped content width; no stretched cards |

Rules:

- No horizontal scrolling on a normal page.
- Tap targets are at least **44 × 44 px**; the visual switch is 48 × 28 px with a 44 px-high hit area.
- The top bar stays fixed in height; the clock and connection state must not jump as values change.
- Text elides or wraps to a fixed number of lines; never push a control off-screen.
- The prototype's Energy and Schedule cards are **post-v1**, not v1 gates.

## 6. Components and data mapping

| Prototype | Kiosk mapping |
| --- | --- |
| Top bar: Home, Connected, clock/date | Overview header; connection state from the HA WebSocket; local clock |
| Hero light: toggle, brightness, colour temperature | Configured `light.*` entity; `light.toggle`, `light.turn_on` with `brightness_pct` and `color_temp_kelvin`; debounced sliders |
| Scenes: Relax/Read/Focus/Night | Configured `scene.*` / `script.*` quick actions; never hardcode names |
| Rooms: Living room, Kitchen, … | Configurable tile/room status group; never hardcode room names |
| Quick actions: All lights off, Night mode, Away | Configured scene/script actions; no invented automations |
| Energy: kWh + seven bars | **Post-v1**; requires an energy data source |
| Schedule timeline | **Post-v1**; requires an automation/calendar source |

The prototype uses mock values. Production data must come from Home Assistant; never ship invented metrics, rooms, scenes, or automations.

## 7. Kiosk adaptation (Slint)

The production app does not embed a browser. The design is ported to Slint:

- Tokens become Slint properties in `ui/theme.slint`; no hardcoded colours, spacing, or type sizes in components.
- CSS `color-mix`, filters and blurred shadows do not map directly. Use flat token colours and simple gradients; simplify the lamp glow for the software renderer.
- The hero dial is a visual readout plus a separate slider; do not rely on a browser range input.
- Focus rings and pressed states are explicit Slint states.
- The switch uses a 44 px hit area even though the visual track is 28 px high.
- Inter is bundled with an OFL licence, or the app falls back to the installed system sans font. Never fetch a font at runtime.
- Verify at 800 × 480, 1024 × 600, 1280, 1440, and 2048 px.

## 8. States

Every data surface must render:

- **Loading:** skeleton or a labelled placeholder; no blank card.
- **Empty:** explain what belongs here and how to configure it.
- **Error:** say what failed and offer the next action (retry, open Status).
- **Populated:** real Home Assistant data with honest units and timestamps.
- **Edge:** one item, many items, very long strings, missing fields, unavailable entity.

## 9. Authority and change rules

- `DESIGN.md` and `tokens.css` are authoritative. If they disagree, `tokens.css` wins and `DESIGN.md` must be corrected in the same change.
- `index.html` and `styles.css` are the reference prototype. They are not the production UI and must not be shipped as the kiosk.
- Any new colour, spacing, type size, radius, or motion value needs a token and a design review.
- A hardcoded value that a token already covers is a defect.
- Update this file, `tokens.css`, the prototype, and the Slint port together when the visual system changes.
