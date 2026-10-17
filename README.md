# Jam Factory

Plugins for code assistants that run coding like a jam session.

Vibe coding follows the mood.
A jam session has pace, theory and discipline, and still leaves room for creativity.
In Jam Factory, the orchestrator keeps the tempo.
It takes each job through spec, plan, implement, verify and triage.
You supervise the session and nudge it in the direction you want.

AI agents write more of our code every day. The engineer still owns the result.
To lead the session, the engineer needs a sharp grasp of the code, the design, and the craft.
That knowledge fades when the agent does all the work.
So these plugins hand work to agents, but leave the thinking to the engineer.

For example:

- `/jam:take` runs one job through the factory, from spec to a branch ready for review.
- `/jam:checks` reports problems in comments and docs. It changes nothing.
- `/jam:walk-through` explains a code path or a change, one step at a time.

## Requirements

- [Claude Code](https://docs.claude.com/en/docs/claude-code) installed and working. The review skills need v2.1.292 or later.
- The `superpowers` plugin. `/jam:verify`, `/jam:verify-fix`, `/jam:submit-review` and `/jam:walk-through -r` need it.
- The `pr-review-toolkit` plugin. `/jam:verify` offers its reviewers as extras.

## Install

The plugin is distributed through a Claude Code marketplace. From inside Claude Code:

```
/plugin marketplace add octonato/jam-factory
/plugin install jam@jam-factory
```

The first command registers the marketplace; the second installs the `jam` plugin from it. Run `/plugin` at any time to manage installed plugins.

The marketplace lives at the root of the `jam-factory` repository, in `.claude-plugin/marketplace.json`. It points at the `claude-plugin/` folder.

## Install from a local clone (development)

If you are working on the plugin itself, point Claude Code at your local checkout instead:

```
git clone https://github.com/octonato/jam-factory
cd jam-factory
```

Then, from inside Claude Code:

```
/plugin marketplace add /absolute/path/to/jam-factory
/plugin install jam@jam-factory
```

Claude Code picks up changes under `claude-plugin/commands/`, `claude-plugin/skills/` and `claude-plugin/agents/` the next time it loads the plugin.

## Uninstall

```
/plugin uninstall jam@jam-factory
```

## Commands

Commands and skills are invoked with the `jam:` prefix — `/jam:checks`, `/jam:verify-fix`, and so on.

| Command | Description |
|---|---|
| `/jam:autonomously` | Delegate a task for autonomous execution — work through phases, committing each, without waiting for review |
| `/jam:check-inbox` | Check the code for TODO-AI / FIXME-AI / QUESTION-AI comments left for Claude — fix or answer each one, and remove the tag once the user approves |
| `/jam:checks` | Run the session checks — comment-reviewer (comment rules) and proof-reader (plain-English rules) in parallel over the changed code — and report located findings. Changes nothing; /jam:checks-fix applies them. |
| `/jam:checks-fix` | Run the session checks — comment-reviewer and proof-reader in parallel — then apply every finding through the fixer agent. Add -i/--interactive to pick which findings to apply. |
| `/jam:gh.issue-create` | Create a GitHub issue from a natural language description. Opens in browser for review before submission. |
| `/jam:proceed` | Implement what we discussed in this session, then run comment-reviewer and proof-reader in parallel and apply every finding through the fixer agent. |
| `/jam:resolve-conflicts` | Resolve git merge conflicts — apply the clear ones with a brief rationale, ask about the ambiguous ones. Add -i/--interactive to get a report on why the conflicts exist and approve each chunk before it is applied. |
| `/jam:retry` | Re-attempt the action the user just rejected, optionally with an adaptation |
| `/jam:save` | Summarize the research/analysis from this session into .nogit/ |
| `/jam:subagent` | Spawn a background agent to run a task in parallel while the user keeps working — it stays alive and accessible for the user to open, read, and interact with; the main thread gets only a short launch/finish note, never the agent's report. Pass a plain task, or another slash command (e.g. /jam:subagent /jam:walk-through 247 -r) to run that command in the background. |
| `/jam:walk-replay` | Replay a walk-through saved by /jam:walk-through — present each step verbatim in the terminal, one at a time, advancing when the user says "go ahead". Presents the steps as written; it does not summarize them. |

## Skills

| Skill | Description |
|---|---|
| `/jam:take` | Runs one job through the factory: spec, plan, implement, verify and triage. It tracks the job in files under $FACTORY_ROOT. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue to start the branch's job. Pass nothing to start it from the session. Add `-r` to resume the job, or `-v` to run another verify round on a done job. It runs on its own and asks only when the request is unclear. Add `-a` to never ask, or `-i` to approve every step. It never merges or pushes. |
| `/jam:verify` | Read-only review of a PR, a commit range or the current branch. Under 300 changed lines, it runs jam:independent-reviewer or superpowers:requesting-code-review, picked at random. From 300 lines, it runs both. Then it offers the pr-review-toolkit reviewers the diff calls for as extras. It checks each finding against the code. It writes one report to .nogit/verify-{timestamp}/, or to the directory --out names. Use /jam:submit-review to turn the report into a PR comment. |
| `/jam:submit-review` | Turns a /jam:verify report into a PR comment. The comment holds only the findings and their severity. It drops findings that comments on the PR already report. Pass verify-{timestamp} to use a run on disk. Pass a target to run /jam:verify first. Pass nothing to pick a run from a list. It offers to open the comment on GitHub. It never posts the comment. |
| `/jam:independent-reviewer` | Independent, read-only code review of a PR, a commit range or the current branch. A fresh subagent reviews the change and never learns how it was built. It writes the report to .nogit/verify-{timestamp}/independent-review.md. Add --report-in-chat to get the report in chat instead. |
| `/jam:verify-fix` | Fix the findings of a /jam:verify run, each in its own commit. Runs on its own by default; add -i to fix one at a time. Pass --resume verify-{timestamp} to use a run on disk. Otherwise pick a run from a list, or run a new review. Never pushes. |
| `/jam:walk-through` | Walk-through of a code path or a set of changes, broken into logical steps. By default it saves the whole walk to .nogit/walk-through/ to read later; add -i/--interactive to walk through it one step at a time instead. Add -r/--review to run /jam:verify plus the comment and plain-English checks. It folds their findings into each step. |

## Agents

| Agent | Description |
|---|---|
| `comment-reviewer` | Checks every code comment added or edited in a diff — inline, javadoc, scaladoc, docstrings — against the comment rules in CLAUDE.md. Reports located findings; never edits. |
| `implementer` | Implements one factory task in a worktree. Changes the code, then runs the build, the tests and the formatter check. Never commits. Ends with READY or FAILED. |
| `independent-reviewer` | Independent, read-only code review of a change. Gets the target, the commands to read it and the original problem. It never learns how the change was built. Writes the report to .nogit/verify-{timestamp}/independent-review.md, or returns it in chat. |
| `fixer` | Applies a list of located findings from comment-reviewer and proof-reader — edits comments and prose only, with the smallest change that satisfies the rule. Never touches code, never commits. |
| `report-writer` | Merges the reviews of one /jam:verify run into report.md. Reads only the run's reviews/ folder and the code. Checks each finding against the code and the finding rules. Drops the ones that fail. |
| `proof-reader` | Checks the English in prose added or edited in a diff — comments, doc comments, Markdown and other documentation — against the plain-English rules in CLAUDE.md. Reports located findings; never edits. |

## Verify workflow

`/jam:verify` runs its base reviewers at xhigh effort. Under 300 changed lines, it runs one, picked at random: `jam:independent-reviewer` or `superpowers:requesting-code-review`. From 300 lines, it runs both. The report lists each reviewer and why it ran, so you can compare the two over time.

The `pr-review-toolkit` reviewers are extras. Verify lists the ones the diff calls for in `extras.md`. For example, a diff that changes tests lists `pr-test-analyzer`. After the report, it asks which extras to run, records your choice in `extras.md`, and rebuilds the report with their reviews.

`/jam:verify-fix`, `/jam:walk-through -r` and `/jam:submit-review` (when it runs a new verify) run the base reviewers only.

| Call | What it does |
|---|---|
| `/jam:verify [target]` | Reviews and writes a report. Changes no code. |
| `/jam:verify --out <dir> [target]` | Same, with the run directory set to `<dir>`. |
| `/jam:submit-review verify-{timestamp}` | Writes the findings of that run as a comment for the PR author, and offers to open it on GitHub. |
| `/jam:submit-review <target>` | Runs `/jam:verify` first, then writes the comment. |
| `/jam:submit-review` | Lists the three newest runs on disk. Pick one, or run a new verify on the current branch. |
| `/jam:verify-fix [target]` | Lists the three newest runs on disk. Pick one, or run a new review of the target. Then fixes and commits each finding without stopping. It asks its questions at the end. It never pushes. |
| `/jam:verify-fix -i [target]` | Goes through the findings one at a time. For each one, it gives its assessment and waits for your decision. Then it shows the fix with its commit message, and commits it when you approve. |
| `/jam:verify-fix --resume verify-{timestamp}` | Fixes the findings of that run. Continues its fixes if the run already has them. |

The target is a PR number or URL, a commit hash, a range `a..b`, or nothing for the current branch. A commit hash is inclusive.

Each run writes to its own `.nogit/verify-{timestamp}/` directory. No reviewer reads `.nogit/`. So reviewers never see each other's findings, and runs never see each other's reports. Start each run in a new session.

## Factory workflow

`/jam:take` takes one change from spec to a branch ready for review. Each job belongs to one feature branch, in the main checkout or in a worktree. The last segment of the branch name is the job id: `octonato/rate-limit` gives `rate-limit`. You create the branch; the factory never does. It refuses to run on the base branch.

```
spec ─► plan ─► implement ─► verify(N) ─► triage(N) ─┬─► implement ─► verify(N+1) ...
                                                     └─► finalize ─► done
```

- **spec** and **plan**: the factory writes `spec.md` and `plan.md` to the job folder. It brainstorms the spec with you through `superpowers:brainstorming` when the mode asks for it. It writes the plan with `superpowers:writing-plans`.
- **implement**: a fresh `jam:implementer` agent makes each task's change. The factory then commits it. In each git repository, the job's first commit is phase 0: a Conventional Commit that describes the whole job, ready to keep when you squash. Later commits are `phase N:`.
- **verify**: runs one `/jam:verify` round over the branch. A round never sees earlier rounds.
- **triage**: the factory hides findings dismissed or deferred in earlier rounds. In `interactive` mode, you mark each remaining finding as fix, dismiss or defer. Then you pick `next round`, `last round` or `finalize`. In the other modes, the factory checks each finding against the code:
  - A valid must-fix or should-fix finding becomes a fix task.
  - A valid nit or an open question is deferred.
  - An invalid finding is dismissed, with the reason in `decisions.md`.

  It then picks `last round` when it added fix tasks, and `finalize` otherwise. So these modes run one verify round per call.
- **done**: the branch is yours to push, merge or open as a PR. The factory never does.

| Call | What it does |
|---|---|
| `/jam:take <description>` | Starts the branch's job from a description. |
| `/jam:take <issue>` | Starts the branch's job from a GitHub issue number or URL. |
| `/jam:take` | Starts the branch's job from what you discussed in the session. |
| `/jam:take -r`, `/jam:take --resume` | Resumes the branch's job at the stage it reached. |
| `/jam:take -v`, `/jam:take --verify` | Runs another verify round on a done job. |

Without `-r` or `-v`, `/jam:take` refuses to run on a branch that already has a job. To track a deferred finding, open an issue with `/jam:gh.issue-create`. Then set the entry's `issue` to its URL.

### Modes

The job's `mode` in `job.json` sets when the factory asks you something. Pick it with a flag when you start or resume the job. You can also edit `mode` by hand at any time. The factory reads it before each stage and before each step that may ask you something.

| Mode | Flag | Spec | Plan | Extra reviewers | Triage |
|---|---|---|---|---|---|
| `default` | none | only when the request is unclear | never | never | never |
| `autonomous` | `-a`, `--autonomously` | never | never | never | never |
| `interactive` | `-i`, `--interactive` | always | always | always | always |

In `default` mode, the factory judges whether the request is clear. If it is, the factory writes the spec alone, short for a small task. If it is not, the factory brainstorms the spec with you. In `autonomous` mode, the factory always writes the spec alone and lists the choices it made under **Assumptions**.

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
    job.json                     the job's mode, stage, round and counts
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

## Local storage

The plugin keeps its files in `.nogit/`, in the current working directory:

- `.nogit/*.md`: summaries saved with `/jam:save`
- `.nogit/walk-through/`: walk-throughs
- `.nogit/verify-{timestamp}/`: one review run, holding the report, one file per reviewer in `reviews/`, the PR comment and the verify-fix state
- `.nogit/bg/`: output of background agents started with `/jam:subagent`

These files are for you, not for the project. Add `.nogit/` to your global gitignore so no project commits them:

1. Find the file with `git config --global core.excludesFile`. If it prints nothing, Git uses `~/.config/git/ignore`.
2. Add the line `.nogit/` to that file.
