# Validated Bus Departures Example Design

## Intent

Provide a canonical, provider-neutral `org.example.bus-departures` example that is validated by the kiosk's real manifest parser and Rhai sandbox. The example teaches catalog authors the supported package contract without placing extension source or catalog data in the kiosk repository.

## Goals

- Keep the example ID, placeholder HTTPS host, and placeholder stop ID generic.
- Require a configurable `stop_id` and bound the optional `limit` to 1-20.
- Return only the declarative `departures` view model.
- Validate populated, empty, limited, and provider-error behavior against recorded fixtures.
- Keep malformed provider responses visible as errors so the host can retain the last good output.

## Non-goals

- Creating or publishing the separate catalog repository.
- Supporting a real transit provider, credentials, Home Assistant service calls, or arbitrary UI.
- Adding an on-screen configuration editor.

## Constraints

- Extension source and catalog data must not enter the kiosk repository.
- The canonical manifest, script, README, and license live in `docs/extensions.md`; executable validation lives in `tests/extension_example_test.rs`.
- English only; no personal identifiers, provider URLs, tokens, or secrets.
- Use the existing `Manifest`, `ExtensionManifest`, `ExtensionHost`, `ExtensionRunner`, and `HttpFetcher` contracts.

## Approaches

1. **Validate the documented canonical example in the real host.** Keeps the repository boundary, prevents documentation drift, and gives the companion catalog a copyable source of truth. Chosen.
2. **Store a complete package under `tests/fixtures`.** Easier to package, but violates the rule that extension source stays out of this repository.
3. **Validate only TOML structure.** Cheaper, but cannot prove the Rhai script compiles or maps provider data correctly.

## Decisions

- Mark the manifest, script, README, and license fences in `docs/extensions.md` so tests extract the exact published package files.
- Accept only non-empty stop IDs made of ASCII letters, digits, `-`, `_`, `.`, or `~`; this keeps query construction safe without adding a host URL-encoding function.
- Let missing required provider fields and HTTP failures propagate. The existing runner keeps the last good result and marks repeated failures degraded.
- Use an in-memory fake HTTP fetcher; tests never contact a network.

## Validation

The integration test parses both manifest schemas, builds and safely extracts the four-file package, runs the extracted script through `ExtensionHost`, checks the requested placeholder URL and output mapping, verifies the configured limit and empty result, and proves provider errors preserve the last good output through `ExtensionRunner`.
