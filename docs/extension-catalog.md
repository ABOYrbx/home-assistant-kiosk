# Extension Catalog Specification (Wave 6)

The community catalog is a **separate, public GitHub repository**. The kiosk never clones it; it fetches a signed index and package release assets over HTTPS. This keeps catalog maintenance independent of the kiosk release cycle.

Proposed repository name: `home-assistant-kiosk-extensions` (provisional). The visibility must be public before community use. The maintainer signing procedure is defined below; the named key holder remains a governance decision in `PLAN.md` section 12.

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

### Signing key custody

Generate the key pair only on the offline signing machine. Minisign creates a password-protected private key by default; do not use `-W` to disable that protection.

```bash
umask 077
minisign -G -p /secure/offline/kiosk-catalog.pub -s /secure/offline/kiosk-catalog.key
chmod 0600 /secure/offline/kiosk-catalog.key
```

- Publish only `kiosk-catalog.pub` in the catalog README and configure its base64 public key in the kiosk. Confirm the complete base64 public key, or a SHA-256 digest of its decoded bytes, with another maintainer through a separate trusted channel before relying on it. Minisign's encoded key ID is not a full fingerprint.
- The private key never enters CI, the catalog repository, the kiosk repository, a pull request, an issue, or a log. Do not store it as a CI secret, attach it to a release, or copy it to the connected maintenance machine.
- Keep one encrypted offline backup. A lost key is a rotation event, not a reason to reuse or weaken the old key.
- Sign only an index whose package sources, checksums, and permissions a maintainer has reviewed.

#### Normal key rotation

1. Generate the new pair offline. Add the new public key to the catalog CI trust list and to a kiosk release/update while retaining the old key in both lists.
2. Publish the kiosk trust update and continue signing with the old key until every deployed kiosk trusts the new key and the rollback window has elapsed. Kiosks that still trust only the old key remain compatible during this stage.
3. Publish a known-good index signed with the new key while both keys are still accepted, and verify it on updated kiosks. This is the rollback point for the new key.
4. Only after adoption is confirmed, remove the old key from the CI and kiosk trust lists. Retain the new-key-signed rollback pair.

#### Compromised key

1. Stop signing with the exposed key immediately and publish a revocation notice. Never make another release with it.
2. Generate a new pair offline. In a separately reviewed emergency trust update, add the new public key to the protected CI trust list and ship a kiosk update that adds the new key and removes the compromised key. Keep the old key in CI only until the replacement index is merged, then remove it immediately; never approve another old-key-signed release.
3. Publish a known-good index and rollback pair signed with the new key. Until affected kiosks install the update, they may still accept a malicious signature from the compromised key; take them offline or update them immediately.
4. Revocation is complete only after every affected kiosk has removed the old key. A repository notice alone does not stop signature verification on deployed devices.

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

## Release and signing flow

CI validates release candidates; only a maintainer signs the validated index, and only on the offline signing machine. CI never receives the private key. The signing boundary carries only `index.json` to the offline machine and `index.json.minisig` back. The connected maintenance machine uses public verification material; the private key remains offline.

### 1. Build and validate the candidate

1. Open a catalog pull request with the extension source, manifest, license, and tests or test plan. Do not include a private key or an unsigned-development configuration.
2. A maintainer reviews the source, license, and declared permissions using the community-review checklist below.
3. `scripts/package.sh <id> <version>` builds the `.hkext` and prints its SHA-256 and byte size. Inspect the archive under the [package rules](#package-format), then publish that exact package as an immutable GitHub release asset; never replace a published asset.
4. Regenerate `index.json` with the release asset URL, checksum, and size, and add the candidate to the pull request. Do not reformat or otherwise change it after validation.
5. Review the downloaded release asset's bytes, checksum, size, manifest, and permissions against the candidate index.
6. CI runs the candidate checks listed below and records the candidate commit and the SHA-256 of the exact `index.json` bytes. A candidate may have no signature or the previous index's signature; candidate validation must not treat either as publishable.
7. Wait for a green candidate check and maintainer approval. A failed check stops the release; fix the candidate and repeat validation.

### 2. Sign the exact validated bytes offline

1. Copy only the validated `index.json` to the offline signing machine on encrypted removable media. Keep that media write-protected whenever it is connected to an online machine. Do not connect the signing machine to a network and do not copy the private key back to the connected maintenance machine.
2. On the offline machine, compare its digest with the digest recorded by CI. Stop if they differ.

   ```bash
   sha256sum index.json
   ```

3. Sign the file with the password-protected offline key. Minisign pre-hashes the file by default and writes `index.json.minisig` beside it.

   ```bash
   minisign -S -s /secure/offline/kiosk-catalog.key -m index.json
   ```

4. Verify the new signature on the offline machine before exporting anything. A nonzero exit status stops the release.

   ```bash
   minisign -V -p /secure/offline/kiosk-catalog.pub -m index.json
   sha256sum index.json
   ```

5. Confirm that the second digest still matches CI. Write only `index.json.minisig` to the removable media on the offline machine, eject it, and return it to the connected maintenance machine; the private key stays offline.

### 3. Verify and publish the signed release

1. On the connected maintenance machine, confirm that `index.json` still has the CI-recorded SHA-256 and verify the returned signature. Use a trusted copy of the public key obtained out-of-band, not a key supplied by the release pull request.

   ```bash
   sha256sum index.json
   minisign -V -p /trusted/kiosk-catalog.pub -m index.json
   ```

2. Commit `index.json` and `index.json.minisig` together in the same release pull request. Never commit one without the other, and never edit the index after signing; any byte change requires a new signature.
3. CI reruns the full validation and verifies the signature against its protected public-key trust list. The signed-release check is required in addition to candidate validation and must pass before merge.
4. Merge the signed pair to the catalog's default branch. The kiosk refreshes it on the next daily refresh or manual request.
5. Verify the published `index.json` and `index.json.minisig` over HTTPS with the same digest and trusted-public-key commands. Keep the previous signed pair for rollback only while its signing key remains trusted.

If any transfer, digest, signature, or CI check fails, do not merge or publish. Keep the current signed pair live, correct the candidate, and repeat validation and offline signing. After publication, a rollback restores a known-good pair through a rollback pull request and the same signed-release CI path; its signing key must still be trusted. Before retiring an old key, publish a new-key-signed rollback pair as described under [Normal key rotation](#normal-key-rotation). Never bypass a failed check with `allow_unsigned` or `dev_mode`.

The private signing key never enters CI, the catalog repository, the kiosk repository, a pull request, an issue, a release asset, or a log.

## CI validation

The catalog workflow separates candidate validation from signed-release validation:

- **Candidate check:** validates a proposed index and its packages without requiring a new signature. It cannot publish or mark the candidate as trusted.
- **Signed-release check:** reruns all candidate checks and additionally verifies `index.json.minisig` over the exact `index.json` bytes with the protected public-key trust list. It runs before merge and on the default branch.

The verification workflow and public-key trust list must come from protected repository configuration or a trusted default-branch workflow that the release pull request cannot modify. Stage trust-list changes in a separate reviewed pull request before using a new key; a release pull request cannot introduce its own trusted key or verification workflow.

The signed-release job is a required check and must execute for every release candidate and default-branch update. A missing, stale, malformed, or mismatched signature is a failed result, never a skipped job: GitHub treats a skipped required check as successful. Candidate validation alone never authorizes publication.

Both checks cover:

- JSON Schema for `index.json` and every manifest.
- Unique, reverse-DNS IDs.
- Semver versions and `api_version`.
- Package SHA-256 and size match the immutable GitHub release asset.
- Package inspection enforces the compressed and decompressed size caps and rejects absolute paths, `..`, symlinks, hardlinks, device files, sockets, and duplicate entries before extraction.
- No real secrets, personal stop IDs, home addresses, or API keys.
- `manifest.toml` permissions match the index.
- Rhai scripts compile with the pinned Rhai engine (no execution during CI).
- The example bus extension runs against a fixture in a sandbox test.

CI uses a read-only token and public verification material only. It must not define a signing-key secret, run `minisign -S`, or upload a private key.

## Community review

Every catalog pull request is reviewed by a maintainer before packaging and signing. The review covers:

| Area | What is checked |
| --- | --- |
| License | An SPDX identifier is declared, the license text ships in the package, and redistribution is allowed. |
| Permissions | `http_hosts` and `ha_entities` are minimal and match the script; no wildcard hosts, credentials, or unrelated entities. |
| Secrets | No API keys, tokens, personal stop IDs, home addresses, or real provider URLs appear in the source, fixtures, README, or screenshots. |
| Home Assistant access | `ha_state` reads only declared entities; the script never calls a service and never receives the HA token. |
| UI | Output is data only: `departures`, `list`, or `key_value`. No native code, Slint files, HTML, images, or arbitrary UI. |
| Tests | The extension has a test or a recorded test plan, and CI validates the schema, manifest, script, checksum, and fixture run. |
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
