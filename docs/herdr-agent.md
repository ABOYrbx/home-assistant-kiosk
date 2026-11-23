# Herdr Agent Dispatcher

`./herdr.sh` is the single entry point. It selects one to four open subtask issues, chooses an agent kind and model, creates a new Herdr workspace, and launches one agent per issue. Each agent gets its own worktree, branch, tab titled with the issue, and pull request. The script never merges.

## Requirements

- Run inside a Herdr pane (`HERDR_ENV=1`). `--list` and `--dry-run` work outside.
- `gh` authenticated as `ABOYrbx`.
- `python3`.
- Optional: `fzf` for the searchable multi-select; otherwise a numbered prompt is used.
- The selected agent kind must be installed in Herdr.
- The model must be valid for that kind. For OpenCode, it must appear in `opencode models`.

## Quick start

```bash
./herdr.sh
```

Select 1–4 issues with Tab and Enter in `fzf`, then choose:

1. agent kind (default `opencode`),
2. model,
3. whether to open one PR per issue.

The script creates a workspace named `agents-<timestamp>`, prepares one worktree per issue, starts the agents, waits for them, runs the gates, pushes each branch, and opens one PR per issue. It never merges.

## Non-interactive

```bash
./herdr.sh \
  --issues 11,12,13,14 \
  --kind opencode \
  --model opencode-go/deepseek-v4.1-flash
```

```bash
./herdr.sh --issues 11 --model opencode-go/deepseek-v4.1-flash --no-pr
./herdr.sh --issues 11 --model opencode-go/deepseek-v4.1-flash --dry-run
./herdr.sh --list
./herdr.sh --all --list
```

## Options

| Option | Meaning |
| --- | --- |
| `--issues <n,n,...>` | Preselect issues; skip the picker |
| `--max <1-4>` | Maximum agents (default 4) |
| `--kind <kind>` | Agent kind (default `opencode`) |
| `--model <provider/model>` | Model for all agents |
| `--no-pr` | Do not push or open PRs |
| `--focus` | Focus the new workspace |
| `--all` | Include commit-step issues |
| `--wait-timeout <ms>` | Per-agent timeout (default 30 min) |
| `--list` | List selectable issues and exit |
| `--dry-run` | Print the plan only |
| `--help` | Show help |

## What each agent receives

The prompt is maintained in [`agent-prompts/implement-issue.md`](agent-prompts/implement-issue.md) and rendered with:

- repository, worktree, branch, issue, parent epic, kind, model,
- the full issue body,
- `AGENTS.md`, `README.md`, `PLAN.md`, `docs/github-rules.md`, `DESIGN.md`, `tokens.css`,
- gates, scope, security, handoff, and definition-of-done rules.

Agents must not push, merge, or open a PR. `herdr.sh` does that after the gates pass.

## Titles and PRs

- Workspace: `agents-YYYYMMDD-HHMM`
- Tab: full issue title
- Agent: short name such as `w0-1-1` or `issue-11`
- Branch: `agent/<issue>-<slug>`
- PR: `Implement #<issue>: <title>`, with `Closes #<issue>` and the parent epic

One PR per issue. Nothing is merged.

## Model handling

- **OpenCode:** a temporary `OPENCODE_CONFIG` merges the global OpenCode config and sets `model`; it is passed to the tab. The file is `0600` and removed when the worker exits.
- **Codex / Claude / Gemini / other kinds:** the model is passed as `--model <model>` to the agent.

## Failure handling

- **Agent blocked:** the worker exits and leaves the tab. Inspect with:

  ```bash
  herdr agent read w0-1-1 --source recent-unwrapped --lines 160
  herdr agent attach w0-1-1
  herdr agent prompt w0-1-1 "<answer>" --wait
  ```

- **No commit or dirty worktree:** the worker refuses to push or open a PR; inspect the log.
- **Gate failure:** the worker keeps the branch and does not open a PR.
- **One failure does not stop the others.** The final table shows per-issue status and log paths.

## Logs

Each run keeps a log directory under the system temp directory:

```text
/tmp/ha-kiosk-team-XXXXXXXX/<issue>.log
```

The final table prints the exact paths. Nothing is deleted automatically, so you can review failures.

## Worktrees

Each issue gets:

```text
../home-assistant-kiosk-<issue>
```

The branch is `agent/<issue>-<slug>`. Existing worktrees are reused only if they are on the expected branch. The script never resets or deletes them.
