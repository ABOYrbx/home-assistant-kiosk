# GitHub Repository Rules

This document is the authoritative policy for the GitHub repository `ABOYrbx/home-assistant-kiosk`. It covers access, branches, merges, releases, security settings, and the current GitHub plan limitation.

## Current access

- Repository: **private**.
- Owner and only collaborator: **`ABOYrbx`**.
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

Policy (applied with [`scripts/apply-github-settings.sh`](../scripts/apply-github-settings.sh) when the GitHub plan supports it):

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

## Current limitation: GitHub Free + private repository

GitHub Free does **not** enable branch protection or repository rulesets for private repositories. The API returns:

```text
Upgrade to GitHub Pro or make this repository public to enable this feature.
```

Until the repository is public or on a plan that supports protection:

- The rules above are the project policy, not a technical block.
- The `docs` check runs on every push and pull request but cannot prevent a merge by itself.
- CODEOWNERS and pull request templates document the expected review.
- Direct pushes to `main` are a policy violation even if GitHub accepts them.
- Do not bypass the documented process because the platform cannot enforce it.

To make the rules technically enforceable, the owner chooses one of:

1. Make the repository public and rerun the settings script.
2. Upgrade to GitHub Pro (or higher) and rerun the settings script.
3. Keep it private on the current plan and accept process-only enforcement.

The script attempts a repository ruleset first and classic branch protection second, and prints a clear message when neither is available. The remaining step is tracked in [issue #10](https://github.com/ABOYrbx/home-assistant-kiosk/issues/10).

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
| Auto-merge | not available on the current GitHub Free private plan; the script requests it |
| Update branch | enabled |
| Web commit signoff | not required |

Security settings currently available: Dependabot alerts and Dependabot security updates are enabled. Private vulnerability reporting and secret scanning are **not** available on GitHub Free for a private repository; enable them when the repository is public or on GitHub Pro. Actions use read-only `GITHUB_TOKEN` permissions and are limited to GitHub-owned or verified actions.

## Pull requests

- Use the pull request template.
- Link the issue it closes and name the wave.
- Record gate evidence: commands run and their result; manual/hardware evidence where relevant.
- Keep one wave or one focused change per pull request.
- Do not mix documentation-only changes with code changes unless the gate requires it.
- Do not mark a wave complete while a gate is unverified.
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

GitHub Free private repositories do not expose private vulnerability reporting or secret scanning through the API. Until the repository is public or on GitHub Pro, do not open a public security issue; read [`SECURITY.md`](../SECURITY.md) for the current reporting path.

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

# Rulesets (requires a plan that supports them)
gh api repos/ABOYrbx/home-assistant-kiosk/rulesets

# Classic protection (requires a plan that supports it)
gh api repos/ABOYrbx/home-assistant-kiosk/branches/main/protection

# Docs check
python3 scripts/check-docs.py

# Apply settings and attempt protection
scripts/apply-github-settings.sh
```
