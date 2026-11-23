# Herdr Issue Agent

`scripts/herdr-issue-agent.sh` launches one implementation agent in Herdr, in its own git worktree, for one GitHub subtask issue. It selects the model you pass, waits for the agent, runs the local gates, pushes the branch, and opens a pull request. It never merges.

## Requirements

- Run it **inside a Herdr pane** (`HERDR_ENV=1`). The script refuses to control a session from a plain terminal.
- `gh` authenticated as `ABOYrbx`.
- `python3`.
- Optional: `fzf` for the searchable issue picker (otherwise a numbered menu is used).
- The selected agent kind installed and ready in Herdr:
  - `opencode` (default)
  - `codex`, `claude`, `gemini`, `cursor`, `hermes`, and the other Herdr-supported kinds
- The model must be valid for that kind. For OpenCode, it must appear in `opencode models`.

## Team mode (up to four agents)

```bash
scripts/herdr-issue-team.sh
```

Select up to four open subtask issues. The script:

1. Creates a new Herdr workspace named `agents-<timestamp>`.
2. Prepares one worktree and branch per issue.
3. Opens one tab per issue, titled with the issue.
4. Starts one agent per issue with the chosen kind/model.
5. Waits for all agents concurrently.
6. Lets each launcher run its gates, push, and open its own PR.
7. Prints a per-issue result table.

Non-interactive selection:

```bash
scripts/herdr-issue-team.sh \
  --issues 11,12,13,14 \
  --kind opencode \
  --model opencode-go/deepseek-v4.1-flash
```

The team script never merges. Each issue gets its own PR and handoff. The initial workspace tab is renamed `control`; the agent tabs carry the issue titles.

## Interactive dispatcher

The easiest entry point:

```bash
scripts/herdr-issue-agent-interactive.sh
```

It lists open subtask issues with `fzf` (or a numbered menu), asks for the agent kind, model, and PR preference, then calls the launcher.

```bash
# Preview the open implementation issues without Herdr
scripts/herdr-issue-agent-interactive.sh --list

# Skip the picker or prompts
scripts/herdr-issue-agent-interactive.sh --issue 11 --kind opencode --model opencode-go/deepseek-v4.1-flash
```

The implementation prompt is the maintained template in [`agent-prompts/implement-issue.md`](agent-prompts/implement-issue.md). It is rendered with the issue, worktree, branch, model, and project rules. Edit the template, not the launcher, when the agent instructions change.

## Usage

```bash
scripts/herdr-issue-agent.sh --issue 11 --model opencode-go/deepseek-v4.1-flash
```

```bash
scripts/herdr-issue-agent.sh \
  --issue 167 \
  --kind codex \
  --model gpt-5.6-luna \
  --no-pr
```

```bash
# Show the plan without changing anything
scripts/herdr-issue-agent.sh --issue 11 --model opencode-go/deepseek-v4.1-flash --dry-run
```

## What it does

1. Verifies Herdr, `gh`, and the account.
2. Loads issue #N and refuses wave epics.
3. Creates `../home-assistant-kiosk-N` and branch `agent/N-<slug>` from `origin/main`.
4. Opens a Herdr tab in that worktree.
5. Starts the chosen agent with the chosen model.
6. Sends a complete implementation prompt with the issue, project rules, gates, and handoff requirements.
7. Waits up to 30 minutes (configurable).
8. Runs `cargo fmt --check`, `cargo clippy`, `cargo test` when a `Cargo.toml` and `cargo` exist, plus `python3 scripts/check-docs.py`.
9. Pushes the branch and opens a PR with `Closes #N` and the agent/gate report.
10. Leaves the tab, worktree, and branch for review.

The script does **not** merge. The PR must be reviewed.

## Options

| Option | Meaning |
| --- | --- |
| `--issue <n>` | Required GitHub subtask issue |
| `--model <model>` | Required model for the agent kind |
| `--kind <kind>` | Herdr agent kind (default `opencode`) |
| `--base <ref>` | Base ref (default `origin/main`) |
| `--worktree <path>` | Custom worktree path |
| `--branch <name>` | Custom branch |
| `--name <name>` | Custom Herdr agent name |
| `--wait-timeout <ms>` | Agent wait timeout (default 30 min) |
| `--no-pr` | Do not push or open a PR |
| `--reuse` | Reuse an existing worktree/branch |
| `--agent-arg <arg>` | Extra native agent argument (repeatable) |
| `--dry-run` | Print the plan only |

## Model handling

- **OpenCode:** the script writes a temporary config that merges the global OpenCode config and sets `model`; the Herdr tab gets `OPENCODE_CONFIG=<temp>`. The file is `0600` and removed when the launcher exits.
- **Codex / Claude / Gemini / other kinds:** the script passes `--model <model>` as a native agent argument, followed by any `--agent-arg` values.
- If the model is wrong, `herdr agent start` or the agent itself reports it; the script exits and keeps the pane available for inspection.

## When the agent is blocked

Herdr reports `blocked` when the agent asks a question or needs approval. The script does not answer it. It prints:

```bash
herdr agent read issue-11 --source recent-unwrapped --lines 160
herdr agent attach issue-11
herdr agent prompt issue-11 "<answer>" --wait
```

Answer in the Herdr pane, then fix or re-run the launcher with `--reuse` if needed.

## Gate failure

If a gate fails, the script exits with code `4` and keeps the worktree and branch. It does not push or open a PR. Inspect the agent output and the gate log, fix the issue, then re-run with `--reuse` or continue manually.

## Commit failure

If the agent finishes without a commit, the script exits with code `3`. Read the agent output, prompt it to finish, or continue manually. It will not open an empty PR.

## Reusing a worktree

```bash
scripts/herdr-issue-agent.sh \
  --issue 11 \
  --model opencode-go/deepseek-v4.1-flash \
  --reuse
```

`--reuse` requires the existing worktree to be on the expected branch. The script never resets or deletes your work.

## Troubleshooting

**`Herdr server is not reachable`**  
Start Herdr and run the script from a Herdr pane, not a plain terminal.

**`gh is authenticated as ...`**  
Switch to `ABOYrbx` and verify `gh auth status`.

**`issue #N is a wave epic`**  
Pick a subtask issue from `docs/issues/index.md`.

**`worktree already exists` / `branch already exists`**  
Use `--reuse`, or pick a different `--worktree`/`--branch`.

**Agent starts with the wrong model**  
Check `opencode models` for OpenCode, or the agent's model flag for Codex/Claude/etc. Pass the model exactly as the agent expects it.
