# octo-stack

Plugins for code assistants that keep the engineer in charge and sharp.

AI agents write more of our code every day. The engineer still owns the result.
To direct agents well, the engineer needs a sharp grasp of the code, the design, and the craft.
That knowledge fades when the agent does all the work.

These plugins hand work to agents, but leave the thinking to the engineer.
Some skills point out problems and let you fix them.
Others walk you through code so you understand it before you change it.

For example:

- `/oc:checks` reports problems in comments and docs. It changes nothing.
- `/oc:resolve-conflicts` applies the clear merge fixes and asks about the ambiguous ones.
- `/oc:walk-through` explains a code path or a change, one step at a time.

## Requirements

- [Claude Code](https://docs.claude.com/en/docs/claude-code) installed and working. The review skills need v2.1.292 or later.
- The `superpowers` plugin. `/oc:verify`, `/oc:verify-fix`, `/oc:submit-review` and `/oc:walk-through -r` need it.
- The `pr-review-toolkit` plugin. `/oc:verify` offers its reviewers as extras.

## Install

The plugin is distributed through a Claude Code marketplace. From inside Claude Code:

```
/plugin marketplace add octonato/octo-stack
/plugin install oc@octo-stack
```

The first command registers the marketplace; the second installs the `oc` plugin from it. Run `/plugin` at any time to manage installed plugins.

The marketplace lives at the root of the `octo-stack` repository, in `.claude-plugin/marketplace.json`. It points at the `claude-plugin/` folder.

## Install from a local clone (development)

If you are working on the plugin itself, point Claude Code at your local checkout instead:

```
git clone https://github.com/octonato/octo-stack
cd octo-stack
```

Then, from inside Claude Code:

```
/plugin marketplace add /absolute/path/to/octo-stack
/plugin install oc@octo-stack
```

Claude Code picks up changes under `claude-plugin/commands/`, `claude-plugin/skills/` and `claude-plugin/agents/` the next time it loads the plugin.

## Uninstall

```
/plugin uninstall oc@octo-stack
```

## Commands

Commands and skills are invoked with the `oc:` prefix — `/oc:checks`, `/oc:verify-fix`, and so on.

| Command | Description |
|---|---|
| `/oc:autonomously` | Delegate a task for autonomous execution — work through phases, committing each, without waiting for review |
| `/oc:check-inbox` | Check the code for TODO-AI / FIXME-AI / QUESTION-AI comments left for Claude — fix or answer each one, and remove the tag once the user approves |
| `/oc:checks` | Run the session checks — comment-reviewer (comment rules) and proof-reader (plain-English rules) in parallel over the changed code — and report located findings. Changes nothing; /oc:checks-fix applies them. |
| `/oc:checks-fix` | Run the session checks — comment-reviewer and proof-reader in parallel — then apply every finding through the fixer agent. Add -i/--interactive to pick which findings to apply. |
| `/oc:gh.issue-create` | Create a GitHub issue from a natural language description. Opens in browser for review before submission. |
| `/oc:proceed` | Implement what we discussed in this session, then run comment-reviewer and proof-reader in parallel and apply every finding through the fixer agent. |
| `/oc:resolve-conflicts` | Resolve git merge conflicts — apply the clear ones with a brief rationale, ask about the ambiguous ones. Add -i/--interactive to get a report on why the conflicts exist and approve each chunk before it is applied. |
| `/oc:retry` | Re-attempt the action the user just rejected, optionally with an adaptation |
| `/oc:save` | Summarize the research/analysis from this session into .nogit/ |
| `/oc:subagent` | Spawn a background agent to run a task in parallel while the user keeps working — it stays alive and accessible for the user to open, read, and interact with; the main thread gets only a short launch/finish note, never the agent's report. Pass a plain task, or another slash command (e.g. /oc:subagent /oc:walk-through 247 -r) to run that command in the background. |
| `/oc:walk-replay` | Replay a walk-through saved by /oc:walk-through — present each step verbatim in the terminal, one at a time, advancing when the user says "go ahead". Presents the steps as written; it does not summarize them. |

## Skills

| Skill | Description |
|---|---|
| `/oc:factory` | Runs one job through the factory: spec, plan, implement, verify and triage. It tracks the job in files under $FACTORY_ROOT. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue to start the branch's job, or nothing to resume it. Pass `issues` to open GitHub issues for deferred findings. It never merges or pushes. |
| `/oc:verify` | Read-only review of a PR, a commit range or the current branch. Under 300 changed lines, it runs oc:independent-reviewer or superpowers:requesting-code-review, picked at random. From 300 lines, it runs both. Then it offers the pr-review-toolkit reviewers the diff calls for as extras. It checks each finding against the code. It writes one report to .nogit/verify-{timestamp}/, or to the directory --out names. Use /oc:submit-review to turn the report into a PR comment. |
| `/oc:submit-review` | Turns an /oc:verify report into a PR comment. The comment holds only the findings and their severity. It drops findings that comments on the PR already report. Pass verify-{timestamp} to use a run on disk. Pass a target to run /oc:verify first. Pass nothing to pick a run from a list. It offers to open the comment on GitHub. It never posts the comment. |
| `/oc:independent-reviewer` | Independent, read-only code review of a PR, a commit range or the current branch. A fresh subagent reviews the change and never learns how it was built. It writes the report to .nogit/verify-{timestamp}/independent-review.md. Add --report-in-chat to get the report in chat instead. |
| `/oc:verify-fix` | Fix the findings of an /oc:verify run, each in its own commit. Runs on its own by default; add -i to fix one at a time. Pass --resume verify-{timestamp} to use a run on disk. Otherwise pick a run from a list, or run a new review. Never pushes. |
| `/oc:walk-through` | Walk-through of a code path or a set of changes, broken into logical steps. By default it saves the whole walk to .nogit/walk-through/ to read later; add -i/--interactive to walk through it one step at a time instead. Add -r/--review to run /oc:verify plus the comment and plain-English checks. It folds their findings into each step. |

## Agents

| Agent | Description |
|---|---|
| `comment-reviewer` | Checks every code comment added or edited in a diff — inline, javadoc, scaladoc, docstrings — against the comment rules in CLAUDE.md. Reports located findings; never edits. |
| `implementer` | Implements one factory task in a worktree. Changes the code, then runs the build, the tests and the formatter check. Never commits. Ends with READY or FAILED. |
| `independent-reviewer` | Independent, read-only code review of a change. Gets the target, the commands to read it and the original problem. It never learns how the change was built. Writes the report to .nogit/verify-{timestamp}/independent-review.md, or returns it in chat. |
| `fixer` | Applies a list of located findings from comment-reviewer and proof-reader — edits comments and prose only, with the smallest change that satisfies the rule. Never touches code, never commits. |
| `report-writer` | Merges the reviews of one /oc:verify run into report.md. Reads only the run's reviews/ folder and the code. Checks each finding against the code and the finding rules. Drops the ones that fail. |
| `proof-reader` | Checks the English in prose added or edited in a diff — comments, doc comments, Markdown and other documentation — against the plain-English rules in CLAUDE.md. Reports located findings; never edits. |

## Verify workflow

`/oc:verify` runs its base reviewers at xhigh effort. Under 300 changed lines, it runs one, picked at random: `oc:independent-reviewer` or `superpowers:requesting-code-review`. From 300 lines, it runs both. The report lists each reviewer and why it ran, so you can compare the two over time.

The `pr-review-toolkit` reviewers are extras. Verify lists the ones the diff calls for in `extras.md`. For example, a diff that changes tests lists `pr-test-analyzer`. After the report, it asks which extras to run, records your choice in `extras.md`, and rebuilds the report with their reviews.

`/oc:verify-fix`, `/oc:walk-through -r` and `/oc:submit-review` (when it runs a new verify) run the base reviewers only.

| Call | What it does |
|---|---|
| `/oc:verify [target]` | Reviews and writes a report. Changes no code. |
| `/oc:verify --out <dir> [target]` | Same, with the run directory set to `<dir>`. |
| `/oc:submit-review verify-{timestamp}` | Writes the findings of that run as a comment for the PR author, and offers to open it on GitHub. |
| `/oc:submit-review <target>` | Runs `/oc:verify` first, then writes the comment. |
| `/oc:submit-review` | Lists the three newest runs on disk. Pick one, or run a new verify on the current branch. |
| `/oc:verify-fix [target]` | Lists the three newest runs on disk. Pick one, or run a new review of the target. Then fixes and commits each finding without stopping. It asks its questions at the end. It never pushes. |
| `/oc:verify-fix -i [target]` | Goes through the findings one at a time. For each one, it gives its assessment and waits for your decision. Then it shows the fix with its commit message, and commits it when you approve. |
| `/oc:verify-fix --resume verify-{timestamp}` | Fixes the findings of that run. Continues its fixes if the run already has them. |

The target is a PR number or URL, a commit hash, a range `a..b`, or nothing for the current branch. A commit hash is inclusive.

Each run writes to its own `.nogit/verify-{timestamp}/` directory. No reviewer reads `.nogit/`. So reviewers never see each other's findings, and runs never see each other's reports. Start each run in a new session.

## Factory workflow

`/oc:factory` takes one change from spec to a branch ready for review. Each job belongs to one feature branch, in the main checkout or in a worktree. The last segment of the branch name is the job id: `octonato/rate-limit` gives `rate-limit`. You create the branch; the factory never does. It refuses to run on the base branch.

```
spec ─► plan ─► implement ─► verify(N) ─► triage(N) ─┬─► implement ─► verify(N+1) ...
                                                     └─► finalize ─► done
```

- **spec** and **plan**: `superpowers:brainstorming` and `superpowers:writing-plans` write `spec.md` and `plan.md` to the job folder. You approve both.
- **implement**: a fresh `oc:implementer` agent makes each task's change. The factory then commits it. In each git repository, the job's first commit is phase 0: a Conventional Commit that describes the whole job, ready to keep when you squash. Later commits are `phase N:`.
- **verify**: runs one `/oc:verify` round over the branch. A round never sees earlier rounds.
- **triage**: the factory hides findings you dismissed or deferred in earlier rounds. You mark each remaining finding as fix, dismiss or defer. Then you pick `next round`, `last round` or `finalize`.
- **done**: the branch is yours to push, merge or open as a PR. The factory never does.

| Call | What it does |
|---|---|
| `/oc:factory <description>` | Starts the branch's job from a description. |
| `/oc:factory <issue>` | Starts the branch's job from a GitHub issue number or URL. |
| `/oc:factory` | Resumes the branch's job at the stage it reached. Without a job, it asks what to build. |
| `/oc:factory issues` | Opens a GitHub issue for each deferred finding that has none, and records its URL. |

### Setup

The factory keeps its files outside git, in one directory per org: `<org>/.factory/`. Every worktree of every repo in the org shares it. Make the script that starts Claude do three things:

1. Export `FACTORY_ROOT=<org>/.factory`.
2. Pass `--add-dir "$FACTORY_ROOT"`, so the Write and Edit tools can reach it without a prompt.
3. Allow sandbox writes to it, with a `sandbox.filesystem.allowWrite` entry in your user settings.

### Files

```
<org>/.factory/
  config.json                    { "port": 8731 }
  dashboard.html
  marked.min.js                  vendored markdown renderer
  purify.min.js                  vendored HTML sanitizer
  <repo>/jobs/<job id>/
    job.json                     the job's stage, round and counts
    request.md                   the description or issue the job started from
    spec.md, plan.md, tasks.md
    decisions.md                 dismissed findings, never shown again
    deferred.md                  deferred findings, with their issue URLs
    round-<N>/
      reviews/                   one file per reviewer
      report.md
      extras.md                  extra reviewer candidates and your pick
      triage.md
```

### Dashboard

Run `claude-plugin/skills/factory/serve.sh <org>/.factory` in a terminal. It serves the dashboard at `http://localhost:<port>/dashboard.html`, with the port from `config.json`. Run it outside a session, because the sandbox blocks the port.

The dashboard lists every job in the org, one row per job. Each row shows the job's id, its path and where it is in the flow. Click a row to open the job's page. Each page has its own URL. The page holds a file tree and the open file, rendered from markdown. With no file open, the page shows the flow graph. Click a node to open its file. To try it, serve `claude-plugin/skills/factory/fixtures`.

## Local storage

The plugin keeps its files in `.nogit/`, in the current working directory:

- `.nogit/*.md`: summaries saved with `/oc:save`
- `.nogit/walk-through/`: walk-throughs
- `.nogit/verify-{timestamp}/`: one review run, holding the report, one file per reviewer in `reviews/`, the PR comment and the verify-fix state
- `.nogit/bg/`: output of background agents started with `/oc:subagent`

These files are for you, not for the project. Add `.nogit/` to your global gitignore so no project commits them:

1. Find the file with `git config --global core.excludesFile`. If it prints nothing, Git uses `~/.config/git/ignore`.
2. Add the line `.nogit/` to that file.
