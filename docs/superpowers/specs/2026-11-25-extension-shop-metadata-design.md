# Extension Shop Metadata and Status Design

## Intent

Complete Wave 6.3.2 by making every Shop card show the extension's name, installed and catalog versions, author, license, description, declared permissions, configuration status, and errors. Installed extensions must retain this information when the catalog is offline.

## Goals

- Show explicit configuration states: `Configured`, `Configuration required`, `No configuration required`, or `Configuration unavailable`.
- Derive configuration state from the installed manifest and its settings/secret inputs, not from the presence of an arbitrary non-empty string.
- Load installed-only metadata from the installed `manifest.toml` on the background shop worker.
- Keep action/runtime errors visible and label them as errors.
- Preserve the existing visual system, touch targets, and Shop actions.

## Non-goals

- Editing configuration on screen.
- Changing installation, update, removal, or trust-confirmation behavior.
- Changing the Status page or design tokens.
- Adding provider-specific values or secrets.

## Constraints

- English code, documentation, UI strings, tests, and commits.
- No blocking file or network I/O on the Slint UI thread.
- No secret values in UI models, logs, tests, or documentation.
- The Shop remains usable at 800 × 480 and the project breakpoints through 2048 px.
- Use only existing `DESIGN.md`, `tokens.css`, and `ui/theme.slint` tokens.

## Approaches

1. **UI-only label from the existing boolean.** Rejected: the boolean currently treats any non-empty string as configured and installed-only cards still have no manifest metadata.
2. **Manager-backed installed manifest snapshots plus explicit UI status.** Chosen: the background manager already owns the store and configuration inputs, so it can produce truthful metadata and status without a schema migration.
3. **Duplicate all metadata in `installed.json`.** Rejected: it duplicates the signed manifest, requires a storage migration, and can become stale.

## Design

- Add an explicit metadata-availability flag to `ShopEntry`; verified catalog entries and successfully loaded installed manifests set it.
- Build installed-only `ShopEntry` values from the installed manifest. If the manifest cannot be read, keep the installed identity/version and surface a manifest error instead of inventing permissions or configuration state.
- Compute `configured` with `Manifest::build_config`, which checks required fields, types, unknown keys, and declared secret placement.
- Include installed entries in the catalog-offline snapshot so `ShopView` can retain their metadata while marking install/update unavailable.
- Expose configuration status and installed version through the UI bridge. The card shows installed and latest versions separately when they differ, uses honest fallbacks for unavailable metadata, and labels errors explicitly.

## Verification

- View-model tests cover metadata availability, configuration states, offline snapshots, and bridge fields.
- Manager tests cover installed-only metadata and required settings/secrets.
- Slint tests cover populated, offline, error, and long-string cards at 800 × 480, 1024 × 600, 1280 × 800, 1440 × 900, and 2048 × 1200.
- Run formatting, Clippy, the full Rust test suite, documentation checks, and Python tests.
