#!/usr/bin/env bash
#
# Interactive Herdr issue dispatcher.
#
# Lists open GitHub issues, lets you pick one, asks for the agent kind, model,
# and PR preference, then calls scripts/herdr-issue-agent.sh.
#
# Run inside a Herdr pane. Use --list to preview open issues without Herdr.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_SLUG="ABOYrbx/home-assistant-kiosk"
GH_ACCOUNT="ABOYrbx"

LIST_ONLY=0
INCLUDE_COMMITS=0
DRY_RUN=0
ISSUE=""
KIND=""
MODEL=""
OPEN_PR=""

usage() {
  cat <<'EOF'
Usage: scripts/herdr-issue-agent-interactive.sh [options]

Options:
  --list                  Print open implementation issues and exit
  --all                   Include commit-step issues (all subtasks)
  --issue <number>        Skip the issue picker
  --kind <kind>           Skip the agent-kind prompt (default: opencode)
  --model <provider/model> Skip the model prompt
  --no-pr                 Do not open a pull request
  --dry-run               Pass --dry-run to the launcher
  -h, --help              Show this help

Without options, it shows a searchable issue picker (fzf) or a numbered menu.
The launcher runs only inside a Herdr pane.
EOF
}

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
warn() { printf 'warning: %s\n' "$*" >&2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --list) LIST_ONLY=1; shift ;;
    --all) INCLUDE_COMMITS=1; shift ;;
    --issue) [ -n "${2:-}" ] || die "missing value for --issue"; ISSUE="$2"; shift 2 ;;
    --kind) [ -n "${2:-}" ] || die "missing value for --kind"; KIND="$2"; shift 2 ;;
    --model) [ -n "${2:-}" ] || die "missing value for --model"; MODEL="$2"; shift 2 ;;
    --no-pr) OPEN_PR=0; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (use --help)" ;;
  esac
done

cd "$REPO"

if [ "$LIST_ONLY" -eq 0 ] && [ "$DRY_RUN" -eq 0 ] && [ "${HERDR_ENV:-}" != "1" ]; then
  die "run this inside a Herdr pane (HERDR_ENV=1), or use --list / --dry-run"
fi

gh auth status >/dev/null 2>&1 || die "gh is not authenticated"
account="$(gh api user --jq .login)"
[ "$account" = "$GH_ACCOUNT" ] || die "gh is authenticated as '$account', expected '$GH_ACCOUNT'"

ISSUE_FILE="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-issues-XXXXXX")"
cleanup() { rm -f "$ISSUE_FILE"; }
trap cleanup EXIT

gh issue list --repo "$REPO_SLUG" --state open --limit 300 \
  --json number,title,labels,url \
  | python3 "$SCRIPT_DIR/list-open-issues.py" "$([ "$INCLUDE_COMMITS" -eq 1 ] && echo all || echo subtask)" > "$ISSUE_FILE"

if [ ! -s "$ISSUE_FILE" ]; then
  die "no open implementation issues found"
fi

if [ "$LIST_ONLY" -eq 1 ]; then
  awk -F '\t' '{printf "#%s  %s\n", $1, $2}' "$ISSUE_FILE"
  exit 0
fi

if [ -z "$ISSUE" ]; then
  if command -v fzf >/dev/null 2>&1; then
    selection="$(fzf \
      --delimiter='\t' \
      --with-nth=2.. \
      --prompt='Issue > ' \
      --height=70% \
      --reverse \
      < "$ISSUE_FILE")" || die "no issue selected"
    ISSUE="${selection%%$'\t'*}"
  else
    menu=()
    nums=()
    while IFS=$'\t' read -r number title labels url; do
      menu+=("#$number  $title")
      nums+=("$number")
    done < "$ISSUE_FILE"

    echo "Open implementation issues:"
    PS3="Select an issue (number, q to quit): "
    select choice in "${menu[@]}"; do
      if [ "$REPLY" = "q" ] || [ "$REPLY" = "Q" ]; then
        exit 0
      fi
      if [ -n "${choice:-}" ]; then
        ISSUE="${nums[$((REPLY - 1))]}"
        break
      fi
      echo "Invalid selection."
    done
  fi
fi

[[ "$ISSUE" =~ ^[0-9]+$ ]] || die "invalid issue number: $ISSUE"
issue_json="$(gh issue view "$ISSUE" --repo "$REPO_SLUG" --json number,title,state,url,labels,body)"
issue_title="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["title"])')"
issue_state="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])')"
issue_url="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["url"])')"
issue_labels="$(printf '%s' "$issue_json" | python3 -c 'import json,sys; print(",".join(x["name"] for x in json.load(sys.stdin).get("labels", [])))')"

[ "$issue_state" = "OPEN" ] || die "issue #$ISSUE is $issue_state"
case ",$issue_labels," in
  *,epic,*) die "issue #$ISSUE is a wave epic; pick a subtask" ;;
esac

printf '\nSelected: #%s %s\n' "$ISSUE" "$issue_title"
printf 'Labels:   %s\n' "$issue_labels"
printf 'URL:      %s\n\n' "$issue_url"

if [ -z "$KIND" ]; then
  read -r -p "Agent kind [opencode]: " KIND
  KIND="${KIND:-opencode}"
fi

if [ -z "$MODEL" ]; then
  if [ "$KIND" = "opencode" ] && command -v opencode >/dev/null 2>&1; then
    MODELS_FILE="$(mktemp "${TMPDIR:-/tmp}/ha-kiosk-models-XXXXXX")"
    trap 'rm -f "$ISSUE_FILE" "$MODELS_FILE"' EXIT
    if opencode models > "$MODELS_FILE" 2>/dev/null && [ -s "$MODELS_FILE" ]; then
      echo "Available OpenCode models:"
      nl -ba "$MODELS_FILE" | sed -n '1,80p'
      if [ "$(wc -l < "$MODELS_FILE" | tr -d ' ')" -gt 80 ]; then
        echo "  ... more models available; type the exact provider/model"
      fi
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

extra_args=()
if [ -z "$OPEN_PR" ]; then
  read -r -p "Open a pull request after gates? [Y/n]: " answer
  case "$answer" in n|N) OPEN_PR=0 ;; *) OPEN_PR=1 ;; esac
fi
[ "$OPEN_PR" -eq 0 ] && extra_args+=(--no-pr)
[ "$DRY_RUN" -eq 1 ] && extra_args+=(--dry-run)

printf '\nIssue:  #%s %s\n' "$ISSUE" "$issue_title"
printf 'Kind:   %s\n' "$KIND"
printf 'Model:  %s\n' "$MODEL"
printf 'PR:     %s\n' "$([ "$OPEN_PR" -eq 1 ] && echo yes || echo no)"
read -r -p "Start the Herdr agent? [Y/n]: " answer
case "$answer" in n|N) exit 0 ;; esac

exec "$SCRIPT_DIR/herdr-issue-agent.sh" \
  --issue "$ISSUE" \
  --kind "$KIND" \
  --model "$MODEL" \
  "${extra_args[@]}"
