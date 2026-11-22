# Security Policy

## Supported versions

The project has no release yet. Security fixes apply to `main` until the first release; after that, the latest release and `main` are supported.

## Reporting a vulnerability

Do **not** open a public issue. GitHub's private vulnerability reporting is currently unavailable on GitHub Free for private repositories. If the repository's **Security** tab offers "Report a vulnerability", use it. Otherwise, contact the maintainer `@ABOYrbx` through the private contact method listed on their GitHub profile and ask for a secure channel before sharing details. If no private channel is available, open a minimal issue titled "Private security contact requested" with no vulnerability details and wait for a response.

Include:

- A short description and impact.
- The affected file, commit, or component.
- Reproduction steps or a minimal proof of concept.
- The environment: kiosk version, Home Assistant version, OS, and hardware.
- Whether the issue also affects the extension catalog or a published extension.

Never include a live Home Assistant token, Minisign private key, private API key, personal address, or personal entity IDs. Redact logs before attaching them.

There is no paid bug bounty and no guaranteed response time. Reports are handled on a best-effort basis. Please allow time for a fix before public disclosure.

## Scope

In scope:

- Kiosk source, build, configuration, cache, and deployment scripts.
- Home Assistant WebSocket handling, token handling, and service calls.
- Extension trust boundary: signature verification, package extraction, Rhai sandbox, permissions, and resource limits.
- Documentation that could cause an insecure deployment.

Out of scope:

- Vulnerabilities in Home Assistant itself; report them to the Home Assistant project.
- Vulnerabilities in third-party extensions; report them to the catalog maintainers and the extension author.
- Vulnerabilities in Slint, Rust crates, or the operating system; report upstream and, if the kiosk needs a workaround, open a private advisory.
- Denial of service that requires physical access to the device.
- Issues that require the attacker to already know the Home Assistant token.

## Security model

- The Home Assistant token is read from a protected file or environment variable and is never logged.
- TLS validation is on by default; `allow_invalid_certs` is a documented last resort.
- Image tokens are sent only to the Home Assistant origin.
- Community extensions are untrusted: signed catalog, SHA-256 package checks, Rhai sandbox, declared permissions, no service calls, no token, and hard resource limits.
- A signed catalog proves provenance, not safety. The shop shows permissions and a warning before installation.

## Secrets in the repository

If a secret is committed:

1. Revoke or rotate it immediately.
2. Remove it from the working tree and history.
3. Report the exposure through the private advisory if it affects users.
4. Add a regression check where possible.

## Extension catalog

The catalog lives in a separate repository. Its signing key is offline. A compromised key, a malicious package, or a signature bypass is a security issue for both repositories; report it privately and do not publish the details before a fix.
