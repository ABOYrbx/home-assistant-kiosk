#!/usr/bin/env bash
#
# Apply the GitHub repository settings described in docs/github-rules.md.
#
# Branch protection and repository rulesets require GitHub Pro (or a public
# repository). On GitHub Free + private, the script applies every available
# setting and prints a clear message for the rest.

set -euo pipefail

REPO="${REPO:-ABOYrbx/home-assistant-kiosk}"
EXPECTED_ACCOUNT="${EXPECTED_ACCOUNT:-ABOYrbx}"
DEFAULT_BRANCH="${DEFAULT_BRANCH:-main}"

warn() {
  printf 'warning: %s\n' "$*" >&2
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

command -v gh >/dev/null 2>&1 || die "gh CLI is not installed"

account="$(gh api user --jq .login 2>/dev/null || true)"
[ "$account" = "$EXPECTED_ACCOUNT" ] || die "gh is authenticated as '${account:-unknown}', expected '$EXPECTED_ACCOUNT'"

echo "Applying settings to $REPO as $account"

# ---------------------------------------------------------------------------
# Repository settings
# ---------------------------------------------------------------------------

gh api -X PATCH "repos/$REPO" --input - >/dev/null <<'JSON'
{
  "has_issues": true,
  "has_projects": false,
  "has_wiki": false,
  "has_discussions": false,
  "allow_squash_merge": true,
  "allow_merge_commit": false,
  "allow_rebase_merge": false,
  "squash_merge_commit_title": "PR_TITLE",
  "squash_merge_commit_message": "PR_BODY",
  "delete_branch_on_merge": true,
  "allow_auto_merge": true,
  "allow_update_branch": true,
  "web_commit_signoff_required": false
}
JSON

# ---------------------------------------------------------------------------
# Security and Actions
# ---------------------------------------------------------------------------

gh api -X PUT "repos/$REPO/vulnerability-alerts" >/dev/null 2>&1 \
  || warn "could not enable Dependabot alerts"

gh api -X PUT "repos/$REPO/automated-security-fixes" >/dev/null 2>&1 \
  || warn "could not enable Dependabot security updates"

gh api -X PUT "repos/$REPO/private-vulnerability-reporting" >/dev/null 2>&1 \
  || warn "could not enable private vulnerability reporting"

gh api -X PATCH "repos/$REPO" --input - >/dev/null 2>&1 <<'JSON' \
  || warn "secret scanning / push protection not available on this plan"
{
  "security_and_analysis": {
    "secret_scanning": { "status": "enabled" },
    "secret_scanning_push_protection": { "status": "enabled" }
  }
}
JSON

gh api -X PUT "repos/$REPO/actions/permissions/workflow" --input - >/dev/null <<'JSON'
{
  "default_workflow_permissions": "read",
  "can_approve_pull_request_reviews": false
}
JSON

if gh api -X PUT "repos/$REPO/actions/permissions" --input - >/dev/null <<'JSON'
{
  "enabled": true,
  "allowed_actions": "selected",
  "sha_pinning_required": false
}
JSON
then
  gh api -X PUT "repos/$REPO/actions/permissions/selected-actions" --input - >/dev/null <<'JSON' \
    || warn "could not restrict Actions to GitHub-owned and verified actions"
{
  "github_owned_allowed": true,
  "verified_allowed": true,
  "patterns_allowed": []
}
JSON
else
  warn "could not set selected Actions policy; leaving the current policy in place"
fi

# ---------------------------------------------------------------------------
# Branch protection: prefer rulesets, fall back to classic protection
# ---------------------------------------------------------------------------

protection_applied=false

if gh api "repos/$REPO/rulesets" >/dev/null 2>&1; then
  ruleset_id="$(gh api "repos/$REPO/rulesets" --jq '.[] | select(.name == "main-protection") | .id' 2>/dev/null | head -n 1 || true)"

  if [ -n "$ruleset_id" ]; then
    if gh api -X PUT "repos/$REPO/rulesets/$ruleset_id" --input - >/dev/null <<'JSON'
{
  "name": "main-protection",
  "target": "branch",
  "enforcement": "active",
  "conditions": {
    "ref_name": {
      "include": ["~DEFAULT_BRANCH"],
      "exclude": []
    }
  },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "required_linear_history" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": true,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": true,
        "required_status_checks": [
          { "context": "docs" }
        ]
      }
    }
  ],
  "bypass_actors": []
}
JSON
    then
      protection_applied=true
    fi
  else
    if gh api -X POST "repos/$REPO/rulesets" --input - >/dev/null <<'JSON'
{
  "name": "main-protection",
  "target": "branch",
  "enforcement": "active",
  "conditions": {
    "ref_name": {
      "include": ["~DEFAULT_BRANCH"],
      "exclude": []
    }
  },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "required_linear_history" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": true,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": true,
        "required_status_checks": [
          { "context": "docs" }
        ]
      }
    }
  ],
  "bypass_actors": []
}
JSON
    then
      protection_applied=true
    fi
  fi
fi

if [ "$protection_applied" != true ]; then
  if gh api -X PUT "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --input - >/dev/null <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["docs"]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 0,
    "require_last_push_approval": false
  },
  "restrictions": null,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "required_conversation_resolution": true,
  "lock_branch": false,
  "allow_fork_syncing": false
}
JSON
  then
    protection_applied=true
  fi
fi

if [ "$protection_applied" = true ]; then
  echo "Branch protection applied to $DEFAULT_BRANCH"
else
  warn "branch protection/rulesets are unavailable on GitHub Free for a private repository"
  warn "make the repository public or upgrade to GitHub Pro, then rerun this script"
fi

# ---------------------------------------------------------------------------
# Verify what is active
# ---------------------------------------------------------------------------

echo
echo "Current repository settings:"
settings="$(gh api "repos/$REPO" --jq '{
  default_branch,
  has_issues,
  has_projects,
  has_wiki,
  has_discussions,
  allow_squash_merge,
  allow_merge_commit,
  allow_rebase_merge,
  delete_branch_on_merge,
  allow_auto_merge,
  allow_update_branch
}')"
echo "$settings"

if ! printf '%s' "$settings" | grep -q '"allow_auto_merge": true'; then
  warn "auto-merge is not available on this plan; merge pull requests manually"
fi

echo
echo "Current collaborators:"
gh api "repos/$REPO/collaborators" --jq '.[] | .login'

echo
echo "Repository rules: see docs/github-rules.md"
