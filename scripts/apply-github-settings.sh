#!/usr/bin/env bash
#
# Apply the GitHub repository settings described in docs/github-rules.md.
#
# The branch rules live in scripts/github-protection.json (repository ruleset)
# and scripts/github-protection-classic.json (classic fallback). A ruleset is
# preferred because it applies to administrators unless a bypass actor is
# listed. The classic fallback sets enforce_admins.
#
# GitHub Free does not expose either feature for private repositories. In that
# case the script applies every available setting and prints a clear warning.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

REPO="${REPO:-ABOYrbx/home-assistant-kiosk}"
EXPECTED_ACCOUNT="${EXPECTED_ACCOUNT:-ABOYrbx}"
DEFAULT_BRANCH="${DEFAULT_BRANCH:-main}"
RULESET_FILE="${RULESET_FILE:-$SCRIPT_DIR/github-protection.json}"
CLASSIC_FILE="${CLASSIC_FILE:-$SCRIPT_DIR/github-protection-classic.json}"

warn() {
  printf 'warning: %s\n' "$*" >&2
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

command -v gh >/dev/null 2>&1 || die "gh CLI is not installed"
[ -f "$RULESET_FILE" ] || die "missing ruleset payload: $RULESET_FILE"
[ -f "$CLASSIC_FILE" ] || die "missing classic protection payload: $CLASSIC_FILE"

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

if ! gh api -X PUT "repos/$REPO/vulnerability-alerts" >/dev/null 2>&1; then
  warn "could not enable Dependabot alerts"
fi

if ! gh api -X PUT "repos/$REPO/automated-security-fixes" >/dev/null 2>&1; then
  warn "could not enable Dependabot security updates"
fi

if gh api -X PUT "repos/$REPO/private-vulnerability-reporting" >/dev/null 2>&1; then
  pvr_status="$(gh api "repos/$REPO/private-vulnerability-reporting" --jq '.enabled' 2>/dev/null || true)"
  if [ "$pvr_status" != true ]; then
    warn "private vulnerability reporting is not enabled (status: ${pvr_status:-unknown})"
  fi
else
  warn "could not enable private vulnerability reporting"
fi

if gh api -X PATCH "repos/$REPO" --input - >/dev/null 2>&1 <<'JSON'
{
  "security_and_analysis": {
    "secret_scanning": { "status": "enabled" },
    "secret_scanning_push_protection": { "status": "enabled" }
  }
}
JSON
then
  scanning_status="$(gh api "repos/$REPO" --jq '[
    (.security_and_analysis.secret_scanning.status // "missing"),
    (.security_and_analysis.secret_scanning_push_protection.status // "missing")
  ] | join("|")' 2>/dev/null || true)"
  if [ "$scanning_status" != "enabled|enabled" ]; then
    warn "secret scanning status is '${scanning_status:-unknown}', expected 'enabled|enabled'"
  fi
else
  warn "secret scanning / push protection not available on this plan"
fi

gh api -X PUT "repos/$REPO/actions/permissions/workflow" --input - >/dev/null <<'JSON'
{
  "default_workflow_permissions": "read",
  "can_approve_pull_request_reviews": false
}
JSON

workflow_policy="$(gh api "repos/$REPO/actions/permissions/workflow" --jq '[
  .default_workflow_permissions,
  (.can_approve_pull_request_reviews | tostring)
] | join("|")' 2>/dev/null || true)"
if [ "$workflow_policy" != "read|false" ]; then
  warn "workflow token policy is '${workflow_policy:-unknown}', expected 'read|false'"
fi

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
# Branch protection: ruleset first, classic fallback
# ---------------------------------------------------------------------------

verify_ruleset() {
  local id="$1" value

  if ! value="$(gh api "repos/$REPO/rulesets/$id" --jq '.enforcement' 2>/dev/null)"; then
    warn "could not read the main-protection ruleset"
    return 1
  fi
  if [ "$value" != "active" ]; then
    warn "ruleset enforcement is '$value', expected 'active'"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.target' 2>/dev/null)" || return 1
  if [ "$value" != "branch" ]; then
    warn "ruleset target is '$value', expected 'branch'"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules | map(.type) | sort | join(",")' 2>/dev/null)" || return 1
  if [ "$value" != "deletion,non_fast_forward,pull_request,required_linear_history,required_status_checks" ]; then
    warn "ruleset rules are '$value'; expected deletion, non_fast_forward, pull_request, required_linear_history, required_status_checks"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.bypass_actors | length' 2>/dev/null)" || return 1
  if [ "$value" != "0" ]; then
    warn "ruleset has $value bypass actors; administrators must not bypass the rule"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.current_user_can_bypass' 2>/dev/null)" || return 1
  if [ "$value" != "never" ]; then
    warn "the authenticated owner can bypass the ruleset ('$value'); expected 'never'"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.conditions.ref_name.include | sort | join(",")' 2>/dev/null)" || return 1
  case "$value" in
    "~DEFAULT_BRANCH" | "refs/heads/$DEFAULT_BRANCH") ;;
    *)
      warn "ruleset ref condition is '$value'; expected the default branch"
      return 1
      ;;
  esac

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.conditions.ref_name.exclude | length' 2>/dev/null)" || return 1
  if [ "$value" != "0" ]; then
    warn "ruleset excludes $value refs; the rule must cover every ref it targets"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "pull_request") | .parameters.required_approving_review_count' 2>/dev/null)" || return 1
  if [ "$value" != "0" ]; then
    warn "required approving review count is '$value', expected 0 for a sole owner"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "pull_request") | .parameters.dismiss_stale_reviews_on_push' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "stale reviews are not dismissed on push"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "pull_request") | .parameters.require_code_owner_review' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "code owner review is required; expected false for a sole owner"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "pull_request") | .parameters.require_last_push_approval' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "last push approval is required; expected false for a sole owner"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "pull_request") | .parameters.required_review_thread_resolution' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "review thread resolution is not required"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "pull_request") | .parameters.allowed_merge_methods | sort | join(",")' 2>/dev/null)" || return 1
  if [ "$value" != "squash" ]; then
    warn "allowed merge methods are '$value', expected 'squash'"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "pull_request") | .parameters.require_extra_approval_for_unattributed_changes' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "unattributed changes would require an extra approval; expected false for a sole owner"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "required_status_checks") | .parameters.strict_required_status_checks_policy' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "required status checks are not strict (the branch may be out of date)"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "required_status_checks") | .parameters.do_not_enforce_on_create' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "status checks are skipped when a branch is created; expected false"
    return 1
  fi

  value="$(gh api "repos/$REPO/rulesets/$id" --jq '.rules[] | select(.type == "required_status_checks") | .parameters.required_status_checks | map(.context) | sort | join(",")' 2>/dev/null)" || return 1
  if [ "$value" != "docs" ]; then
    warn "required status checks are '$value', expected 'docs'"
    return 1
  fi

  return 0
}

verify_classic() {
  local value

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.enforce_admins.enabled' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "classic protection does not enforce the rules for administrators"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_status_checks.strict' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "classic required status checks are not strict"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_status_checks.contexts | sort | join(",")' 2>/dev/null)" || return 1
  if [ "$value" != "docs" ]; then
    warn "classic required status checks are '$value', expected 'docs'"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_pull_request_reviews.required_approving_review_count' 2>/dev/null)" || return 1
  if [ "$value" != "0" ]; then
    warn "classic required approving review count is '$value', expected 0"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_pull_request_reviews.dismiss_stale_reviews' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "classic protection does not dismiss stale reviews on push"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_pull_request_reviews.require_code_owner_reviews' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "classic protection requires code owner review; expected false for a sole owner"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_pull_request_reviews.require_last_push_approval' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "classic protection requires last push approval; expected false for a sole owner"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_linear_history.enabled' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "classic protection does not require a linear history"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.allow_force_pushes.enabled' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "classic protection allows force-pushes"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.allow_deletions.enabled' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "classic protection allows branch deletion"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.required_conversation_resolution.enabled' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "classic protection does not require conversation resolution"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.restrictions == null' 2>/dev/null)" || return 1
  if [ "$value" != "true" ]; then
    warn "classic protection restricts pushes; expected no push restriction because ABOYrbx is the only collaborator"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.lock_branch' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "classic protection locks the branch; expected false"
    return 1
  fi

  value="$(gh api "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --jq '.allow_fork_syncing' 2>/dev/null)" || return 1
  if [ "$value" != "false" ]; then
    warn "classic protection allows fork syncing; expected false"
    return 1
  fi

  return 0
}

protection_kind=""
rulesets_error=""
if rulesets_json="$(gh api "repos/$REPO/rulesets" 2>&1)"; then
  rulesets_available=true
else
  rulesets_available=false
  rulesets_error="$rulesets_json"
fi

if [ "$rulesets_available" = true ]; then
  ruleset_id="$(gh api "repos/$REPO/rulesets" --jq '.[] | select(.name == "main-protection") | .id' 2>/dev/null | head -n 1 || true)"

  if [ -n "$ruleset_id" ]; then
    if ! apply_output="$(gh api -X PUT "repos/$REPO/rulesets/$ruleset_id" --input "$RULESET_FILE" 2>&1)"; then
      die "could not update the main-protection ruleset: $apply_output"
    fi
  else
    if ! apply_output="$(gh api -X POST "repos/$REPO/rulesets" --input "$RULESET_FILE" 2>&1)"; then
      die "could not create the main-protection ruleset: $apply_output"
    fi
    ruleset_id="$(gh api "repos/$REPO/rulesets" --jq '.[] | select(.name == "main-protection") | .id' 2>/dev/null | head -n 1 || true)"
    [ -n "$ruleset_id" ] || die "the main-protection ruleset was created but its id could not be resolved"
  fi

  if ! verify_ruleset "$ruleset_id"; then
    die "the main-protection ruleset is not in the required state"
  fi
  protection_kind="ruleset"
else
  plan_limited=false
  case "$rulesets_error" in
    *"Upgrade to GitHub Pro or make this repository public"*) plan_limited=true ;;
    *) warn "could not list repository rulesets: $rulesets_error" ;;
  esac

  if classic_output="$(gh api -X PUT "repos/$REPO/branches/$DEFAULT_BRANCH/protection" --input "$CLASSIC_FILE" 2>&1)"; then
    if ! verify_classic; then
      die "classic branch protection is not in the required state"
    fi
    protection_kind="classic"
  else
    case "$classic_output" in
      *"Upgrade to GitHub Pro or make this repository public"*) plan_limited=true ;;
    esac
    if [ "$plan_limited" = true ]; then
      warn "branch protection/rulesets are unavailable on GitHub Free for a private repository"
      warn "make the repository public or upgrade to GitHub Pro, then rerun this script"
    else
      die "could not apply classic branch protection: $classic_output"
    fi
  fi
fi

if [ -n "$protection_kind" ]; then
  echo "Branch protection applied to $DEFAULT_BRANCH ($protection_kind)"
else
  warn "branch protection was not applied; the policy is process-only until the plan supports it"
fi

# ---------------------------------------------------------------------------
# Verify what is active
# ---------------------------------------------------------------------------

echo
echo "Current repository settings:"
settings="$(gh api "repos/$REPO" --jq '{
  default_branch,
  private,
  visibility,
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

if ! printf '%s' "$settings" | grep -Eq '"allow_auto_merge":[[:space:]]*true'; then
  warn "auto-merge is not available on this plan; merge pull requests manually"
fi

echo
echo "Current collaborators:"
collaborators="$(gh api "repos/$REPO/collaborators" --jq '.[].login' 2>/dev/null || true)"
printf '%s\n' "$collaborators"
if [ "$collaborators" != "$EXPECTED_ACCOUNT" ]; then
  warn "unexpected collaborator set; expected only '$EXPECTED_ACCOUNT'"
fi

echo
echo "Repository rules: see docs/github-rules.md"
