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
- The `pr-review-toolkit` plugin. `/oc:verify`, `/oc:verify-fix` and `/oc:walk-through -r` need it.

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
| `/oc:verify` | Deep, read-only review of a PR, a commit range or the current branch. Runs the pr-review-toolkit reviewers and oc:independent-reviewer in parallel at xhigh effort. It checks each finding against the code. It writes one report to .nogit/verify-{timestamp}/. Add --submit to also write the findings as a comment for the PR author and offer to open it on GitHub. It never posts the comment. |
| `/oc:independent-reviewer` | Independent, read-only code review of a PR, a commit range or the current branch. A fresh subagent reviews the change and never learns how it was built. It writes the report to .nogit/verify-{timestamp}/independent-review.md. Add --report-in-chat to get the report in chat instead. |
| `/oc:verify-fix` | Review your PR with /oc:verify. Fix each finding in its own commit. Runs on its own by default; add -i to fix one at a time. Use --resume verify-{timestamp} to continue a run. Never pushes. |
| `/oc:walk-through` | Walk-through of a code path or a set of changes, broken into logical steps. By default it saves the whole walk to .nogit/walk-through/ to read later; add -i/--interactive to walk through it one step at a time instead. Add -r/--review to run /oc:verify plus the comment and plain-English checks. It folds their findings into each step. |

## Agents

| Agent | Description |
|---|---|
| `comment-reviewer` | Checks every code comment added or edited in a diff — inline, javadoc, scaladoc, docstrings — against the comment rules in CLAUDE.md. Reports located findings; never edits. |
| `independent-reviewer` | Independent, read-only code review of a change. Gets the target, the commands to read it and the original problem. It never learns how the change was built. Writes the report to .nogit/verify-{timestamp}/independent-review.md, or returns it in chat. |
| `fixer` | Applies a list of located findings from comment-reviewer and proof-reader — edits comments and prose only, with the smallest change that satisfies the rule. Never touches code, never commits. |
| `proof-reader` | Checks the English in prose added or edited in a diff — comments, doc comments, Markdown and other documentation — against the plain-English rules in CLAUDE.md. Reports located findings; never edits. |

## Verify workflow

`/oc:verify` runs seven reviewers in parallel at xhigh effort: six `pr-review-toolkit` reviewers and `oc:independent-reviewer`. `/oc:verify-fix` and `/oc:walk-through -r` run the same review.

| Call | What it does |
|---|---|
| `/oc:verify [target]` | Reviews and writes a report. Changes no code. |
| `/oc:verify --submit [target]` | Also writes the findings as a comment for the PR author, and offers to open it on GitHub. |
| `/oc:verify-fix [target]` | Reviews, then fixes and commits each finding without stopping. It asks its questions at the end. It never pushes. |
| `/oc:verify-fix -i [target]` | Shows the plan and waits for approval. Then shows each fix with its commit message, and commits it when you approve. |
| `/oc:verify-fix --resume verify-{timestamp}` | Continues an interrupted verify-fix run. |

The target is a PR number or URL, a commit hash, a range `a..b`, or nothing for the current branch. A commit hash is inclusive.

Each run writes to its own `.nogit/verify-{timestamp}/` directory. No reviewer reads `.nogit/`. So reviewers never see each other's findings, and runs never see each other's reports. Start each run in a new session.

## Local storage

The plugin keeps its files in `.nogit/`, in the current working directory:

- `.nogit/*.md`: summaries saved with `/oc:save`
- `.nogit/walk-through/`: walk-throughs
- `.nogit/verify-{timestamp}/`: one review run, holding the report, the independent review, the PR comment and the verify-fix state
- `.nogit/bg/`: output of background agents started with `/oc:subagent`

These files are for you, not for the project. Add `.nogit/` to your global gitignore so no project commits them:

1. Find the file with `git config --global core.excludesFile`. If it prints nothing, Git uses `~/.config/git/ignore`.
2. Add the line `.nogit/` to that file.
