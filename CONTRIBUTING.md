# Contributing

Thanks for helping with the Home Assistant Kiosk. This repository is currently a planning repository; the application starts in Wave 0.

Read these first:

- [`README.md`](README.md) -- project overview and status.
- [`PLAN.md`](PLAN.md) -- implementation plan and gates.
- [`AGENTS.md`](AGENTS.md) -- working rules for agents and contributors.
- [`docs/github-rules.md`](docs/github-rules.md) -- branch, merge, and access policy.

## Language

English only: code, comments, commits, branches, issues, pull requests, documentation, and UI strings. German prose is not accepted in the repository or on GitHub.

## Who can push

- The repository is private and only **`ABOYrbx`** can push.
- No other collaborators, teams, bots, or deploy keys are allowed.
- Every push must use the `ABOYrbx` GitHub account. Verify with `gh auth status` before pushing.
- External contributions are welcome through pull requests once the repository is public or access is granted.
- Do not create a fork, mirror, or second remote for contribution work.

## Before you start

1. Pick an open **subtask issue** (the wave issues are epics), or open one with the bug/feature template.
2. State whether the change is v1 or post-v1. Wave 6 (extension shop) is post-v1.
3. Create a short-lived branch from `main`:

   ```text
   wave-3/media
   docs/repository-rules
   fix/cache-rollback
   ```

4. Keep the change focused. Do not mix unrelated refactors, formatting, and features.

## Commit messages

Use Conventional Commits:

```text
feat: add websocket heartbeat
fix: handle removed attributes
docs: document repository rules
test: add extension sandbox cases
chore: scaffold ha-kiosk
perf: reduce album-art cache memory
```

One logical change per commit. Do not commit generated build output, secrets, local config, or screenshots with personal data.

## Pull requests

- Use the pull request template.
- Link the subtask issue it closes and the parent wave epic.
- Record the gate evidence: commands run and their result, plus manual/hardware evidence where relevant.
- Keep the pull request in English.
- Close a subtask issue only when its definition of done is met; close the epic only when all subtasks and the wave gate are complete.
- Squash merge only; the branch is deleted after merge.
- The `docs` check must pass.
- Do not force-push `main` or bypass a failing check.

## Code and configuration rules

- Rust 1.95+, edition 2024, Slint 1.19.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo audit` before a release.
- No provider-specific URLs, stop IDs, entity IDs, addresses, or personal data in the source or examples.
- No secrets in code, config examples, tests, issues, or logs.
- Config and cache files are `0600`; the cache directory is `0700`.
- The Home Assistant token is never logged or sent to a third-party host.
- UI targets are at least 44 x 44 logical pixels.
- New behavior needs a test or a recorded manual test.
- Design changes update `DESIGN.md`, `tokens.css`, the Vite prototype, and the Slint port together. No hardcoded colour, spacing, or type value when a token exists. Check 800 × 480, 1024 × 600, and 1280+; 44 px targets; AA contrast.

## Extensions

- Extension source and catalog data belong in the separate `home-assistant-kiosk-extensions` repository.
- Do not add extension code to this repository.
- Follow [`docs/extensions.md`](docs/extensions.md) and [`docs/extension-catalog.md`](docs/extension-catalog.md).
- Extensions are sandboxed Rhai scripts with declared permissions; native code, arbitrary UI, filesystem access, and Home Assistant service calls are not accepted.

## Herdr dispatch

`./herdr.sh` selects 1–4 open subtask issues, creates a new Herdr workspace, starts one agent per issue, and opens one PR per issue. It never merges.

```bash
./herdr.sh
./herdr.sh --issues 11,12 --model opencode-go/deepseek-v4.1-flash
```

Read [`docs/herdr-agent.md`](docs/herdr-agent.md) first.

## Security

Do not open a public issue for a vulnerability. Use GitHub's private security advisory and read [`SECURITY.md`](SECURITY.md). Never include a live token, private key, or personal data in a report.

## Conduct

By participating, you agree to the [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## License

No project license has been chosen yet. Contributions are accepted only with the understanding that the license decision is pending. Do not add a license file without the owner's decision.
