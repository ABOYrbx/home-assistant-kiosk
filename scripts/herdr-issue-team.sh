#!/usr/bin/env bash
#
# Launch up to four Herdr agents in a new workspace, one issue per agent.
#
# Each agent gets:
#   - its own worktree and branch,
#   - its own Herdr tab titled with the issue,
#   - the requested kind and model,
#   - the shared implementation prompt,
#   - its own gate run, push, and pull request.
#
# The script never merges. Run it inside Herdr.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_SLUG="ABOYrbx/home-assistant-kiosk"
GH_ACCOUNT="ABOYrbx"

MAX_AGENTS=4
LIST_ONLY=0
INCLUDE_COMMITS=0
DRY_RUN=0
FOCUS=0
ISSUES_ARG=""
KIND=""
MODEL=""
OPEN_PR=""

usage() {
  cat <<'EOF'
Usage: scripts/herdr-issue-team.sh [options]

Select 1–4 open subtask issues and launch one agent per issue in a new Herdr workspace.

Options:
  --issues <n,n,...>      Preselect issues (comma or space separated)
  --max <1-4>             Maximum agents (default: 4)
  --kind <kind>           Agent kind (default: opencode)
  --model <provider/model> Model for all agents
  --no-pr                 Do not push or open pull requests
  --focus                 Focus the new workspace
  --all                   Include commit-step issues
  --dry-run               Print the plan without creating anything
  --list                  Print selectable issues and exit
  -h, --help              Show this help

Without --issues, a searchable multi-select (fzf) or numbered prompt is used.
The script must run inside a Herdr pane; --list and --dry-run work outside.
EOF
}

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
warn() { printf 'warning: %s\n' "$*" >&2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --issues) [ -n "${2:-}" ] || die "missing value for --issues"; ISSUES_ARG="$2"; shift 2 ;;
    --max) [ -n "${2:-}" ] || die "missing value for --max"; MAX_AGENTS="$2"; shift 2 ;;
    --kind) [ -n "${2:-}" ] || die "missing value for --kind"; KIND="$2"; shift 2 ;;
    --model) [ -n "${2:-}" ] || die "missing value for --model"; MODEL="$2"; shift 2 ;;
    --no-pr) OPEN_PR=0; shift ;;
    --focus) FOCUS=1; shift ;;
    --all) INCLUDE_COMMITS=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --list) LIST_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (use --help)" ;;
  esac
done

[[ "$MAX_AGENTS" =~ ^[1-4]$ ]] || die "--max must be 1-4"

cd "$REPO"

if [ "$LIST_ONLY" -eq 0 ] && [ "$DRY_RUN" -eq 0 ] && [ "${HERDR_ENV:-}" != "1" ]; then
  die "run this inside a Herdr pane (HERDR_ENV=1), or use --list / --dry-run"
fi

gh auth status >/dev/null 2>&1 || die "gh is not authenticated"
account="$(gh api user --jq .login)"
[ "$account" = "$GH_ACCOUNT" ] || die "gh is authenticated as '$account', expected '$GH_ACCOUNT'"

ISSUE_FILE="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-team-issues-XXXXXX")"
cleanup() { rm -f "$ISSUE_FILE"; }
trap cleanup EXIT

gh issue list --repo "$REPO_SLUG" --state open --limit 300 \
  --json number,title,labels,url \
  | python3 "$SCRIPT_DIR/list-open-issues.py" "$([ "$INCLUDE_COMMITS" -eq 1 ] && echo all || echo subtask)" > "$ISSUE_FILE"

if [ ! -s "$ISSUE_FILE" ]; then
  die "no open subtask issues found"
fi

if [ "$LIST_ONLY" -eq 1 ]; then
  awk -F '\t' '{printf "#%s  %s\n", $1, $2}' "$ISSUE_FILE" | head -n 40
  echo "..."
  echo "$(wc -l < "$ISSUE_FILE" | tr -d ' ') issues available; use the picker for the full list."
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

# Keep order and reject duplicates.
sel_issues=()
sel_titles=()
sel_urls=()
sel_labels=()
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

if [ -z "$KIND" ]; then
  read -r -p "Agent kind [opencode]: " KIND
  KIND="${KIND:-opencode}"
fi

if [ -z "$MODEL" ]; then
  if [ "$KIND" = "opencode" ] && command -v opencode >/dev/null 2>&1; then
    MODELS_FILE="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-team-models-XXXXXX")"
    trap 'rm -f "$ISSUE_FILE" "$MODELS_FILE"' EXIT
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

workspace_label="agents-$(date +%Y%m%d-%H%M)"

# Derive paths, branches, and short unique agent names before any Herdr mutation.
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
  worktree="$(dirname "$REPO")/$(basename "$REPO")-$issue"
  branch="agent/$issue-$slug"

  if [[ "$title" =~ ^\[W([0-9]+)\.([0-9]+)\.([0-9]+)\] ]]; then
    agent="w${BASH_REMATCH[1]}-${BASH_REMATCH[2]}-${BASH_REMATCH[3]}"
  else
    agent="issue-$issue"
  fi
  suffix=2
  while herdr agent get "$agent" >/dev/null 2>&1; do
    agent="${agent%-*}-${suffix}"
    suffix=$((suffix + 1))
  done

  sel_worktrees+=("$worktree")
  sel_branches+=("$branch")
  sel_agents+=("$agent")
done

printf '\nWorkspace: %s\n' "$workspace_label"
for i in "${!sel_issues[@]}"; do
  printf '  #%s  %s\n' "${sel_issues[$i]}" "${sel_titles[$i]}"
  printf '       worktree %s\n' "${sel_worktrees[$i]}"
  printf '       branch   %s\n' "${sel_branches[$i]}"
  printf '       agent    %s\n' "${sel_agents[$i]}"
done
printf '  kind/model: %s / %s\n' "$KIND" "$MODEL"
printf '  after work: %s\n' "$([ "$OPEN_PR" -eq 1 ] && echo "push + PR per issue" || echo "leave branches local")"

if [ "$DRY_RUN" -eq 1 ]; then
  echo
  echo "Dry run: nothing was created."
  echo "Planned: create workspace '$workspace_label', prepare worktrees, then run:"
  for i in "${!sel_issues[@]}"; do
    echo "  scripts/herdr-issue-agent.sh --issue ${sel_issues[$i]} --workspace <new> --tab-label \"${sel_titles[$i]}\" --name ${sel_agents[$i]} --worktree \"${sel_worktrees[$i]}\" --branch \"${sel_branches[$i]}\" --reuse --kind $KIND --model $MODEL $([ "$OPEN_PR" -eq 1 ] || echo --no-pr)"
  done
  exit 0
fi

# Prepare worktrees sequentially; concurrent `git worktree add` is avoidable.
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

workspace_json="$(herdr workspace create --cwd "$REPO" --label "$workspace_label" $([ "$FOCUS" -eq 1 ] && echo --focus || echo --no-focus))"
workspace_id="$(printf '%s' "$workspace_json" | python3 -c '
import json, sys
data = json.load(sys.stdin)
workspace = data["result"]["workspace"]
if isinstance(workspace, dict):
    print(workspace.get("workspace_id") or workspace.get("id") or "")
else:
    print(workspace)
')"
[ -n "$workspace_id" ] || die "Herdr did not return a workspace id: $workspace_json"

root_tab="$(printf '%s' "$workspace_json" | python3 -c '
import json, sys
data = json.load(sys.stdin)
tab = data["result"].get("tab") or {}
if isinstance(tab, dict):
    print(tab.get("tab_id") or tab.get("id") or "")
else:
    print(tab)
')"
if [ -n "$root_tab" ]; then
  herdr tab rename "$root_tab" "control" >/dev/null 2>&1 || true
fi

LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ha-kiosk-team-XXXXXX")"
printf '\nWorkspace id: %s\nLogs: %s\n\n' "$workspace_id" "$LOG_DIR"

for i in "${!sel_issues[@]}"; do
  issue="${sel_issues[$i]}"
  args=(--issue "$issue" --workspace "$workspace_id" --tab-label "${sel_titles[$i]}" --name "${sel_agents[$i]}" --worktree "${sel_worktrees[$i]}" --branch "${sel_branches[$i]}" --reuse --kind "$KIND" --model "$MODEL")
  [ "$OPEN_PR" -eq 0 ] && args+=(--no-pr)
  (
    "$SCRIPT_DIR/herdr-issue-agent.sh" "${args[@]}" > "$LOG_DIR/$issue.log" 2>&1
    echo $? > "$LOG_DIR/$issue.status"
  ) &
done

wait

failed=0
printf 'Results:\n'
for i in "${!sel_issues[@]}"; do
  issue="${sel_issues[$i]}"
  status="$(cat "$LOG_DIR/$issue.status" 2>/dev/null || echo 99)"
  pr="$(sed -n 's/^PR: *//p' "$LOG_DIR/$issue.log" | tail -n 1)"
  if [ "$status" -eq 0 ]; then
    printf '  #%s  OK   %s  %s\n' "$issue" "${sel_branches[$i]}" "${pr:-no PR printed}"
  else
    failed=1
    printf '  #%s  FAIL status=%s  log=%s\n' "$issue" "$status" "$LOG_DIR/$issue.log"
  fi
done

if [ "$failed" -ne 0 ]; then
  printf '\nOne or more agents failed. Inspect the logs above; no merge was performed.\n' >&2
  exit 1
fi

printf '\nAll agents completed. PRs are open for review; nothing was merged.\n'
