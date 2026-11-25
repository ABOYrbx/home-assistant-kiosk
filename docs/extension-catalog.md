# Extension Catalog Specification (Wave 6)

The community catalog is a **separate, public GitHub repository**. The kiosk never clones it; it fetches a signed index and package release assets over HTTPS. This keeps catalog maintenance independent of the kiosk release cycle.

Proposed repository name: `home-assistant-kiosk-extensions` (provisional). The visibility must be public before community use; the private-key custody is an open decision (see `PLAN.md` section 12).

See also: [Extension Author Guide](extensions.md) for the manifest, script API, output schema, configuration, packaging, and author checklist.

## Repository layout

```text
home-assistant-kiosk-extensions/
├── README.md
├── index.json
├── index.json.minisig
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
│   ├── validate.sh
│   └── package.sh
└── .github/workflows/validate.yml
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
      "package_url": "https://github.com/example/home-assistant-kiosk-extensions/releases/download/org.example.bus-departures-v1.0.0/org.example.bus-departures-1.0.0.hkext",
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
catalog_url = "https://raw.githubusercontent.com/example/home-assistant-kiosk-extensions/main/index.json"
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
      "source_catalog": "https://raw.githubusercontent.com/example/home-assistant-kiosk-extensions/main/index.json",
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
5. A maintainer signs `index.json` offline with Minisign.
6. Both `index.json` and `index.json.minisig` are committed to the catalog repository.
7. CI verifies the schema and signature with the public key.
8. The kiosk refreshes its catalog on its next daily refresh or manual request.

The private signing key never enters CI, the catalog repository, or the kiosk repository.

## CI validation

The catalog CI checks:

- JSON Schema for `index.json` and every manifest.
- Unique, reverse-DNS IDs.
- Semver versions and `api_version`.
- Package SHA-256 and size match the GitHub release asset.
- No real secrets, personal stop IDs, home addresses, or API keys.
- `manifest.toml` permissions match the index.
- Rhai scripts compile with the pinned Rhai engine (no execution during CI).
- The example bus extension runs against a fixture in a sandbox test.
- The index signature verifies with the public key.

## Community review

Every catalog pull request is reviewed by a maintainer before packaging and signing. This includes new extensions, updates, permission changes, and changes to the entry script or output schema. A signed catalog proves provenance, not safety; CI and approval do not make third-party code trustworthy.

The review follows this sequence:

1. The author submits the source, manifest, README, license, fixtures or test plan, and the evidence listed below.
2. CI validates the schemas, manifest, script syntax, permissions, checksums, and fixture run.
3. A maintainer reviews the exact source revision and records a decision. Automated checks do not replace source review.
4. Requested changes are reviewed again. Approval applies only to the recorded commit or package; any later source change invalidates it.
5. After approval, the maintainer builds the package, verifies its SHA-256 and size, and signs the index offline. The reviewer confirms that the package matches the approved source before signing.

| Area | What is checked |
| --- | --- |
| License | An SPDX identifier is declared, the full license text and copyright notices ship in the package, and the terms allow redistribution through the catalog. |
| Permissions | Every HTTP host and HA entity is necessary and justified. Exact allowlists match the manifest, index, and script; there are no wildcard hosts, credentials in URLs, or unrelated entities. |
| Secrets | Source, fixtures, README, screenshots, logs, and package contents contain no tokens, API keys, personal stop IDs, addresses, personal entity IDs, or private provider endpoints. Examples use placeholders. |
| Home Assistant access | `ha_state` reads only declared entities and returns a snapshot. There are no service calls, Home Assistant URLs or credentials, token access, filesystem access, process access, or environment access. |
| UI | Output is data only: `departures`, `list`, or `key_value`. There is no native code, WASM, arbitrary Slint, HTML, executable content, or custom UI; built-in widgets own presentation. |
| Tests | Fixture-based tests or a recorded test plan cover the scenarios below. CI validates the schema, manifest, script, checksum, and fixture run without live credentials. |
| Removal | The README explains disable, remove, and retained settings/secrets. Catalog withdrawal and security withdrawal do not silently uninstall or replace packages on existing kiosks. |

### Submission evidence

Include this evidence in the catalog pull request. Do not attach secrets, unredacted logs, personal configuration, or a live Home Assistant token.

- **License:** SPDX identifier, full license text, copyright notices, and any redistribution conditions.
- **Permissions:** A short justification for each HTTP host and HA entity, including where the script uses it.
- **Secrets:** The secret-scan result for source, fixtures, documentation, screenshots, and package contents; rotate and remove any exposed credential before review.
- **Home Assistant access:** The declared read-only entity list and confirmation that no service call, token, or Home Assistant endpoint is used.
- **UI:** The declared output kind and confirmation that the package contains no native code or arbitrary UI.
- **Tests:** Commands, fixture results or a recorded test plan, expected output, and any untested limitations.
- **Removal:** Operator steps for disabling and removing the extension, plus the catalog withdrawal/deprecation plan.

### Test evidence

Use recorded fixtures; never require a live Home Assistant instance or credentials. Tests must assert the observable result, not only that the script compiles. A recorded manual test plan must state the environment, steps, expected result, actual result, and date.

| Scenario | Expected evidence |
| --- | --- |
| Successful refresh | A fixture produces the declared data-only output with the expected fields and item cap. |
| API failure | The failure is visible, the kiosk does not crash, and the last good output is retained. |
| Empty or missing data | Empty, missing, or unavailable data is explicit and does not blank the whole page. |
| Forbidden host or entity | Access outside the manifest allowlist is rejected before any request or entity read, including cross-host redirects. |
| Malformed output | Unknown fields, wrong types, and excess items are rejected without crashing the kiosk. |
| Resource limits | Deliberate looping or oversized values hit operation, allocation, or wall-clock limits while the UI thread remains responsive. |
| Removal | Install/remove leaves no installed version or `installed.json` entry; retained settings and the secret file are documented and handled by the operator. |

### Review decision and record

Record the decision in the pull request so the signed release has an audit trail:

- Reviewer: maintainer name and review date.
- Commit or package: reviewed commit SHA and, once built, package SHA-256 and byte size.
- Decision: approve, request changes, or reject, with the reason.
- Evidence: CI run plus fixture or manual-test results for every required scenario.
- Follow-up: requested changes, deprecation/removal notes, or `none`.

A maintainer never signs an index entry that has not passed review. Security removals follow the process below.

### Reviewer checklist

- [ ] Extension ID is reverse-DNS and version is semver.
- [ ] License is present and compatible with redistribution.
- [ ] Permissions are minimal and match the manifest, index, and script.
- [ ] No Home Assistant service calls, filesystem, process, or environment access.
- [ ] Output is `departures`, `list`, or `key_value` and within limits.
- [ ] No native code, Slint files, HTML, executable content, or arbitrary UI.
- [ ] No secrets, personal data, or private provider endpoints in source, fixtures, README, screenshots, or logs.
- [ ] Script handles API errors, missing fields, and empty lists.
- [ ] Tests or a recorded test plan cover every scenario in the test-evidence table.
- [ ] Package checksum and size match the approved source and release asset.
- [ ] README explains configuration, limitations, and removal.
- [ ] Review decision and evidence are recorded against the exact revision.

## Catalog removal and deprecation

| Action | Effect |
| --- | --- |
| Disable | In **Shop → Installed**, disable the extension. Execution stops, but installed files, configuration, and secrets remain so it can be re-enabled. |
| Remove | In **Shop → Installed**, remove the extension. The kiosk deletes all installed versions and its `installed.json` entry; configuration settings and the secret file are retained. |
| Catalog withdrawal | Remove the entry from `index.json`, publish a new signed index, and keep the package URL available for existing installations. New installs and updates disappear after a kiosk refreshes the catalog; installed extensions remain installed and usable. |
| Security withdrawal | Publish a new signed index without the entry and a notice in the catalog README. Withdrawal is not a remote uninstall: tell operators to disable or remove the extension. If the signing key is affected, follow the revocation and rotation process above. |

After **Remove**, the operator may delete `[extensions.settings."<id>"]` from the kiosk configuration and `/etc/ha-kiosk/extensions/<id>.secret.toml` when the data is no longer needed. Remove does not delete those files automatically. Never put their contents in an issue or review comment.

For a planned deprecation, document the replacement and migration steps in the extension README and catalog notice. Keep the last compatible package available while operators migrate. Never reuse a version or replace a published package silently. The kiosk disables an extension that repeatedly fails its sandbox or output validation and shows the error.

## Hosting

- Use a public GitHub repository with raw `index.json` over HTTPS.
- Do not require a GitHub token in the kiosk.
- Do not use GitHub API rate-limited endpoints for catalog reads.
- Keep package URLs on HTTPS; GitHub release assets are the default.
- A mirror is allowed if the signed index and checksums remain identical.
