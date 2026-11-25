# Extension Author Guide (Wave 6)

This guide describes the post-v1 community extension API for the Home Assistant Kiosk. An extension adds a **data widget**, such as bus departures, without shipping a new kiosk binary or arbitrary UI.

Extensions are untrusted third-party code. A signed catalog proves who published a package; it does not prove that the package is safe. Keep permissions minimal and read the [security rules](#security-rules) before publishing.

See also: [Extension Catalog Specification](extension-catalog.md) for the companion repository, signed index, release flow, and community review process.

## Trust boundary

An extension can:

- Fetch JSON from exact HTTPS hosts declared in its manifest.
- Read exact Home Assistant entities declared in its manifest, as read-only snapshots.
- Read its own declared settings and secret values.
- Return one of three declarative view models: `departures`, `list`, or `key_value`.

An extension cannot:

- Ship native code, shared libraries, executables, WASM, or arbitrary Slint files.
- Access the filesystem, processes, environment variables, imports, or modules.
- Reach an undeclared network host or use credentials in a request URL.
- Call Home Assistant services or receive the Home Assistant token.
- Draw arbitrary UI, render HTML/Markdown, or open windows.
- Read the kiosk configuration or another extension's settings.

The Shop shows the warning and declared permissions before first installation. Installed extensions keep their last good data when a refresh fails. Three consecutive failures from an already constructed host's `refresh` call mark an extension degraded; a later success clears that state.

## Package layout

An extension source directory in the companion catalog repository contains:

```text
org.example.bus-departures/
├── manifest.toml
├── main.rhai
├── README.md
└── LICENSE
```

The `.hkext` archive stores these files at its root. Do not wrap them in another directory.

The kiosk extractor requires `manifest.toml`; the store then requires the manifest's entry script to exist. The runtime accepts an entry script in a subdirectory, but catalog publication policy requires it at the archive root and additionally requires `README.md` and `LICENSE`. Include all four files at the root so the package satisfies both contracts.

Archive paths must be relative and contain only normal path components. Absolute paths, `..`, symlinks, hardlinks, device files, sockets, duplicate entries, and decompression bombs are rejected. See [Packaging](#packaging) for the exact limits.

## Manifest

`manifest.toml` is the trust contract. Unknown fields are rejected by both the catalog parser and the runtime manifest parser, so a typo cannot silently widen a permission.

```toml
id = "org.example.bus-departures"
name = "Bus Departures"
version = "1.0.0"
api_version = 1
min_app_version = "0.1.0"
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

### Manifest fields

| Field | Required | Type | Rules |
| --- | --- | --- | --- |
| `id` | yes | string | Immutable reverse-DNS-style identifier: at least two lowercase labels separated by dots; labels use lowercase ASCII letters, digits, and interior hyphens; each label is 1–63 characters and starts/ends alphanumeric; the whole ID is at most 128 characters. |
| `name` | yes | string | Non-empty display name. |
| `version` | yes | string | Semantic version. Never reuse a published version. |
| `api_version` | yes | integer | Must be `1` for this API. |
| `min_app_version` | yes | string | Semantic version of the oldest compatible kiosk. The catalog rejects an entry when this is newer than the running kiosk. |
| `entry` | yes | string | Relative path ending in `.rhai`, at most 128 bytes. Subdirectories are allowed; absolute paths, backslashes, empty segments, `.` and `..` are rejected. |
| `provides` | yes | string | One of `departures`, `list`, or `key_value`; must match the returned `kind`. |
| `author` | yes | string | Non-empty author name or organization. |
| `license` | yes | string | Non-empty SPDX license identifier. The package must include the license text. |
| `homepage` | no | string | Absolute HTTP or HTTPS URL. HTTPS is recommended. |
| `description` | yes | string | Non-empty plain text; catalog limit is 2,000 bytes. |
| `permissions` | no | table | Declares `http_hosts` and `ha_entities`; both default to empty arrays. |
| `config_schema` | no | array of tables | Declares at most 32 configuration fields. |

The signed catalog entry repeats the manifest metadata. Every shared field must match the packaged manifest exactly: ID, name, version, API version, minimum app version, output kind, permissions, config schema, author, license, homepage, and description. The entry path is validated inside the package because it is not part of the catalog entry.

## Permissions

Permissions are exact allowlists, not capabilities that a script can widen at runtime.

### HTTP hosts

```toml
[permissions]
http_hosts = ["api.example.com", "api.example.com:8443"]
```

- Maximum: 32 entries, each at most 253 bytes.
- Use a DNS hostname or IPv4 address, optionally followed by a non-zero port.
- Hostname labels are 1–63 ASCII letters, digits, or hyphens, with no empty label or leading/trailing hyphen.
- Do not include a scheme, path, query, fragment, user information, wildcard, or IPv6 literal.
- Host matching is case-insensitive. Without a port, only the URL scheme's default port is allowed: 443 for HTTPS, 80 for development HTTP. With a port, that exact port is required.
- Production requests must use HTTPS and must not contain credentials.
- Development mode permits plain HTTP only to `localhost` or `127.0.0.0/8` addresses that are explicitly allowlisted. IPv6 literals cannot be declared. It is not a production option.
- `http_get_json` never follows redirects, including same-host redirects. A provider must return the JSON response directly.

An allowlisted host grants the script the full origin (scheme, host, and port), including every path and query string on that origin. Review scripts for unintended requests and sensitive query parameters, and never put API keys in URLs.

### Home Assistant entities

```toml
[permissions]
ha_entities = ["sensor.departures", "calendar.example"]
```

- Maximum: 64 entries.
- IDs must be exact and lowercase: a domain of lowercase letters/underscores, one dot, then an object ID of lowercase letters/digits/underscores.
- No wildcards are supported.
- `ha_state` returns only a snapshot; there is no service-call API and no token exposure.
- A field of type `entity_id` validates syntax only. It does **not** add the entity to this allowlist.

## Configuration schema

Each `[[config_schema]]` entry describes one key that may appear in the extension's settings or secret file.

| Field | Required | Rules |
| --- | --- | --- |
| `key` | yes | 1–64 lowercase ASCII letters, digits, or underscores; must start with a lowercase letter; unique within the manifest. |
| `label` | yes | Non-empty human-readable label shown by the Shop. |
| `type` | yes | One of the types below. |
| `required` | no | Boolean; defaults to `false`. A required key must be supplied explicitly, even if a default is declared. |
| `default` | no | Value used only when an optional key is absent. Must match the field type and any integer bounds. Secret fields cannot declare defaults. |
| `help` | no | Human-readable explanation. |
| `min` / `max` | no | Signed 64-bit integer bounds. They constrain `integer` values; when both are present, `min` must not exceed `max`. Do not use them for other field types. |
| `options` | no | Non-empty array of exact string choices for `select`. |

### Field types

| Type | TOML/JSON value | Validation |
| --- | --- | --- |
| `string` | string | Any string. |
| `integer` | integer | Signed 64-bit integer; checked against `min` and `max`. |
| `boolean` | boolean | `true` or `false`. |
| `url` | string | Absolute HTTP or HTTPS URL. This does not grant network access; `http_get_json` still enforces the host allowlist and production HTTPS rule. |
| `secret` | string | Read only from the separate secret file; never from normal settings. |
| `entity_id` | string | Lowercase `domain.object_id` syntax. Reading it still requires a matching permission entry. |
| `select` | string | Must exactly match one of `options`. |

### Settings and secret files

Non-secret values live in the kiosk configuration:

```toml
[extensions.settings."org.example.bus-departures"]
stop_id = "REPLACE_ME"
limit = 6
```

Secret values live in a separate file:

```text
/etc/ha-kiosk/extensions/org.example.bus-departures.secret.toml
```

For an extension whose manifest declares a required `api_key` field of type `secret`, the file would contain:

```toml
api_key = "REPLACE_ME"
```

The bus example above does not declare a secret, so it must not include this file.

The operator must own the secret file as the service user and set its mode to `0600`; use a private directory such as `0700`. The kiosk configuration itself is also mode `0600`.

Before each run, the host builds one `config` map for `refresh(config)`:

- Only keys declared in `config_schema` are accepted. Unknown keys are errors.
- Non-secret keys come from `[extensions.settings."<id>"]`; secret keys come from `<id>.secret.toml`.
- Putting a secret in normal settings, or a non-secret value in the secret file, is an error.
- Missing required keys are errors.
- Missing optional keys use their declared default. An optional key without a default is omitted from the map.
- Values are type-checked before the script runs.
- Declared secret values are redacted from host errors and `log` output. Do not log secrets, authorization headers, signed URLs, or full API responses yourself.

## Script API

The entry script must define:

```rhai
fn refresh(config) {
    // Return a map matching the manifest's `provides` value.
}
```

The host registers exactly four functions. Calling an unregistered host function, importing a module, or using disabled dynamic-evaluation/printing symbols fails compilation or execution.

| Function | Behavior |
| --- | --- |
| `http_get_json(url)` | Performs a GET to an allowlisted URL and returns parsed JSON as a Rhai value. HTTPS only in production, no credentials, no redirects, 2xx responses only, valid JSON only, 10-second request timeout, 1 MiB response cap. Errors abort the refresh. |
| `ha_state(entity_id)` | Returns a map with `state` and `attributes` for an allowlisted entity. Returns `()` when the entity is declared but absent from the current snapshot. Undeclared entities are errors. |
| `now()` | Returns the current time as an RFC 3339 string with an explicit offset. The host uses its configured timezone and falls back to UTC when none is supplied. |
| `log(message)` | Writes a bounded debug message. At most 100 messages of 2 KiB each are retained; older messages are discarded. Declared secret values are redacted. |

Rhai's normal language operations are available, but the sandbox has no filesystem, process, environment, module, or arbitrary network API. Scripts run off the UI thread.

### Bus departures example

This example is provider-neutral. Replace the endpoint and field names with the provider's documented format, and keep the host in `http_hosts`.

```rhai
fn refresh(config) {
    let url = "https://api.example.com/departures?stop=" + config.stop_id;
    let data = http_get_json(url);
    let items = [];

    for departure in data.departures {
        if items.len() >= config.limit {
            break;
        }
        items.push(#{
            line: departure.line,
            destination: departure.destination,
            departure: departure.time,
            delay_minutes: departure.delay_minutes ?? 0,
            platform: departure.platform ?? (),
            cancelled: departure.cancelled ?? false,
            remarks: departure.remarks ?? ()
        });
    }

    #{ kind: "departures", title: "Bus", items: items }
}
```

`??` is Rhai's null-coalescing operator. Handle provider errors and missing fields explicitly; do not assume every response has the same shape.

### Resource limits

| Limit | Default |
| --- | ---: |
| Script size | 256 KiB |
| Rhai operations per refresh | 1,000,000 |
| String size | 64 KiB |
| Array entries | 1,000 |
| Map entries | 1,000 |
| Call levels | 32 |
| Expression depth | 64 |
| Expression depth inside function bodies | 32 |
| Functions per script | 64 |
| Variables in scope | 64 |
| Modules | 0 |
| Cooperative wall-clock checks | 10 seconds |
| HTTP timeout per request | 10 seconds |
| HTTP response body | 1 MiB |
| Retained log messages | 100 |
| Bytes per log message | 2 KiB |
| Output rows/items | 100 |

Limits apply while compiling and running the script and while validating its result. Deadline checks are cooperative: a blocking HTTP request already in progress can outlast the refresh deadline by up to its own 10-second timeout. Avoid unbounded loops and large intermediate structures. A limit violation aborts the refresh; it is not a partial success.

## Output schema

Every `refresh` result is a map with a `kind` matching `provides`. Unknown fields and wrong types are rejected. The host validates the complete result before truncating `departures.items`, `list.rows`, or `key_value.items` to 100 entries; a malformed item beyond the cap still rejects the refresh.

All output is data only. Built-in widgets own fonts, colors, layout, and touch targets. Keep labels and values concise.

### `departures`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | string | yes | Must be `"departures"`. |
| `title` | string | no | Widget heading. |
| `items` | array | no | Up to 100 departure objects; defaults to an empty array. |
| `items[].line` | string | yes | Non-empty line or route label. |
| `items[].destination` | string | yes | Non-empty destination label. |
| `items[].departure` | string | yes | Non-empty RFC 3339 timestamp; include an offset. The host checks that it is non-empty, so authors must supply a valid timestamp. |
| `items[].delay_minutes` | integer | no | Signed delay in minutes; defaults to no delay. |
| `items[].platform` | string or null | no | Omit or return `()`. |
| `items[].cancelled` | boolean | no | Defaults to `false`. |
| `items[].remarks` | string or null | no | Short status text. |

### `list`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | string | yes | Must be `"list"`. |
| `title` | string | no | Widget heading. |
| `columns` | array of strings | yes | Header labels; the array must contain at least one string. |
| `rows` | array of arrays of strings | no | Up to 100 rows; defaults to an empty array. Every row must have exactly the same length as `columns`. |

Cells are strings. Convert numbers and booleans in the script rather than returning JSON numbers or booleans.

```json
{"kind": "list", "title": "Trams", "columns": ["Line", "Destination", "Time"],
 "rows": [["1", "Central Station", "21:55"]]}
```

### `key_value`

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `kind` | string | yes | Must be `"key_value"`. |
| `title` | string | no | Widget heading. |
| `items` | array | no | Up to 100 entries; defaults to an empty array. |
| `items[].label` | string | yes | Non-empty field label. |
| `items[].value` | string | yes | Value rendered as text. Convert numbers and booleans to strings in the script. |

```json
{"kind": "key_value", "title": "Stop status",
 "items": [{"label": "Stop", "value": "Central"}, {"label": "Updated", "value": "21:50"}]}
```

### Failure behavior

A compile error, sandbox violation, HTTP error, or malformed output fails the whole refresh. The extension keeps its last good output and reports the error. Manifest, configuration, script-read, and host-construction errors are reported before refresh health tracking begins. For a constructed host, three consecutive compile/runtime/HTTP/output failures mark it degraded in the Shop and Status page; a successful refresh clears the error and failure count.

## Packaging

From the parent directory:

```bash
tar -czf org.example.bus-departures-1.0.0.hkext \
  -C org.example.bus-departures \
  manifest.toml main.rhai README.md LICENSE
sha256sum org.example.bus-departures-1.0.0.hkext
```

On macOS, `shasum -a 256` produces the same digest. The `-C` flag keeps `manifest.toml` at the archive root.

Package limits:

- Suffix: `.hkext`.
- Format: gzipped tar.
- Maximum compressed size: enforced from `extensions.max_package_bytes` (default 5 MiB).
- Maximum decompressed file data: 20 MiB.
- Maximum archive entries: 1,000.
- Accepted entry types: regular files and directories only.

The signed catalog records the exact package URL, lowercase SHA-256 digest, and byte size. The kiosk checks all three before extraction, then checks that the packaged manifest matches the signed entry. Publish the `.hkext` as a stable HTTPS release asset. Never change a published package; publish a new version instead.

## Local testing

1. Validate the manifest and compile the script with the pinned Rhai engine used by the companion catalog CI.
2. Test success, provider error, missing fields, and empty output against recorded fixtures.
3. Confirm that a forbidden host or entity fails before any request/read occurs.
4. Confirm that malformed output fails and that the last good output remains available.
5. Exercise operation, allocation, HTTP-size, redirect, and wall-clock limits.
6. Install from a scratch catalog. Plain HTTP and unsigned catalogs require both `allow_unsigned = true` and `dev_mode = true`; never use that mode in production.
7. Verify offline use after installation and the Shop at 800 × 480 with touch input.

Do not use a live Home Assistant or real provider credentials in automated tests.

## Publishing and review

1. Open a pull request in the companion catalog repository.
2. Include `manifest.toml`, the entry script, `README.md`, `LICENSE`, and tests or a recorded test plan.
3. Keep permissions minimal and explain every requested host and entity in the README.
4. Use placeholders in source, fixtures, screenshots, and documentation. Never include secrets, personal stop IDs, home addresses, or real provider URLs.
5. Wait for CI validation and maintainer review.
6. A maintainer packages the release asset, regenerates and signs the index offline, and publishes the signed index.
7. Never sign a package whose source, permissions, checksum, and size you have not reviewed.

The catalog review covers license, permissions, secrets, read-only Home Assistant access, no service calls, data-only output, tests, documentation, and removal/deprecation. A signed index establishes provenance, not safety.

## Versioning and deprecation

- **Patch:** bug fix with the same manifest contract, permissions, and behavior.
- **Minor:** backward-compatible behavior changes or optional settings that older hosts already tolerate. Set `min_app_version` to the first kiosk that understands any new manifest/config field.
- **Major:** removed or renamed settings, changed types/default behavior, newly emitted output fields, incompatible output changes, or other breaking changes. Output parsers reject unknown fields, so a newly emitted field is not backward-compatible; require a kiosk version that accepts it and use a new `api_version` if the host API contract itself changes.
- **API version:** keep `api_version = 1` while the host function and output contracts remain compatible. A future incompatible host contract requires a new API version.
- **Permissions:** treat any expansion as a trust-sensitive change requiring explicit review and a new version; never widen permissions silently.
- Never reuse a version or replace a published package silently.
- Removing an entry from the catalog does not uninstall it from existing kiosks. Security removals require a newly signed index and a notice in the catalog repository.

## Security rules

- [ ] No native code, WASM, arbitrary Slint files, HTML, images, or custom controls.
- [ ] No filesystem, process, environment, module, import, or dynamic-evaluation access.
- [ ] Only exact declared HTTPS hosts; no credentials, wildcards, or redirects.
- [ ] Only exact declared Home Assistant entities, read-only; no service calls or token access.
- [ ] No secrets or personal/provider-specific identifiers in the manifest, source, fixtures, README, screenshots, package, or logs.
- [ ] Secret fields have no defaults and are supplied only through the `0600` secret file.
- [ ] Unknown configuration keys and wrong value types are rejected before execution.
- [ ] Output matches the declared kind, uses the documented types, and stays within the 100-item cap.
- [ ] The script handles API errors, missing fields, empty lists, and limit violations without crashing the kiosk.
- [ ] The script respects operation, allocation, and wall-clock limits.
- [ ] License text is present and compatible with redistribution.
- [ ] Package URL, SHA-256, byte size, and manifest match the signed catalog entry.
- [ ] Tests cover success, provider error, empty output, malformed output, forbidden host/entity, and offline behavior.
- [ ] The README explains configuration, limitations, disabling/removal, and deprecation.
