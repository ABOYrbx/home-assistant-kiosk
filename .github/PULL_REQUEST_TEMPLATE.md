## Summary

<!-- What changed and why, in English. -->

## Issue / wave

Closes #

Wave: <!-- 0–6, or "docs" for repository policy work -->

## Gate evidence

<!-- Paste commands and results. Write "not applicable" for docs-only changes. -->

```text
$ cargo fmt --check
$ cargo clippy --all-targets -- -D warnings
$ cargo test
$ python3 scripts/check-docs.py
```

- [ ] The relevant wave gate in `PLAN.md` is met, or this is a docs/policy change.
- [ ] Hardware evidence is attached or the item is marked "not verified on hardware".

## Checklist

- [ ] English only in code, comments, docs, commits, and this pull request.
- [ ] No secrets, tokens, private keys, personal entity IDs, stop IDs, addresses, or internal hostnames.
- [ ] No provider-specific URL or ID was added to the kiosk source.
- [ ] Documentation and `config.example.toml` match the behavior.
- [ ] New behavior has a test or a recorded manual test.
- [ ] Error, offline, empty, and unavailable states are handled where relevant.
- [ ] The push was made with the `ABOYrbx` account.
- [ ] I did not force-push or bypass a failing check.
