# Extension Author Guide (Wave 6)

This guide describes the post-v1 community extensions for the Home Assistant Kiosk. Extensions add **data widgets** -- for example bus departures -- without shipping a new kiosk binary.

Extensions are untrusted third-party code. The signed catalog proves who published a package; it does not prove that the package is safe. Read the security rules before publishing.

See also: [Extension Catalog Specification](extension-catalog.md) for the repository layout, index format, release flow, signing, and review process.

## What an extension can and cannot do

An extension can:

- Fetch JSON from declared HTTPS hosts.
- Read declared Home Assistant entities (read-only snapshot).
- Return one of three view models: `departures`, `list`, or `key_value`.
- Read its own non-secret settings and secret values.

An extension cannot:

- Ship native code, a shared library, an executable, WASM, or arbitrary Slint files.
- access the filesystem, processes, environment variables, or arbitrary network hosts.
- Call Home Assistant services or receive the Home Assistant token.
- Draw arbitrary UI or open windows.
- Read the kiosk configuration or another extension's settings.

## Shop page

The Shop is a fifth page. It appears only when `extensions.enabled = true`; with the default configuration the kiosk keeps four pages.

- **Installed** lists extensions recorded in `installed.json`, including ones that are no longer in the catalog.
- **Available** lists catalog entries that are not installed. Each card shows the name, version, author, license, description, status, declared permissions, configuration keys, and the exact settings and secret file paths.
- **Actions:** Install, Update, Remove, and Enable/Disable. Install opens a first-install trust confirmation that repeats the warning and permissions; nothing is installed until the user confirms. Update is offered only when the catalog version differs from the installed version.
- **Honest state:** an action is shown as pending until the store confirms it. A failed action shows the error on the card; it is never rendered as a success.
- **Offline:** installed extensions stay usable. Install and update are unavailable, and the page shows the catalog error instead of hiding the page.
- **Errors** are also collected on the Status page.
- **No on-screen keyboard:** the page shows the configuration keys and the exact TOML and secret file paths. Edit those files directly.

The shop view model is backed by `extensions::manager`. The manager refreshes the signed catalog, downloads and verifies packages, drives the store, and runs installed scripts only on a background thread. The Shop page never blocks the UI thread and never reports an action as successful before the store confirms it.

## Package layout

An extension source directory contains these files:

```text
org.example.bus-departures/
├── manifest.toml
├── main.rhai
├── README.md
└── LICENSE
```

The `.hkext` archive stores these files at its root; do not wrap them in an extra directory. Paths must be relative; absolute paths, `..`, symlinks, hardlinks, and device files are rejected.

## Manifest

```toml
id = "org.example.bus-departures"
name = "Bus Departures"
version = "1.0.0"
api_version = 1
min_app_version = "0.6.0"
entry = "main.rhai"
provides = "departures" # departures | list | key_value
author = "Example Author"
license = "MIT"
homepage = "https://example.com/bus-departures"
description = "Shows upcoming bus departures for a configured stop."

[permissions]
http_hosts = ["api.example.com"]
ha_entities = []

[[config_schema]]
key = "stop_id"
label = "Stop ID"
type = "string"
required = true
help = "The stop identifier used by the transit API."

[[config_schema]]
key = "limit"
label = "Maximum departures"
type = "integer"
required = false
default = 6
min = 1
max = 20
```

Rules:

- `id` is immutable and reverse-DNS. Use a domain you control.
- `version` is semver. Never reuse a version.
- `api_version` is `1` for this API. A newer kiosk may reject older extensions only if the API changes.
- `min_app_version` is the lowest kiosk version that can run the extension.
- `provides` must match the object returned by `refresh`.
- `http_hosts` lists exact hosts, optionally with a port. No wildcards; HTTPS only.
- `ha_entities` lists exact entity IDs the script may read.
- `license` is an SPDX identifier. The package must include the license text.
- Do not put secrets in the manifest. Secret fields use a separate file.

Config field types: `string`, `integer`, `boolean`, `url`, `secret`, `entity_id`, and `select` (with `options`). The shop displays the required keys and the file path; it does not provide an on-screen keyboard.

## Script API

`main.rhai` must define:

```rhai
fn refresh(config) {
    // Return a map matching the manifest's `provides` value.
}
```

The host registers only these functions:

| Function | Behaviour |
| --- | --- |
| `http_get_json(url)` | GET only, HTTPS, host allowlist, no credentials, 10 s timeout, 1 MB cap, no redirects followed (so a cross-host redirect cannot escape the allowlist). Returns a Rhai value. |
| `ha_state(entity_id)` | Read-only map with `state` and `attributes`, allowlisted entity IDs only. Returns `()` if unavailable. |
| `now()` | RFC 3339 timestamp in the Home Assistant timezone. |
| `log(message)` | Debug log. Do not log secrets or full API responses. |

No other host functions are registered. Rhai's own language has no filesystem, process, environment, or network access.

Production fetches always use HTTPS. `extensions.dev_mode = true` additionally allows plain HTTP for an allowlisted loopback host so a local test server can be used while developing an extension; leave `dev_mode = false` on an installed kiosk.

Limits: 1,000,000 operations, 64 KB strings, 1,000 array/map entries, max call depth, no modules/imports, 10 s wall clock. Scripts run off the UI thread. The wall-clock limit includes blocking host calls: each fetch uses the shorter of its HTTP timeout and the run's remaining wall-clock budget, and a late result fails the run.

### Bus departures example

The example is provider-neutral. Replace the API URL and field names with the ones your provider documents; keep the host in `http_hosts`.

```rhai
fn refresh(config) {
    let url = "https://api.example.com/departures?stop=" + config.stop_id;
    let data = http_get_json(url);
    let items = [];

    for d in data.departures {
        items.push(#{
            line: d.line,
            destination: d.destination,
            departure: d.time,          // RFC 3339 with offset preferred
            delay_minutes: d.delay_minutes ?? 0,
            platform: d.platform ?? (),
            cancelled: d.cancelled ?? false,
            remarks: d.remarks ?? ()
        });
    }

    #{ kind: "departures", title: "Bus", items: items }
}
```

`??` is Rhai's null-coalescing operator; use the syntax supported by the pinned Rhai version. The host validates and caps the result.

### Other output kinds

`list`:

```json
{"kind": "list", "title": "Trams", "columns": ["Line", "Destination", "Time"],
 "rows": [["1", "Central Station", "21:55"]]}
```

`key_value`:

```json
{"kind": "key_value", "title": "Stop status",
 "items": [{"label": "Stop", "value": "Central"}, {"label": "Updated", "value": "21:50"}]}
```

Output is data only. The built-in widgets decide fonts, colors, layout, and touch targets.

## Output schema

Every `refresh` result is a map. The host validates it against the manifest's `provides` value, rejects unknown fields and wrong types, and caps the item count at 100. Every item is validated before excess items are dropped, so malformed output beyond the cap still rejects the refresh. A rejected result shows an error and keeps the last good data.

`departures`:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | string | yes | Must be `"departures"`. |
| `title` | string | no | Widget heading. |
| `items` | array | yes | Up to 100 departure objects. |
| `items[].line` | string | yes | Line or route label. |
| `items[].destination` | string | yes | Destination label. |
| `items[].departure` | string | yes | RFC 3339 timestamp; an offset is preferred. |
| `items[].delay_minutes` | integer | no | Signed delay in minutes; defaults to no delay. |
| `items[].platform` | string or null | no | Omit or return `()`. |
| `items[].cancelled` | boolean | no | Defaults to `false`. |
| `items[].remarks` | string or null | no | Short status text. |

`list`:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | string | yes | Must be `"list"`. |
| `title` | string | no | Widget heading. |
| `columns` | array of strings | yes | Header labels. |
| `rows` | array of arrays | yes | Up to 100 rows; each row matches `columns` in length. |
| `rows[]` | string, number, or boolean | yes | Cell values are rendered as text. |

`key_value`:

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | string | yes | Must be `"key_value"`. |
| `title` | string | no | Widget heading. |
| `items` | array | yes | Up to 100 entries. |
| `items[].label` | string | yes | Field label. |
| `items[].value` | string, number, or boolean | yes | Field value rendered as text. |

The host does not render HTML, Markdown, images, or custom controls. Long strings are truncated by the widget, not by the script.

## Configuration and secrets

Non-secret settings:

```toml
[extensions.settings."org.example.bus-departures"]
stop_id = "REPLACE_ME"
limit = 6
```

Secret settings:

```text
/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml
```

The secret file is mode `0600` and owned by the service user. The host injects only the declared secret keys into `config`; it never logs them.

## Packaging

From the parent directory:

```bash
tar -czf org.example.bus-departures-1.0.0.hkext \
  -C org.example.bus-departures \
  manifest.toml main.rhai README.md LICENSE
sha256sum org.example.bus-departures-1.0.0.hkext
```

The `-C` flag keeps `manifest.toml` at the archive root, as the catalog requires.

The catalog records the SHA-256 and byte size. Publish the `.hkext` as a GitHub release asset or another stable HTTPS URL. Do not change a published package; publish a new version.

## Local testing

1. Start a local catalog (plain HTTP is allowed only with `allow_unsigned = true` and `extensions.dev_mode`).
2. Install the package in a scratch directory.
3. Check that the script cannot reach a host or entity outside its manifest.
4. Check that an invalid output shape shows an error and keeps the last good data.
5. Check the operation/time limits with a deliberately slow or looping script.
6. Test the shop at 800 x 480 with touch emulation.

## Publishing

1. Open a pull request in the catalog repository.
2. Include `manifest.toml`, `main.rhai`, `README.md`, `LICENSE`, and tests or a test plan.
3. Keep permissions minimal. A bus widget should not request Home Assistant entities it does not display.
4. Do not include secrets, personal stop IDs, home addresses, or real API keys.
5. Wait for CI validation and review.
6. A maintainer publishes the release asset and signs the catalog index offline.
7. Never sign a package whose contents you have not reviewed.

## Versioning and deprecation

- Patch: bug fix with the same behaviour and permissions.
- Minor: new optional config fields or output fields; keep old configs working.
- Major: breaking config or output changes; update `min_app_version` if the kiosk API changed.
- Removing a package from the catalog does not uninstall it from existing kiosks; the shop marks it unavailable.

## Security checklist

- [ ] No native code, WASM, or arbitrary Slint files.
- [ ] No filesystem, process, environment, or module access.
- [ ] Only declared HTTPS hosts; no credentials in URLs.
- [ ] Only declared HA entities, read-only; no service calls.
- [ ] No secrets in the manifest, README, logs, or examples.
- [ ] Output matches the declared kind and is capped at 100 items.
- [ ] Script respects the operation and time limits.
- [ ] License is present and compatible with redistribution.
- [ ] The package checksum matches the catalog entry.
