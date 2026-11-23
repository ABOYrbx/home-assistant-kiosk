#!/usr/bin/env bash
#
# Launch a Herdr agent for one GitHub issue.
#
# The script must run inside a Herdr-managed pane (HERDR_ENV=1). It:
#   1. loads the issue,
#   2. creates a git worktree and branch,
#   3. opens a Herdr tab in that worktree,
#   4. starts the requested agent kind with the requested model,
#   5. sends a complete implementation prompt,
#   6. waits for the agent,
#   7. runs the local gates,
#   8. pushes the branch and opens a pull request (unless --no-pr).
#
# It never merges. Review the PR yourself.
#
# Examples:
#   scripts/herdr-issue-agent.sh --issue 11 --model opencode-go/deepseek-v4.1-flash
#   scripts/herdr-issue-agent.sh --issue 167 --kind codex --model gpt-5.6-luna --no-pr
#   scripts/herdr-issue-agent.sh --issue 11 --model opencode-go/deepseek-v4.1-flash --dry-run
#
# Requirements: bash, git, gh (authenticated as ABOYrbx), python3, Herdr.
# The model must be valid for the selected agent kind.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_SLUG="ABOYrbx/home-assistant-kiosk"
GH_ACCOUNT="ABOYrbx"

ISSUE=""
MODEL=""
KIND="opencode"
BASE_REF="origin/main"
WORKTREE=""
BRANCH=""
WAIT_MS=1800000
OPEN_PR=1
REUSE=0
DRY_RUN=0
AGENT_NAME=""
WORKSPACE_ID=""
TAB_LABEL=""
EXTRA_ARGS=()

usage() {
  cat <<'EOF'
Usage: scripts/herdr-issue-agent.sh --issue <number> --model <provider/model> [options]

Required:
  --issue <number>       GitHub issue number (a subtask, not a wave epic)
  --model <model>        Model for the selected agent kind

Options:
  --kind <kind>          Herdr agent kind (default: opencode)
                         Examples: opencode, codex, claude, gemini, cursor, hermes
  --base <ref>           Base ref (default: origin/main)
  --worktree <path>      Worktree path (default: ../home-assistant-kiosk-<issue>)
  --branch <name>        Branch name (default: agent/<issue>-<slug>)
  --name <name>          Herdr agent name (default: issue-<issue>)
  --workspace <id>       Put the new tab in this Herdr workspace
  --tab-label <text>     Tab label (default: issue-<issue>)
  --wait-timeout <ms>    Agent wait timeout (default: 1800000 = 30 min)
  --no-pr                Do not push or open a pull request
  --reuse                Reuse an existing worktree/branch
  --dry-run              Print the plan without changing anything
  --agent-arg <arg>      Extra native argument passed after the agent's model args
                         Repeat for multiple arguments.
  -h, --help             Show this help

The script must run inside a Herdr pane. It does not merge pull requests.
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf 'warning: %s\n' "$*" >&2
}

require_value() {
  [ $# -ge 2 ] && [ -n "$2" ] || die "missing value for $1"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --issue) require_value "$@"; ISSUE="$2"; shift 2 ;;
    --model) require_value "$@"; MODEL="$2"; shift 2 ;;
    --kind) require_value "$@"; KIND="$2"; shift 2 ;;
    --base) require_value "$@"; BASE_REF="$2"; shift 2 ;;
    --worktree) require_value "$@"; WORKTREE="$2"; shift 2 ;;
    --branch) require_value "$@"; BRANCH="$2"; shift 2 ;;
    --name) require_value "$@"; AGENT_NAME="$2"; shift 2 ;;
    --workspace) require_value "$@"; WORKSPACE_ID="$2"; shift 2 ;;
    --tab-label) require_value "$@"; TAB_LABEL="$2"; shift 2 ;;
    --wait-timeout) require_value "$@"; WAIT_MS="$2"; shift 2 ;;
    --agent-arg) require_value "$@"; EXTRA_ARGS+=("$2"); shift 2 ;;
    --no-pr) OPEN_PR=0; shift ;;
    --reuse) REUSE=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (use --help)" ;;
  esac
done

[ -n "$ISSUE" ] || die "--issue is required"
[ -n "$MODEL" ] || die "--model is required"
[[ "$ISSUE" =~ ^[0-9]+$ ]] || die "--issue must be a number"
[[ "$WAIT_MS" =~ ^[0-9]+$ ]] || die "--wait-timeout must be milliseconds"

cd "$REPO"

if [ "$DRY_RUN" -eq 0 ]; then
  [ "${HERDR_ENV:-}" = "1" ] || die "run this inside a Herdr pane (HERDR_ENV=1), not from a plain terminal"
  herdr agent list >/dev/null 2>&1 || die "Herdr server is not reachable; start Herdr first and run this inside it"
fi

gh auth status >/dev/null 2>&1 || die "gh is not authenticated"
account="$(gh api user --jq .login)"
[ "$account" = "$GH_ACCOUNT" ] || die "gh is authenticated as '$account', expected '$GH_ACCOUNT'"

issue_json="$(gh issue view "$ISSUE" --repo "$REPO_SLUG" --json number,title,body,url,state,labels)"
issue_title="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["title"])')"
issue_body="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("body") or "")')"
issue_state="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])')"
issue_labels="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(",".join(x["name"] for x in json.load(sys.stdin).get("labels", [])))')"

[ "$issue_state" = "OPEN" ] || die "issue #$ISSUE is $issue_state; only open issues can be dispatched"
case ",$issue_labels," in
  *,epic,*) die "issue #$ISSUE is a wave epic; pick a subtask issue instead" ;;
esac

parent_epic="$(printf '%s' "$issue_body" | python3 -c '
import re, sys
m = re.search(r"Parent epic:\*\* #([0-9]+)", sys.stdin.read())
print(m.group(1) if m else "")
')"
if [ -z "$parent_epic" ]; then
  wave="$(printf '%s' "$issue_labels" | tr ',' '\n' | sed -n 's/^wave-\([0-9]\+\)$/\1/p' | head -n 1)"
  case "$wave" in
    0) parent_epic=5 ;;
    1) parent_epic=3 ;;
    2) parent_epic=2 ;;
    3) parent_epic=6 ;;
    4) parent_epic=1 ;;
    5) parent_epic=4 ;;
    6) parent_epic=7 ;;
  esac
fi

slug="$(printf '%s' "$issue_title" \
  | tr '[:upper:]' '[:lower:]' \
  | sed -E 's/^\[[^]]*\] *//; s/[^a-z0-9]+/-/g; s/^-+//; s/-+$//' \
  | cut -c1-48)"
[ -n "$slug" ] || slug="issue-$ISSUE"

if [ -z "$WORKTREE" ]; then
  WORKTREE="$(dirname "$REPO")/$(basename "$REPO")-$ISSUE"
fi
if [ -z "$BRANCH" ]; then
  BRANCH="agent/$ISSUE-$slug"
fi
if [ -z "$AGENT_NAME" ]; then
  AGENT_NAME="issue-$ISSUE"
fi
if [ -z "$TAB_LABEL" ]; then
  TAB_LABEL="issue-$ISSUE"
fi
if [ "$OPEN_PR" -eq 1 ]; then
  PR_MODE="push branch and open PR"
else
  PR_MODE="leave branch local"
fi

printf 'Issue:     #%s %s\n' "$ISSUE" "$issue_title"
printf 'Epic:      %s\n' "${parent_epic:-unknown}"
printf 'Worktree:  %s\n' "$WORKTREE"
printf 'Branch:    %s\n' "$BRANCH"
printf 'Workspace: %s\n' "${WORKSPACE_ID:-<current>}"
printf 'Tab:       %s\n' "$TAB_LABEL"
printf 'Agent:     %s (%s)\n' "$AGENT_NAME" "$KIND"
printf 'Model:     %s\n' "$MODEL"
printf 'Afterwork: %s\n' "$PR_MODE"

if [ "$DRY_RUN" -eq 1 ]; then
  cat <<EOF

Dry run: no files, worktrees, tabs, agents, commits, or PRs were created.
Planned commands:
  git fetch origin main
  git worktree add -b "$BRANCH" "$WORKTREE" "$BASE_REF"
  herdr tab create --cwd "$WORKTREE" --label "$TAB_LABEL" --no-focus ${WORKSPACE_ID:+--workspace "$WORKSPACE_ID"}
  herdr agent start "$AGENT_NAME" --kind "$KIND" --pane <tab-root-pane> -- <model args>
  herdr agent prompt "$AGENT_NAME" "<implementation prompt>" --wait --timeout "$WAIT_MS"
  python3 scripts/check-docs.py   # inside the worktree
  git push -u origin "$BRANCH" && gh pr create ...  # unless --no-pr
EOF
  exit 0
fi

# --- worktree -------------------------------------------------------------

if ! git show-ref --verify --quiet "refs/remotes/origin/main"; then
  BASE_REF="main"
fi
git fetch origin main >/dev/null 2>&1 || warn "could not fetch origin main; using $BASE_REF"

if [ -e "$WORKTREE" ]; then
  [ "$REUSE" -eq 1 ] || die "worktree already exists: $WORKTREE (use --reuse or --worktree)"
  [ -d "$WORKTREE/.git" ] || [ -f "$WORKTREE/.git" ] || die "not a git worktree: $WORKTREE"
  existing_branch="$(git -C "$WORKTREE" rev-parse --abbrev-ref HEAD)"
  if [ "$existing_branch" != "$BRANCH" ]; then
    die "worktree $WORKTREE is on '$existing_branch', expected '$BRANCH'"
  fi
else
  if git show-ref --verify --quiet "refs/heads/$BRANCH"; then
    [ "$REUSE" -eq 1 ] || die "branch already exists: $BRANCH (use --reuse)"
    git worktree add "$WORKTREE" "$BRANCH" >/dev/null
  else
    git worktree add -b "$BRANCH" "$WORKTREE" "$BASE_REF" >/dev/null
  fi
fi

# --- agent model configuration --------------------------------------------

OPENCODE_CONFIG_FILE=""
cleanup() {
  if [ -n "$OPENCODE_CONFIG_FILE" ] && [ -f "$OPENCODE_CONFIG_FILE" ]; then
    rm -f "$OPENCODE_CONFIG_FILE"
  fi
}
trap cleanup EXIT

MODEL_ARGS=()
if [ "$KIND" = "opencode" ]; then
  OPENCODE_CONFIG_FILE="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-opencode-XXXXXX")"
  python3 - "$MODEL" "$OPENCODE_CONFIG_FILE" <<'PY'
import json, os, sys

model, out = sys.argv[1], sys.argv[2]
global_path = os.path.expanduser("~/.config/opencode/opencode.json")
data = {}
try:
    with open(global_path, encoding="utf-8") as handle:
        loaded = json.load(handle)
    if isinstance(loaded, dict):
        data = loaded
except (OSError, ValueError):
    data = {}
data["$schema"] = data.get("$schema", "https://opencode.ai/config.json")
data["model"] = model
with open(out, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2)
    handle.write("\n")
PY
else
  MODEL_ARGS=(--model "$MODEL")
fi

# --- Herdr tab + agent ----------------------------------------------------

tab_args=(herdr tab create --cwd "$WORKTREE" --label "$TAB_LABEL" --no-focus)
if [ -n "$WORKSPACE_ID" ]; then
  tab_args+=(--workspace "$WORKSPACE_ID")
fi
if [ -n "$OPENCODE_CONFIG_FILE" ]; then
  tab_args+=(--env "OPENCODE_CONFIG=$OPENCODE_CONFIG_FILE")
fi
tab_json="$("${tab_args[@]}")"

pane_id="$(printf '%s' "$tab_json" | python3 -c '
import json, sys
data = json.load(sys.stdin)
root = data["result"]["root_pane"]
print(root["pane_id"] if isinstance(root, dict) else root)
')"
[ -n "$pane_id" ] || die "Herdr did not return a root pane: $tab_json"

# Pick a free agent name; Herdr names are unique among live agents.
base_agent="$AGENT_NAME"
suffix=2
while herdr agent get "$AGENT_NAME" >/dev/null 2>&1; do
  AGENT_NAME="${base_agent}-${suffix}"
  suffix=$((suffix + 1))
done

start_args=(herdr agent start "$AGENT_NAME" --kind "$KIND" --pane "$pane_id" --timeout 120000)
if [ "${#MODEL_ARGS[@]}" -gt 0 ] || [ "${#EXTRA_ARGS[@]}" -gt 0 ]; then
  start_args+=(--)
  [ "${#MODEL_ARGS[@]}" -gt 0 ] && start_args+=("${MODEL_ARGS[@]}")
  [ "${#EXTRA_ARGS[@]}" -gt 0 ] && start_args+=("${EXTRA_ARGS[@]}")
fi

printf '\nStarting Herdr agent %s in pane %s\n' "$AGENT_NAME" "$pane_id"
if ! "${start_args[@]}"; then
  die "Herdr could not start the agent; inspect pane $pane_id"
fi

gh issue edit "$ISSUE" --repo "$REPO_SLUG" --add-assignee "$GH_ACCOUNT" >/dev/null

PROMPT_TEMPLATE="$REPO/docs/agent-prompts/implement-issue.md"
[ -f "$PROMPT_TEMPLATE" ] || die "missing prompt template: $PROMPT_TEMPLATE"

PROMPT="$(python3 - "$PROMPT_TEMPLATE" "$REPO" "$WORKTREE" "$BRANCH" "$ISSUE" "$issue_title" "${parent_epic:-unknown}" "$KIND" "$MODEL" "$issue_body" <<'PY'
import sys

(path, repo, worktree, branch, issue, title, epic, kind, model, body) = sys.argv[1:]
text = open(path, encoding="utf-8").read()
for key, value in (
    ("REPO", repo),
    ("WORKTREE", worktree),
    ("BRANCH", branch),
    ("ISSUE", issue),
    ("TITLE", title),
    ("EPIC", epic),
    ("KIND", kind),
    ("MODEL", model),
):
    text = text.replace("{{" + key + "}}", value)
text = text.replace("{{ISSUE_BODY}}", body)
print(text, end="")
PY
)"

printf '\nSending implementation prompt (%s ms timeout)\n' "$WAIT_MS"
set +e
herdr agent prompt "$AGENT_NAME" "$PROMPT" --wait --timeout "$WAIT_MS"
prompt_status=$?
set -e

printf '\nAgent state:\n'
herdr agent get "$AGENT_NAME" 2>&1 || true

if [ "$prompt_status" -ne 0 ]; then
  warn "agent prompt did not settle successfully (exit $prompt_status)"
  herdr agent read "$AGENT_NAME" --source recent-unwrapped --lines 160 2>&1 || true
  printf '\nInspect or answer it with:\n  herdr agent read %s --source recent-unwrapped --lines 160\n  herdr agent attach %s\n  herdr agent prompt %s "<answer>" --wait\n' "$AGENT_NAME" "$AGENT_NAME" "$AGENT_NAME"
  exit 2
fi

# --- commit and gates ------------------------------------------------------

if ! git -C "$WORKTREE" rev-parse --verify HEAD >/dev/null 2>&1; then
  die "worktree has no HEAD"
fi

commits="$(git -C "$WORKTREE" log --oneline "$BASE_REF..HEAD" 2>/dev/null || true)"
if [ -z "$commits" ]; then
  warn "the agent did not create a commit"
  herdr agent read "$AGENT_NAME" --source recent-unwrapped --lines 160 2>&1 || true
  exit 3
fi

printf '\nCommits on %s:\n%s\n' "$BRANCH" "$commits"

if [ -n "$(git -C "$WORKTREE" status --porcelain)" ]; then
  warn "worktree has uncommitted changes; refusing to open a PR"
  git -C "$WORKTREE" status --short
  exit 3
fi

GATE_LOG="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-gate-XXXXXX")"
{
  if [ -f "$WORKTREE/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    echo "== cargo fmt --check =="
    (cd "$WORKTREE" && cargo fmt --check)
    echo "== cargo clippy =="
    (cd "$WORKTREE" && cargo clippy --all-targets -- -D warnings)
    echo "== cargo test =="
    (cd "$WORKTREE" && cargo test)
  else
    echo "== Rust gate skipped (no Cargo.toml or no cargo) =="
  fi
  echo "== docs check =="
  (cd "$WORKTREE" && python3 scripts/check-docs.py)
} 2>&1 | tee "$GATE_LOG"
gate_status=${PIPESTATUS[0]}

if [ "$gate_status" -ne 0 ]; then
  warn "local gates failed; branch is kept at $WORKTREE"
  printf 'Gate log: %s\n' "$GATE_LOG"
  printf 'Fix and retry, or inspect the agent with:\n  herdr agent read %s --source recent-unwrapped --lines 160\n' "$AGENT_NAME"
  exit 4
fi

# --- PR -------------------------------------------------------------------

pr_url=""
if [ "$OPEN_PR" -eq 1 ]; then
  printf '\nPushing %s and opening a pull request\n' "$BRANCH"
  git -C "$WORKTREE" push -u origin "$BRANCH"

  existing_pr="$(gh pr list --repo "$REPO_SLUG" --head "$BRANCH" --json url --jq '.[0].url // empty' 2>/dev/null || true)"
  if [ -n "$existing_pr" ]; then
    pr_url="$existing_pr"
    printf 'Pull request already exists: %s\n' "$pr_url"
  else
    report="$(tail -c 6000 "$GATE_LOG")"
    pr_body="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-pr-XXXXXX")"
    cat > "$pr_body" <<EOF
Closes #$ISSUE

Parent epic: #${parent_epic:-unknown}

## Summary

Automated implementation in Herdr with \`$KIND\` and model \`$MODEL\`.

Branch: \`$BRANCH\`
Worktree: \`$WORKTREE\`

## Gate evidence

\`\`\`text
$report
\`\`\`

## Review checklist

- [ ] I reviewed the diff and the agent's handoff comment.
- [ ] The gate commands above are sufficient for this change.
- [ ] No secrets, personal data, provider-specific URLs, or hardcoded entity IDs were added.
- [ ] Documentation and \`config.example.toml\` match the behavior.
EOF
    pr_title="Implement #$ISSUE: $issue_title"
    pr_url="$(gh pr create --repo "$REPO_SLUG" --base main --head "$BRANCH" --title "$pr_title" --body-file "$pr_body")"
    rm -f "$pr_body"
    printf 'Pull request: %s\n' "$pr_url"
  fi
fi

printf '\nDone.\n'
printf 'Issue:     #%s\n' "$ISSUE"
printf 'Branch:    %s\n' "$BRANCH"
printf 'Worktree:  %s\n' "$WORKTREE"
printf 'Herdr:     agent %s, pane %s\n' "$AGENT_NAME" "$pane_id"
printf 'Gate log:  %s\n' "$GATE_LOG"
[ -n "$pr_url" ] && printf 'PR:        %s\n' "$pr_url"
printf '\nNext: review the branch/PR. The script never merges.\n'
