#!/usr/bin/env bash
#
# herdr.sh — interactive Herdr agent dispatcher for the Home Assistant Kiosk.
#
# One script: pick 1–4 open subtask issues, choose kind/model, and launch one
# agent per issue in a new Herdr workspace. Each agent gets its own worktree,
# branch, tab titled with the issue, and its own pull request.
#
# The script NEVER merges.
#
# Examples:
#   ./herdr.sh
#   ./herdr.sh --issues 11,12,13,14 --kind opencode --model opencode-go/deepseek-v4.1-flash
#   ./herdr.sh --list
#   ./herdr.sh --issues 11 --model opencode-go/deepseek-v4.1-flash --dry-run
#
# Must run inside a Herdr pane (HERDR_ENV=1), except --list and --dry-run.

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_SLUG="ABOYrbx/home-assistant-kiosk"
GH_ACCOUNT="ABOYrbx"
PROMPT_TEMPLATE="$ROOT/docs/agent-prompts/implement-issue.md"

MAX_AGENTS=4
WAIT_MS=1800000
LIST_ONLY=0
INCLUDE_COMMITS=0
DRY_RUN=0
FOCUS=0
ISSUES_ARG=""
KIND=""
MODEL=""
OPEN_PR=""

ISSUE_FILE=""
WORKSPACE_ID=""
LOG_DIR=""

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
warn() { printf 'warning: %s\n' "$*" >&2; }

usage() {
  cat <<'EOF'
Usage: ./herdr.sh [options]

Interactive (default):
  Pick 1–4 open subtask issues, choose kind/model, launch one agent per issue
  in a new Herdr workspace, and open one PR per issue after the gates.

Options:
  --issues <n,n,...>       Preselect issues (comma or space separated)
  --issue <n>              Alias for one issue
  --max <1-4>              Maximum agents (default: 4)
  --kind <kind>            Agent kind (default: opencode)
  --model <provider/model> Model for all agents
  --no-pr                  Do not push or open pull requests
  --focus                  Focus the new workspace
  --all                    Include commit-step issues
  --wait-timeout <ms>      Per-agent timeout (default: 1800000)
  --list                   List selectable issues and exit
  --dry-run                Print the plan without creating anything
  -h, --help               Show this help

The implementation prompt is docs/agent-prompts/implement-issue.md.
No agent is allowed to push, merge, or open a PR; this script does that.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --issues) [ -n "${2:-}" ] || die "missing value for --issues"; ISSUES_ARG="$2"; shift 2 ;;
    --issue) [ -n "${2:-}" ] || die "missing value for --issue"; ISSUES_ARG="$2"; shift 2 ;;
    --max) [ -n "${2:-}" ] || die "missing value for --max"; MAX_AGENTS="$2"; shift 2 ;;
    --kind) [ -n "${2:-}" ] || die "missing value for --kind"; KIND="$2"; shift 2 ;;
    --model) [ -n "${2:-}" ] || die "missing value for --model"; MODEL="$2"; shift 2 ;;
    --wait-timeout) [ -n "${2:-}" ] || die "missing value for --wait-timeout"; WAIT_MS="$2"; shift 2 ;;
    --no-pr) OPEN_PR=0; shift ;;
    --focus) FOCUS=1; shift ;;
    --all) INCLUDE_COMMITS=1; shift ;;
    --list) LIST_ONLY=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (use --help)" ;;
  esac
done

[[ "$MAX_AGENTS" =~ ^[1-4]$ ]] || die "--max must be 1-4"
[[ "$WAIT_MS" =~ ^[0-9]+$ ]] || die "--wait-timeout must be milliseconds"

cd "$ROOT"

if [ ! -f "$PROMPT_TEMPLATE" ]; then
  die "missing prompt template: $PROMPT_TEMPLATE"
fi

if [ "$LIST_ONLY" -eq 0 ] && [ "$DRY_RUN" -eq 0 ] && [ "${HERDR_ENV:-}" != "1" ]; then
  if herdr status 2>/dev/null | grep -q 'status: running'; then
    die "this terminal is not a managed Herdr pane. Open a terminal inside the Herdr space (This computer) and run ./herdr.sh there; HERDR_ENV must be 1."
  else
    die "Herdr server is not running. Start or attach Herdr, open a space under 'This computer', and run ./herdr.sh from a managed pane; HERDR_ENV must be 1."
  fi
fi

if [ "$LIST_ONLY" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  herdr agent list >/dev/null 2>&1 || die "Herdr server is not reachable. Start/attach Herdr and open a managed space first."
fi

gh auth status >/dev/null 2>&1 || die "gh is not authenticated"
account="$(gh api user --jq .login)"
[ "$account" = "$GH_ACCOUNT" ] || die "gh is authenticated as '$account', expected '$GH_ACCOUNT'"

# ---------------------------------------------------------------------------
# Issue discovery and selection
# ---------------------------------------------------------------------------

ISSUE_FILE="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-issues-XXXXXX")"
ISSUE_HELPER="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-issue-helper-XXXXXX")"
cleanup() {
  rm -f "$ISSUE_FILE" "$ISSUE_HELPER"
}
trap cleanup EXIT

cat > "$ISSUE_HELPER" <<'PY'
import json
import re
import sys

mode = sys.argv[1] if len(sys.argv) > 1 else "subtask"
data = json.load(sys.stdin)
rows = []
for item in data:
    labels = {label["name"] for label in item.get("labels", [])}
    if "epic" in labels:
        continue
    if "subtask" not in labels:
        continue
    if mode != "all" and "commit-step" in labels:
        continue
    title = item["title"]
    match = re.search(r"\[W(\d+)\.(\d+)\.(\d+)\]", title)
    key = (int(match.group(1)), int(match.group(2)), int(match.group(3)), item["number"]) if match else (99, 0, 0, item["number"])
    rows.append((key, item["number"], title, ",".join(sorted(labels)), item["url"]))
rows.sort(key=lambda row: row[0])
for _, number, title, labels, url in rows:
    print(f"{number}\t{title}\t{labels}\t{url}")
PY

gh issue list --repo "$REPO_SLUG" --state open --limit 300 \
  --json number,title,labels,url \
  | python3 "$ISSUE_HELPER" "$([ "$INCLUDE_COMMITS" -eq 1 ] && echo all || echo subtask)" > "$ISSUE_FILE"

if [ ! -s "$ISSUE_FILE" ]; then
  die "no open subtask issues found"
fi

if [ "$LIST_ONLY" -eq 1 ]; then
  awk -F '\t' '{printf "#%s  %s\n", $1, $2}' "$ISSUE_FILE" | sed -n '1,40p'
  echo "..."
  echo "$(wc -l < "$ISSUE_FILE" | tr -d ' ') issues available; run ./herdr.sh for the picker."
  exit 0
fi

picked=()
if [ -n "$ISSUES_ARG" ]; then
  normalized="${ISSUES_ARG//,/ }"
  read -r -a picked <<< "$normalized"
elif command -v fzf >/dev/null 2>&1; then
  selection="$(fzf \
    --multi \
    --delimiter='\t' \
    --with-nth=2.. \
    --prompt='Issues (Tab to select) > ' \
    --header="Select 1-$MAX_AGENTS issues" \
    --height=80% \
    --reverse \
    < "$ISSUE_FILE")" || die "no issues selected"
  while IFS= read -r line; do
    [ -n "$line" ] && picked+=("${line%%$'\t'*}")
  done <<< "$selection"
else
  echo "Available issues:"
  awk -F '\t' '{printf "%s  #%s  %s\n", NR, $1, $2}' "$ISSUE_FILE"
  read -r -p "Issue numbers (space separated, 1-$MAX_AGENTS): " -a picked
fi

[ "${#picked[@]}" -gt 0 ] || die "no issues selected"
[ "${#picked[@]}" -le "$MAX_AGENTS" ] || die "selected ${#picked[@]} issues; maximum is $MAX_AGENTS"

sel_issues=()
sel_titles=()
sel_labels=()
sel_urls=()
for issue in "${picked[@]}"; do
  [[ "$issue" =~ ^[0-9]+$ ]] || die "invalid issue number: $issue"
  duplicate=0
  for existing in "${sel_issues[@]:-}"; do
    [ "$existing" = "$issue" ] && duplicate=1
  done
  [ "$duplicate" -eq 1 ] && die "duplicate issue: $issue"
  row="$(awk -F '\t' -v n="$issue" '$1 == n { print; exit }' "$ISSUE_FILE")"
  [ -n "$row" ] || die "issue #$issue is not in the selectable list"
  IFS=$'\t' read -r _ title labels url <<< "$row"
  sel_issues+=("$issue")
  sel_titles+=("$title")
  sel_labels+=("$labels")
  sel_urls+=("$url")
done

# ---------------------------------------------------------------------------
# Kind / model / PR choice
# ---------------------------------------------------------------------------

if [ -z "$KIND" ]; then
  read -r -p "Agent kind [opencode]: " KIND
  KIND="${KIND:-opencode}"
fi

MODELS_FILE=""
if [ -z "$MODEL" ]; then
  if [ "$KIND" = "opencode" ] && command -v opencode >/dev/null 2>&1; then
    MODELS_FILE="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-models-XXXXXX")"
    trap 'cleanup; rm -f "$MODELS_FILE"' EXIT
    if opencode models > "$MODELS_FILE" 2>/dev/null && [ -s "$MODELS_FILE" ]; then
      echo "Available OpenCode models:"
      nl -ba "$MODELS_FILE" | sed -n '1,80p'
      [ "$(wc -l < "$MODELS_FILE" | tr -d ' ')" -gt 80 ] && echo "  ... type the exact provider/model for more"
      echo
    fi
    while [ -z "$MODEL" ]; do
      read -r -p "Model (number or provider/model): " answer
      if [[ "$answer" =~ ^[0-9]+$ ]]; then
        MODEL="$(sed -n "${answer}p" "$MODELS_FILE")"
      else
        MODEL="$answer"
      fi
      [ -n "$MODEL" ] || echo "A model is required."
    done
    if ! grep -Fxq "$MODEL" "$MODELS_FILE"; then
      warn "model '$MODEL' is not in the current opencode model list"
      read -r -p "Continue anyway? [y/N]: " confirm
      case "$confirm" in y|Y) ;; *) die "aborted" ;; esac
    fi
  else
    while [ -z "$MODEL" ]; do
      read -r -p "Model for $KIND: " MODEL
    done
  fi
fi

if [ -z "$OPEN_PR" ]; then
  read -r -p "Open one pull request per issue after gates? [Y/n]: " answer
  case "$answer" in n|N) OPEN_PR=0 ;; *) OPEN_PR=1 ;; esac
fi

# ---------------------------------------------------------------------------
# Worktree / branch / agent names
# ---------------------------------------------------------------------------

base_ref="origin/main"
git show-ref --verify --quiet "refs/remotes/origin/main" || base_ref="main"
git fetch origin main >/dev/null 2>&1 || warn "could not fetch origin main; using $base_ref"

sel_worktrees=()
sel_branches=()
sel_agents=()
for i in "${!sel_issues[@]}"; do
  issue="${sel_issues[$i]}"
  title="${sel_titles[$i]}"
  slug="$(printf '%s' "$title" | tr '[:upper:]' '[:lower:]' | sed -E 's/^\[[^]]*\] *//; s/[^a-z0-9]+/-/g; s/^-+//; s/-+$//' | cut -c1-48)"
  [ -n "$slug" ] || slug="issue-$issue"
  sel_worktrees+=("$(dirname "$ROOT")/$(basename "$ROOT")-$issue")
  sel_branches+=("agent/$issue-$slug")
  if [[ "$title" =~ ^\[W([0-9]+)\.([0-9]+)\.([0-9]+)\] ]]; then
    sel_agents+=("w${BASH_REMATCH[1]}-${BASH_REMATCH[2]}-${BASH_REMATCH[3]}")
  else
    sel_agents+=("issue-$issue")
  fi
done

workspace_label="agents-$(date +%Y%m%d-%H%M)"
printf '\nWorkspace: %s\n' "$workspace_label"
for i in "${!sel_issues[@]}"; do
  printf '  #%s  %s\n' "${sel_issues[$i]}" "${sel_titles[$i]}"
  printf '       worktree %s\n       branch   %s\n       agent    %s\n' "${sel_worktrees[$i]}" "${sel_branches[$i]}" "${sel_agents[$i]}"
done
printf '  kind/model: %s / %s\n' "$KIND" "$MODEL"
printf '  after work: %s\n' "$([ "$OPEN_PR" -eq 1 ] && echo "push + one PR per issue" || echo "leave branches local")"

if [ "$DRY_RUN" -eq 1 ]; then
  echo
  echo "Dry run: nothing was created."
  echo "Planned: create workspace '$workspace_label', prepare worktrees, then:"
  for i in "${!sel_issues[@]}"; do
    echo "  #${sel_issues[$i]} -> agent ${sel_agents[$i]} | tab \"${sel_titles[$i]}\" | branch ${sel_branches[$i]}"
  done
  exit 0
fi

# ---------------------------------------------------------------------------
# Worktrees, workspace, agents
# ---------------------------------------------------------------------------

for i in "${!sel_issues[@]}"; do
  worktree="${sel_worktrees[$i]}"
  branch="${sel_branches[$i]}"
  if [ -e "$worktree" ]; then
    [ -d "$worktree/.git" ] || [ -f "$worktree/.git" ] || die "not a git worktree: $worktree"
    current_branch="$(git -C "$worktree" rev-parse --abbrev-ref HEAD)"
    [ "$current_branch" = "$branch" ] || die "worktree $worktree is on '$current_branch', expected '$branch'"
  elif git show-ref --verify --quiet "refs/heads/$branch"; then
    git worktree add "$worktree" "$branch" >/dev/null
  else
    git worktree add -b "$branch" "$worktree" "$base_ref" >/dev/null
  fi
done

workspace_json="$(herdr workspace create --cwd "$ROOT" --label "$workspace_label" $([ "$FOCUS" -eq 1 ] && echo --focus || echo --no-focus))"
WORKSPACE_ID="$(printf '%s' "$workspace_json" | python3 -c '
import json, sys
data = json.load(sys.stdin)
workspace = data["result"]["workspace"]
if isinstance(workspace, dict):
    print(workspace.get("workspace_id") or workspace.get("id") or "")
else:
    print(workspace)
')"
[ -n "$WORKSPACE_ID" ] || die "Herdr did not return a workspace id: $workspace_json"

root_tab="$(printf '%s' "$workspace_json" | python3 -c '
import json, sys
data = json.load(sys.stdin)
tab = data["result"].get("tab") or {}
if isinstance(tab, dict):
    print(tab.get("tab_id") or tab.get("id") or "")
else:
    print(tab)
')"
[ -n "$root_tab" ] && herdr tab rename "$root_tab" "control" >/dev/null 2>&1 || true

LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ha-kiosk-team-XXXXXX")"
printf '\nWorkspace id: %s\nLogs: %s\n\n' "$WORKSPACE_ID" "$LOG_DIR"

# ---------------------------------------------------------------------------
# Per-issue worker
# ---------------------------------------------------------------------------

render_prompt() {
  local issue="$1" title="$2" body="$3" worktree="$4" branch="$5" epic="$6"
  python3 - "$PROMPT_TEMPLATE" "$ROOT" "$worktree" "$branch" "$issue" "$title" "$epic" "$KIND" "$MODEL" "$body" <<'PY'
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
}

run_gates() {
  local worktree="$1"
  if [ -f "$worktree/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    echo "== cargo fmt --check =="
    (cd "$worktree" && cargo fmt --check) || return 1
    echo "== cargo clippy =="
    (cd "$worktree" && cargo clippy --all-targets -- -D warnings) || return 1
    echo "== cargo test =="
    (cd "$worktree" && cargo test) || return 1
  else
    echo "== Rust gate skipped (no Cargo.toml or no cargo) =="
  fi
  echo "== docs check =="
  (cd "$worktree" && python3 scripts/check-docs.py) || return 1
  return 0
}

run_issue() {
  local i="$1"
  local issue="${sel_issues[$i]}"
  local title="${sel_titles[$i]}"
  local worktree="${sel_worktrees[$i]}"
  local branch="${sel_branches[$i]}"
  local agent="${sel_agents[$i]}"
  local issue_json title2 body epic tab_json pane cfg=""

  issue_json="$(gh issue view "$issue" --repo "$REPO_SLUG" --json number,title,body)"
  body="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("body") or "")')"
  epic="$(printf '%s' "$body" | python3 -c '
import re, sys
m = re.search(r"Parent epic:\*\* #([0-9]+)", sys.stdin.read())
print(m.group(1) if m else "unknown")
')"

  if [ "$KIND" = "opencode" ]; then
    cfg="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-opencode-XXXXXX")"
    python3 - "$MODEL" "$cfg" <<'PY'
import json, os, sys
model, out = sys.argv[1], sys.argv[2]
path = os.path.expanduser("~/.config/opencode/opencode.json")
data = {}
try:
    with open(path, encoding="utf-8") as handle:
        loaded = json.load(handle)
    if isinstance(loaded, dict):
        data = loaded
except (OSError, ValueError):
    pass
data["$schema"] = data.get("$schema", "https://opencode.ai/config.json")
data["model"] = model
with open(out, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2)
    handle.write("\n")
PY
    trap 'rm -f "$cfg"' EXIT
  fi

  tab_args=(herdr tab create --workspace "$WORKSPACE_ID" --cwd "$worktree" --label "$title" --no-focus)
  [ -n "$cfg" ] && tab_args+=(--env "OPENCODE_CONFIG=$cfg")
  tab_json="$("${tab_args[@]}")"
  pane="$(printf '%s' "$tab_json" | python3 -c '
import json, sys
data = json.load(sys.stdin)
root = data["result"]["root_pane"]
print(root["pane_id"] if isinstance(root, dict) else root)
')"
  [ -n "$pane" ] || { echo "error: no pane from Herdr"; return 2; }

  local base_agent="$agent" suffix=2
  while herdr agent get "$agent" >/dev/null 2>&1; do
    agent="${base_agent}-${suffix}"
    suffix=$((suffix + 1))
  done

  local -a start_args
  start_args=(herdr agent start "$agent" --kind "$KIND" --pane "$pane" --timeout 120000)
  if [ "$KIND" != "opencode" ]; then
    start_args+=(-- --model "$MODEL")
  fi
  if ! "${start_args[@]}"; then
    echo "error: agent start failed in pane $pane"
    return 2
  fi

  gh issue edit "$issue" --repo "$REPO_SLUG" --add-assignee "$GH_ACCOUNT" >/dev/null

  local prompt
  prompt="$(render_prompt "$issue" "$title" "$body" "$worktree" "$branch" "$epic")"
  echo "Sending prompt to $agent"
  if ! herdr agent prompt "$agent" "$prompt" --wait --timeout "$WAIT_MS"; then
    echo "error: agent prompt failed or blocked"
    herdr agent read "$agent" --source recent-unwrapped --lines 160 2>&1 || true
    return 2
  fi

  herdr agent get "$agent" 2>&1 || true

  if ! git -C "$worktree" rev-parse --verify HEAD >/dev/null 2>&1; then
    echo "error: no HEAD in $worktree"
    return 3
  fi
  local commits
  commits="$(git -C "$worktree" log --oneline "$base_ref..HEAD" 2>/dev/null || true)"
  if [ -z "$commits" ]; then
    echo "error: agent did not commit"
    return 3
  fi
  echo "Commits:"
  echo "$commits"
  if [ -n "$(git -C "$worktree" status --porcelain)" ]; then
    echo "error: worktree has uncommitted changes"
    git -C "$worktree" status --short
    return 3
  fi

  local gate_log
  gate_log="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-gate-XXXXXX")"
  if ! run_gates "$worktree" > "$gate_log" 2>&1; then
    cat "$gate_log"
    echo "error: gates failed; branch kept"
    rm -f "$gate_log"
    return 4
  fi
  cat "$gate_log"

  if [ "$OPEN_PR" -eq 1 ]; then
    git -C "$worktree" push -u origin "$branch"
    local existing_pr pr_body pr_title report
    existing_pr="$(gh pr list --repo "$REPO_SLUG" --head "$branch" --json url --jq '.[0].url // empty' 2>/dev/null || true)"
    if [ -n "$existing_pr" ]; then
      echo "PR: $existing_pr"
    else
      pr_body="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-pr-XXXXXX")"
      report="$(tail -c 5000 "$gate_log")"
      cat > "$pr_body" <<EOF
Closes #$issue

Parent epic: #$epic

## Summary

Automated implementation in Herdr with \`$KIND\` and model \`$MODEL\`.

Branch: \`$branch\`

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
      pr_title="Implement #$issue: $title"
      gh pr create --repo "$REPO_SLUG" --base main --head "$branch" --title "$pr_title" --body-file "$pr_body"
      rm -f "$pr_body"
    fi
  fi
  rm -f "$gate_log"
}

# ---------------------------------------------------------------------------
# Launch all workers
# ---------------------------------------------------------------------------

pids=()
for i in "${!sel_issues[@]}"; do
  issue="${sel_issues[$i]}"
  ( run_issue "$i" ) > "$LOG_DIR/$issue.log" 2>&1 &
  pids+=("$!")
done

failed=0
printf '\nResults:\n'
for i in "${!sel_issues[@]}"; do
  issue="${sel_issues[$i]}"
  if wait "${pids[$i]}"; then
    status=0
  else
    status=$?
  fi
  pr="$(sed -n 's/^PR: *//p' "$LOG_DIR/$issue.log" | tail -n 1)"
  if [ "$status" -eq 0 ]; then
    printf '  #%s  OK   %s  %s\n' "$issue" "${sel_branches[$i]}" "${pr:-no PR printed}"
  else
    failed=1
    printf '  #%s  FAIL status=%s  log=%s\n' "$issue" "$status" "$LOG_DIR/$issue.log"
  fi
done

if [ "$failed" -ne 0 ]; then
  printf '\nOne or more agents failed. Inspect the logs; nothing was merged.\n' >&2
  exit 1
fi

printf '\nAll agents completed. PRs are open for review; nothing was merged.\n'
