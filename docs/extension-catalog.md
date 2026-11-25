# Extension Catalog Specification (Wave 6)

The community catalog is a **separate, public GitHub repository**. The kiosk never clones it; it fetches a signed index and package release assets over HTTPS. This keeps catalog maintenance independent of the kiosk release cycle.

Repository: [`ABOYrbx/home-assistant-kiosk-extensions`](https://github.com/ABOYrbx/home-assistant-kiosk-extensions). It is public for community use. `ABOYrbx` is the designated production-key custodian and will generate the Minisign private key on encrypted offline storage before the first real release; no production key exists yet (see `PLAN.md` section 12).

See also: [Extension Author Guide](extensions.md) for the manifest, script API, output schema, configuration, packaging, and author checklist.

## Repository layout

```text
home-assistant-kiosk-extensions/
├── README.md
├── index.json
├── index.json.minisig
├── catalog.pub
├── schema/
│   ├── index.schema.json
│   └── manifest.schema.json
├── extensions/
│   └── org.example.bus-departures/
│       ├── manifest.toml
│       ├── main.rhai
│       ├── README.md
│       └── LICENSE
├── scripts/
│   ├── validate.py
│   ├── validate.sh
│   ├── package.py
│   └── package.sh
├── tests/
└── .github/
    ├── CODEOWNERS
    └── workflows/validate.yml
```

- `index.json` is generated from the extension manifests and release assets.
- `index.json.minisig` is a Minisign signature over the exact bytes of `index.json`.
- `schema/` contains JSON Schemas used by CI and by the kiosk author tooling.
- `extensions/<id>/` holds the source of each extension.
- `scripts/package.sh` builds a `.hkext` and prints its SHA-256 and size.
- The CI workflow validates schemas, manifests, scripts, and checksums. It never holds the signing private key.

## index.json

```json
{
  "schema_version": 1,
  "generated_at": "2026-11-22T20:00:00Z",
  "extensions": [
    {
      "id": "org.example.bus-departures",
      "name": "Bus Departures",
      "version": "1.0.0",
      "api_version": 1,
      "min_app_version": "0.6.0",
      "description": "Shows upcoming bus departures for a configured stop.",
      "author": "Example Author",
      "license": "MIT",
      "homepage": "https://example.com/bus-departures",
      "provides": "departures",
      "permissions": {
        "http_hosts": ["api.example.com"],
        "ha_entities": []
      },
      "config_schema": [
        {
          "key": "stop_id",
          "label": "Stop ID",
          "type": "string",
          "required": true
        }
      ],
      "package_url": "https://github.com/ABOYrbx/home-assistant-kiosk-extensions/releases/download/org.example.bus-departures-v1.0.0/org.example.bus-departures-1.0.0.hkext",
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "size_bytes": 12345
    }
  ]
}
```

Rules:

- `schema_version` is `1`.
- `generated_at` is UTC ISO 8601.
- One entry per extension; multiple versions are not listed. The latest compatible version is published.
- `id` is immutable and reverse-DNS.
- `package_url` is HTTPS and stable; GitHub release assets are recommended.
- `sha256` is lowercase hex over the exact `.hkext` bytes.
- `size_bytes` must match the download.
- `permissions` is the exact allowlist the host enforces.
- `config_schema` mirrors `manifest.toml`.
- Unknown fields are rejected by the kiosk parser.

## Signature

The index is signed with [Minisign](https://jedisct1.github.io/minisign/) using an offline private key:

```bash
minisign -S -s /secure/offline/kiosk-catalog.key -m index.json
```

This creates `index.json.minisig`. The kiosk is configured with one or more Minisign public keys:

```toml
[extensions]
public_keys = ["RWQ..."]
allow_unsigned = false
```

Verification rules:

1. Fetch `index.json` and `index.json.minisig` over HTTPS.
2. Verify the signature over the exact index bytes against a configured public key.
3. Reject if no configured key matches.
4. Parse and validate the index.
5. Cache the last verified index for offline use.

Key rotation: publish a new public key in the catalog repository and in a kiosk release/update first; sign the index with the new key; keep the old key in the config for a transition period; then remove it. A compromised key needs a revocation notice and a new release.

### Signing key custody

- Generate the Minisign key pair on an offline machine or hardware token; record the public key in the catalog README and in the kiosk configuration.
- The private key never enters CI, the catalog repository, the kiosk repository, a pull request, an issue, or a log. Do not store it as a CI secret.
- Keep one encrypted offline backup. A lost key is a rotation event, not a reason to reuse or weaken the old key.
- Sign only an index whose package sources, checksums, and permissions a maintainer has reviewed.
- If the private key is exposed, publish a revocation notice, rotate the key, and ship a kiosk configuration update.

`allow_unsigned = true` exists only for local development. A production shop must reject unsigned catalogs and must show a persistent warning if the flag is on.

## Kiosk trust configuration

```toml
[extensions]
enabled = true
catalog_url = "https://raw.githubusercontent.com/ABOYrbx/home-assistant-kiosk-extensions/main/index.json"
public_keys = ["RWQ..."]
allow_unsigned = false
dev_mode = false
max_catalog_bytes = 2097152
max_package_bytes = 5242880
store_path = "/var/lib/ha-kiosk/extensions"
```

- `catalog_url` must be HTTPS. HTTP is accepted only when `allow_unsigned` and `dev_mode` are both true.
- `max_catalog_bytes` caps the index download; `max_package_bytes` caps both the signed `size_bytes` and the download.
- `public_keys` lists Minisign public keys in base64. Verification succeeds when any configured key matches, so key rotation lists the old and the new key during the transition.
- An unsigned catalog is accepted only when `allow_unsigned` and `dev_mode` are both true. Production configurations leave both false.
- The last verified index is cached as `catalog-cache.json` (mode `0600`) next to the store and is used when a refresh fails.

## Installed state

The kiosk keeps installed extensions under `store_path`:

```text
/var/lib/ha-kiosk/extensions/
├── installed.json                      # mode 0600
├── catalog-cache.json                  # mode 0600, last verified index
└── org.example.bus-departures/
    ├── 1.0.0/                          # mode 0700, files 0600
    └── 0.9.0/                          # one previous version for rollback
```

`installed.json`:

```json
{
  "schema_version": 1,
  "extensions": [
    {
      "id": "org.example.bus-departures",
      "version": "1.0.0",
      "enabled": true,
      "manifest_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "source_catalog": "https://raw.githubusercontent.com/ABOYrbx/home-assistant-kiosk-extensions/main/index.json",
      "installed_at": "2026-11-23T12:00:00Z"
    }
  ]
}
```

- Installation extracts into a staging directory inside the store and renames it into place, so a failed install never leaves a partial version.
- Only the active version and one previous version are kept. Rollback switches the record back and removes the newer files.
- The package manifest must match the signed catalog entry for id, version, provides, permissions, and config schema; a mismatch is rejected before anything is installed.

## Package format

- Suffix: `.hkext`.
- Format: gzipped tar (`tar -czf`).
- Root: the archive contains `manifest.toml`, the entry script, `README.md`, and `LICENSE` at its root; no wrapping directory.
- Required: `manifest.toml`, `entry` script, `README.md`, `LICENSE`.
- Maximum compressed size: 5 MiB (configurable).
- Maximum decompressed size: 20 MiB.
- Rejected: absolute paths, `..`, symlinks, hardlinks, device files, sockets, duplicate entries.
- The SHA-256 and size in the signed index are the authority. The kiosk verifies both before extraction.

## Release flow

1. Maintainer reviews the extension source and permissions.
2. `scripts/package.sh <id> <version>` builds the `.hkext` and prints SHA-256 and size.
3. The package is attached to a GitHub release.
4. `index.json` is regenerated with the package URL, checksum, and size.
5. `scripts/validate.sh` checks the complete catalog and source tree before signing.
6. A maintainer signs `index.json` offline with Minisign.
7. Both `index.json` and `index.json.minisig` are committed to the catalog repository.
8. CI verifies the schema and signature with the public key.
9. The kiosk refreshes its catalog on its next daily refresh or manual request.

The private signing key never enters CI, the catalog repository, or the kiosk repository.

## CI validation

The current catalog CI checks:

- JSON Schema for `index.json` and every indexed manifest.
- Unique, reverse-DNS IDs.
- Semver versions and integer `api_version` values.
- Source-tree safety, including symlinks, native/UI files, and common secret patterns.
- `manifest.toml` permissions and configuration match the index.
- The index signature verifies with the public key.

The following release checks are planned but are not implemented by the scaffold workflow yet:

- Package SHA-256 and size match the GitHub release asset.
- Rhai scripts compile with the pinned Rhai engine, without execution during CI.
- The example bus extension runs against a fixture in a sandbox test.

## Community review

Every catalog pull request is reviewed by a maintainer before packaging and signing. The review covers:

| Area | What is checked |
| --- | --- |
| License | An SPDX identifier is declared, the license text ships in the package, and redistribution is allowed. |
| Permissions | `http_hosts` and `ha_entities` are minimal and match the script; no wildcard hosts, credentials, or unrelated entities. |
| Secrets | No API keys, tokens, personal stop IDs, home addresses, or real provider URLs appear in the source, fixtures, README, or screenshots. |
| Home Assistant access | `ha_state` reads only declared entities; the script never calls a service and never receives the HA token. |
| UI | Output is data only: `departures`, `list`, or `key_value`. No native code, Slint files, HTML, images, or arbitrary UI. |
| Tests | The extension has a test or a recorded test plan. CI validates the schema, manifest, package safety, and signature; release-asset and fixture checks remain planned. |
| Removal | The README states how to disable or remove the extension, and the catalog entry can be withdrawn without breaking installed kiosks. |

A reviewer may approve, request changes, or reject a submission. A maintainer never signs an index entry that has not passed review. Security removals follow the process below.

## Review checklist

- [ ] Extension ID is reverse-DNS and version is semver.
- [ ] License is present and compatible with redistribution.
- [ ] Permissions are minimal (HTTP hosts and HA entities).
- [ ] No Home Assistant service calls, filesystem, process, or environment access.
- [ ] Output is `departures`, `list`, or `key_value` and within limits.
- [ ] No native code, Slint files, HTML, images, or arbitrary UI.
- [ ] No secrets, personal data, or real provider IDs in source, fixtures, or README.
- [ ] Script handles API errors, missing fields, and empty lists.
- [ ] Tests or a recorded test plan cover success, API error, and empty output.
- [ ] Package checksum and size match.
- [ ] README explains configuration and limitations.
- [ ] Removal/deprecation plan is clear.

## Catalog removal and deprecation

- Mark a package unavailable by removing it from `index.json`; installed kiosks keep the package.
- For a security removal, publish a new signed index and a notice in the repository README.
- Never reuse a version or replace a published package silently.
- The kiosk disables an extension that repeatedly fails its sandbox or output validation and shows the error.

## Hosting

- Use a public GitHub repository with raw `index.json` over HTTPS.
- Do not require a GitHub token in the kiosk.
- Do not use GitHub API rate-limited endpoints for catalog reads.
- Keep package URLs on HTTPS; GitHub release assets are the default.
- A mirror is allowed if the signed index and checksums remain identical.
