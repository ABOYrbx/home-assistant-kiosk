# GitHub Repository Rules

This document is the authoritative policy for the GitHub repository `ABOYrbx/home-assistant-kiosk`. It covers access, branches, merges, releases, security settings, and how the policy is enforced.

## Current access

- Repository: **public** since 2026-11-25 (issue #10). Anyone may read and fork it.
- Owner and only collaborator: **`ABOYrbx`**. Only `ABOYrbx` may push.
- No teams, no bots, no deploy keys, and no outside collaborators.
- `gh` must be authenticated as `ABOYrbx` before any push.
- Commit identity: `ABOYrbx <141421787+ABOYrbx@users.noreply.github.com>`.

Verify:

```bash
gh auth status
git config user.name
git config user.email
gh api repos/ABOYrbx/home-assistant-kiosk/collaborators \
  --jq '.[] | "\(.login) \(.permissions.admin)"'
```

## Branch policy for `main`

The `main-protection` repository ruleset enforces this policy. `scripts/apply-github-settings.sh` applies and verifies it.

- Pull request required before merging.
- No direct pushes to `main`; no force-pushes; no branch deletion.
- Required status check: `docs`.
- Branch must be up to date before merging (`strict`).
- Conversation threads must be resolved.
- Linear history required.
- Squash merge only; merge commits and rebase merges disabled.
- Delete the branch after merge.
- Administrators are subject to the same rules (`enforce_admins`).
- Push restriction: `ABOYrbx` only.
- Required approving reviews: `0`, because a sole owner cannot approve their own pull request. CODEOWNERS still requests review and conversation resolution still applies.

Additional release policy:

- Releases and tags (`v*`) are created by `ABOYrbx` from `main` only.
- Tags are annotated; never move or delete a published tag.
- Release assets include checksums.
- The version follows semver; the extension catalog has its own versions.

## Enforcement status

The repository is public, so GitHub Free exposes repository rulesets for it. The `main-protection` ruleset is active and was verified after issue #10:

- Enforcement is `active` and targets `~DEFAULT_BRANCH`.
- `bypass_actors` is empty, so administrators are subject to the rules.
- The rules cover deletion, non-fast-forward pushes, linear history, a pull request, and the required `docs` status check.
- The `docs` check is strict, so the branch must be up to date before merging.
- Required approving reviews are `0`; CODEOWNERS still requests review and conversation resolution applies.

Before 2026-11-25 the repository was private on GitHub Free, which does not expose branch protection or rulesets. During that period the policy was process-only. If the repository is ever made private again on a plan without protection support, the API returns:

```text
Upgrade to GitHub Pro or make this repository public to enable this feature.
```

In that case the script warns, the policy remains in force as process, and the technical block must be restored before the next merge. The script prefers a repository ruleset and falls back to classic branch protection when rulesets are unavailable. It verifies the applied state and exits non-zero when a supported feature could not be applied.

## Available repository settings

These are applied by the script:

| Setting | Value |
| --- | --- |
| Default branch | `main` |
| Issues | enabled |
| Projects | disabled |
| Wiki | disabled |
| Discussions | disabled |
| Merge commits | disabled |
| Rebase merges | disabled |
| Squash merges | enabled |
| Squash title | pull request title |
| Squash message | pull request body |
| Delete branch on merge | enabled |
| Auto-merge | enabled |
| Update branch | enabled |
| Web commit signoff | not required |

Security settings: Dependabot alerts, Dependabot security updates, private vulnerability reporting, secret scanning, and push protection are enabled. Actions use read-only `GITHUB_TOKEN` permissions and are limited to GitHub-owned or verified actions.

The repository's Actions policy is `allowed_actions: selected` with only
GitHub-owned and verified actions permitted. A third-party action such as
`dtolnay/rust-toolchain` is not on that list, and a workflow that references one
fails at startup with no jobs. Install Rust in workflows with the runner's
`rustup` instead:

```yaml
- name: Set up Rust
  run: |
    rustup toolchain install stable --profile minimal
    rustup default stable
    rustup component add rustfmt clippy
```

## Pull requests

- Use the pull request template.
- Link the subtask issue it closes and the parent wave epic.
- Record gate evidence: commands run and their result; manual/hardware evidence where relevant.
- Keep one subtask or one coherent group per pull request.
- Do not close a subtask issue until its definition of done is met; do not close the epic until every subtask and the wave gate are complete.
- The final push must be made with `ABOYrbx`.

Branch naming:

```text
wave-0/scaffold
wave-1/ha-client
wave-3/media
docs/repository-rules
fix/cache-rollback
```

Commit messages follow Conventional Commits:

```text
feat: add websocket heartbeat
fix: handle removed attributes
docs: document repository rules
chore: scaffold ha-kiosk
test: add extension sandbox cases
```

## Issues

- Use the bug or feature issue form.
- Include reproduction steps, expected/actual behavior, environment, and logs with secrets redacted.
- Feature requests state whether they are v1 or post-v1.
- Extension submissions belong in the separate catalog repository, not this one.
- Do not open a public issue for a security vulnerability; use the private security advisory.
- Blank issues are disabled; use the templates.

## Security settings

Enable and keep enabled:

- Dependabot alerts.
- Dependabot security updates.
- Private vulnerability reporting when the plan supports it.
- Secret scanning and push protection when the plan supports it.
- Actions token permissions: read-only by default; workflows cannot approve pull requests.
- Allowed Actions: GitHub-owned and verified actions only.

Do not open a public issue for a security vulnerability. Use the private advisory form; private vulnerability reporting is enabled. Secret scanning and push protection are enabled and must not be bypassed.

## Branches and tags

- `main` is the only long-lived branch.
- Feature branches are short-lived and deleted after merge.
- Release tags use `vMAJOR.MINOR.PATCH`.
- A release is built from `main`, with checksums and a changelog.
- The extension catalog is a separate repository and follows its own release rules.

## Extensions

- Extension source and catalog data do **not** live in this repository.
- The companion catalog is `home-assistant-kiosk-extensions` (provisional name).
- Catalog contributions follow the review and signing rules in [`docs/extension-catalog.md`](extension-catalog.md).
- The Minisign private key stays offline and is never committed, pasted into CI, or shared through GitHub.

## Emergency changes

There is no silent bypass. If a rule blocks an urgent fix:

1. Say so in the issue or pull request.
2. Use a normal pull request with the smallest possible change.
3. If the platform blocks merging and no second reviewer exists, the owner may temporarily adjust the rule, record why, and restore it immediately.
4. Never disable a rule to push directly to `main` without recording the reason.

## Verification

```bash
# Repository settings
gh api repos/ABOYrbx/home-assistant-kiosk \
  --jq '{default_branch,allow_squash_merge,allow_merge_commit,allow_rebase_merge,delete_branch_on_merge,allow_auto_merge,allow_update_branch}'

# Collaborators
gh api repos/ABOYrbx/home-assistant-kiosk/collaborators \
  --jq '.[] | .login'

# Ruleset (public repository)
gh api repos/ABOYrbx/home-assistant-kiosk/rulesets \
  --jq '.[] | select(.name == "main-protection")'
gh api repos/ABOYrbx/home-assistant-kiosk/rulesets \
  --jq '.[] | select(.name == "main-protection") | .id'

# Ruleset fields (replace <id>)
gh api repos/ABOYrbx/home-assistant-kiosk/rulesets/<id> \
  --jq '{enforcement, rules: [.rules[].type], bypass_actors, conditions}'

# Classic protection, only when rulesets are unavailable
gh api repos/ABOYrbx/home-assistant-kiosk/branches/main/protection

# Docs check
python3 scripts/check-docs.py

# Apply settings and verify protection
scripts/apply-github-settings.sh
```
