# Build summary — Issue #135 (W6.3.2)

- Shop cards now show the full extension metadata: name, installed and latest version, author, license, description, declared permissions, configuration keys, and errors. Missing metadata is labelled unavailable rather than invented.
- Added four explicit configuration states — `Configured`, `Configuration required`, `No configuration required`, and `Configuration unavailable` — derived from the installed manifest schema with `Manifest::build_config`, so required settings, types, and secret placement are checked truthfully.
- The background manager now builds installed-only entries from the installed `manifest.toml` and carries complete snapshots in `ShopEvent::CatalogOffline`, so installed extensions keep their metadata offline; missing or invalid manifests surface as errors instead of silently claiming no permissions.
- Exposed configuration status, installed version, and metadata availability through the Slint bridge and card, with wrapping verified at 375–2048 px and 44 px touch targets preserved.
- Added view-model, manager, and Slint UI tests and updated `docs/extensions.md` and `config.example.toml` to document the new states.
- Verification: `cargo fmt --check`, docs check, 40 Python tests, 60 library tests, and 68 issue-specific Rust tests pass; clippy is clean for the library and touched targets. Full `cargo test` / `--all-targets` clippy remain blocked by pre-existing UI test targets that reference undeclared Slint properties and were not touched by this change.
