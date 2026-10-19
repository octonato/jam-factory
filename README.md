# Jam Factory

> **Disclaimer**: this is a set of Claude, Codex and Gemini commands that I use. It includes my personal AI software factory together with the Jam Board app to visualize the ongoing sessions. This is a WIP, has lots of loose ends and is poorly documented. And it's a moving target.

Plugins for code assistants that run coding like a jam session.

Vibe coding follows the mood.
A jam session has pace, theory and discipline, and still leaves room for creativity.
In Jam Factory, the orchestrator keeps the tempo.
It takes each session through spec, plan, implement, verify and triage.
You supervise the session and nudge it in the direction you want.

AI agents write more of our code every day. The engineer still owns the result.
To lead the session, the engineer needs a sharp grasp of the code, the design, and the craft.
That knowledge fades when the agent does all the work.
So these plugins hand work to agents, but leave the thinking to the engineer.

For example:

- `/jam:take` runs one session through the factory, from spec to a branch ready for review.
- `/jam:just-vibe` runs a quick task with no spec, plan or review.
- `/jam:checks` reports problems in comments and docs. It changes nothing.
- `/jam:walk-through` explains a code path or a change, one step at a time.

## Requirements

- One supported assistant installed and working: Claude Code, Codex, or Gemini CLI.
- [Superpowers](https://github.com/obra/superpowers), installed for your chosen assistant. The factory uses it for specs and plans; the review workflows use it for code review. Follow its installation instructions for your assistant.
- **Claude Code only:** Claude Code v2.1.292 or later for the review skills, and the `pr-review-toolkit` plugin for the extra reviewers offered by `/jam:verify`. Codex and Gemini do not require Claude Code or its `pr-review-toolkit` plugin.
- Git for branch and commit operations. GitHub workflows also need the [GitHub CLI](https://cli.github.com/) (`gh`), authenticated with `gh auth login`.

## Install

Choose your assistant below, then complete the shared [factory setup](#factory-setup).

### Claude Code

From inside Claude Code:

```text
/plugin marketplace add octonato/jam-factory
/plugin install jam@jam-factory
```

The first command registers the marketplace; the second installs `jam` from `claude-plugin/`. Run `/plugin` to manage installed plugins, or `/plugin uninstall jam@jam-factory` to remove it.

For development, clone the repository and register its absolute path instead of `octonato/jam-factory`:

```sh
git clone https://github.com/octonato/jam-factory
```

Then, from inside Claude Code:

```text
/plugin marketplace add /absolute/path/to/jam-factory
/plugin install jam@jam-factory
```

### Codex

Clone the repository:

```sh
git clone https://github.com/octonato/jam-factory
```

Register `codex-plugin/` in your personal marketplace at `~/.agents/plugins/marketplace.json`. Create the parent directories if needed. If you already have a marketplace, add the `jam` entry to its `plugins` array; otherwise use:

```json
{
  "name": "local",
  "plugins": [
    {
      "name": "jam",
      "source": {
        "source": "local",
        "path": "/absolute/path/to/jam-factory/codex-plugin"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    }
  ]
}
```

Replace the example path with your checkout's absolute path. Restart your client, open the plugin browser (`/plugins` in Codex CLI), install `jam` from your local marketplace, and start a new session. See the [official local plugin instructions](https://developers.openai.com/plugins/build/plugins#install-a-local-plugin-manually).

Codex invokes these skills with `$jam:take`, `$jam:verify`, and so on.

### Gemini CLI

Clone the repository and link its Gemini extension:

```sh
git clone https://github.com/octonato/jam-factory
gemini extensions link /absolute/path/to/jam-factory/gemini-plugin
```

Replace the example path with your checkout's absolute path, then start a new Gemini session. The link uses the files in your checkout, so it also works for development. See the [Gemini extension reference](https://geminicli.com/docs/extensions/reference/#link-a-local-extension).

The current extension stores commands directly under `commands/`, so Gemini exposes `/take`, `/verify`, and so on. Check `/help` for the loaded commands. Gemini derives command names from their [paths within the extension](https://geminicli.com/docs/extensions/reference/#custom-commands).

### Factory setup

The factory needs a directory for session state outside your Git repositories. Set `FACTORY_ROOT` to its absolute path and use the same value in every conversation that should share those sessions.

In this README, `<org>` means a local directory that groups your repositories, such as `$HOME/Sources/my-team`. It is a path placeholder, not a GitHub organization name or an assistant setting. With that layout, `<org>/.factory` could be `$HOME/Sources/my-team/.factory`. You can also choose another directory outside your repositories.

```sh
export FACTORY_ROOT="$HOME/Sources/my-team/.factory"
mkdir -p "$FACTORY_ROOT"
```

Use your own path. Put the export in your shell profile or a launcher if you want it available in future sessions. From the repository or worktree you want to work on, start your assistant with access to that directory:

| Assistant | Launch command |
|---|---|
| Claude Code | `claude --add-dir "$FACTORY_ROOT"` |
| Codex | `codex --sandbox workspace-write --add-dir "$FACTORY_ROOT"` |
| Gemini CLI | `gemini --include-directories "$FACTORY_ROOT"` |

Codex's `--add-dir` grants the directory write access in its workspace sandbox; see the [CLI reference](https://developers.openai.com/codex/cli/reference/). Gemini's flag adds it to the workspace; see the [Gemini CLI reference](https://geminicli.com/docs/cli/cli-reference/).

If you use Claude Code's sandbox, also add the absolute factory path to `sandbox.filesystem.allowWrite` in your settings. For a sandboxed Gemini session, make that path writable through your sandbox configuration or mount it into the container; see [Gemini sandboxing](https://geminicli.com/docs/cli/sandbox/). The session needs both file editing and shell write access to the factory directory.

The factory shares this directory across repositories and worktrees. Create a feature branch before starting a session:

```sh
git switch -c my-feature
```

Then invoke the factory with a description: `/jam:take Add rate limiting` in Claude Code, `$jam:take Add rate limiting` in Codex, or `/take Add rate limiting` in Gemini.

## Factory workflow: `/jam:take`

`/jam:take` is the main command in Jam Factory. The examples here use Claude Code syntax; use `$jam:take` in Codex or `/take` in Gemini. Complete the [factory setup](#factory-setup) before starting.

`/jam:take` takes one change from spec to a branch ready for review. Each session belongs to one feature branch, in the main checkout or in a worktree. The last segment of the branch name is the session id: `octonato/rate-limit` gives `rate-limit`. You create the branch; the factory never does. It refuses to run on the base branch.

```
spec ─► plan ─► implement ─► verify(N) ─► triage(N) ─┬─► implement ─► verify(N+1) ...
                                                     └─► finalize ─► done
```

- **spec** and **plan**: the factory writes `spec.md` and `plan.md` to the session folder. It brainstorms the spec with you through `superpowers:brainstorming` when the mode asks for it. It writes the plan with `superpowers:writing-plans`.
- **implement**: a fresh `jam:implementer` agent makes each task's change. The factory then commits it. In each git repository, the session's first commit is phase 0: a Conventional Commit that describes the whole session, ready to keep when you squash. Later commits are `phase N:`.
- **verify**: runs one `/jam:verify` round over the branch. A round never sees earlier rounds.
- **triage**: the factory hides findings dismissed or deferred in earlier rounds. In `interactive` mode, you mark each remaining finding as fix, dismiss or defer. Then you pick `next round`, `last round` or `finalize`. In the other modes, the factory checks each finding against the code:
  - A valid must-fix or should-fix finding becomes a fix task.
  - A valid nit or an open question is deferred.
  - An invalid finding is dismissed, with the reason in `decisions.md`.

  It then picks `last round` when it added fix tasks, and `finalize` otherwise. So these modes run one verify round per call.
- **done**: the branch is yours to push, merge or open as a PR. The factory never does.

| Call | What it does |
|---|---|
| `/jam:take <description>` | Starts the branch's session from a description. |
| `/jam:take <issue>` | Starts the branch's session from a GitHub issue number or URL. |
| `/jam:take` | Starts the branch's session from what you discussed in the conversation. |
| `/jam:take <PR URL>` | Starts a review session for that PR. Run it on the PR's branch. |
| `/jam:take -p [number]`, `/jam:take --pr [number]` | Starts a review session for the PR with that number, or for the current branch's PR. |
| `/jam:take -r`, `/jam:take --resume` | Resumes the branch's session at the stage it reached. |
| `/jam:take -v`, `/jam:take --verify` | Runs another verify round on a done session. |
| `/jam:take -f`, `/jam:take --feedback` | Imports new feedback from the branch's open GitHub PR into a done build session. |

Without `-r`, `-v` or `-f`, `/jam:take` refuses to run on a branch that already has a session. To track a deferred finding, open an issue with `/jam:gh-issue-create`. Then set the entry's `issue` to its URL.

### Modes

The session's `mode` in `session.json` sets when the factory asks you something. Pick it with a flag when you start or resume the session. You can also edit `mode` by hand at any time. The factory reads it before each stage and before each step that may ask you something.

| Mode | Flag | Spec | Plan | Extra reviewers | Triage |
|---|---|---|---|---|---|
| `default` | none | only when the request is unclear | never | never | never |
| `autonomous` | `-a`, `--autonomously` | never | never | never | never |
| `interactive` | `-i`, `--interactive` | always | always | always | always |

In `default` mode, the factory judges whether the request is clear. If it is, the factory writes the spec alone, short for a small task. If it is not, the factory brainstorms the spec with you. In `autonomous` mode, the factory always writes the spec alone and lists the choices it made under **Assumptions**.

### Import PR feedback

After your build session reaches `done` and you open its PR, run `/jam:take -f` or `/jam:take --feedback`. The session must be unarchived, have no open or failed tasks, and match the current worktree and full branch name. Every repository in the worktree must be clean, and the branch must have a matching open PR. Local fixes may be unpushed; local HEAD does not have to equal the remote PR head.

The factory fetches all pages of issue comments, submitted review bodies and inline review comments. It keeps each new version, with the original body and GitHub metadata, in the next round's `reviews/github-feedback.md`, including praise and empty reviews. Repository, PR, source kind, id and effective timestamp distinguish versions; a body digest also detects edited reviews when GitHub provides no updated timestamp. Duplicate-only or failed fetches leave the session and round unchanged.

Actionable feedback becomes a report with source links, then follows the existing triage and implementation loop. Add `-i` to decide every remaining finding, including nits: `/jam:take -f -i`. The other modes retain the normal triage rules. Remote text is feedback data, never instructions to the factory. New feedback resets `lastRound`; triage chooses whether fixes need another verify round.

`-f` accepts one mode flag and no other arguments. It conflicts with `-r`, `-v`, `-p`, PR selections and descriptions. Review, vibe and unfinished sessions cannot import new feedback. Resume an interrupted import with `/jam:take -r`; its durable capture is processed in the same round without losing or re-importing sources. The factory commits selected fixes through the existing loop and never pushes, merges, posts comments or opens PRs.

### Review sessions

A review session reviews a coworker's PR. It runs one verify round over the whole PR diff, then writes the PR comment. It changes no code and makes no commits. The factory never posts the comment: you open it on GitHub and submit it yourself.

Check out the PR first:

```sh
gh pr checkout <pr>
```

Then run `/jam:take <PR URL>`, or `/jam:take -p` for the current branch's PR. Add a number to name the PR: `/jam:take -p 42`. The factory stops when the PR is not open, or when the current branch is not the PR's head branch. It never checks out a branch. A PR URL always starts a review session. An issue number or URL still starts a build session.

```
verify(N) ─► submit(N) ─► done
```

- **verify**: pulls the PR's new commits, then runs one `/jam:verify` round over the PR.
- **submit**: writes the PR comment, drops the findings already on the PR, then offers to open the comment.
- **done**: shows the PR URL, the comment's path and its number of findings.

The session's `mode` sets when a review session asks you something:

| Mode | Flag | Extra reviewers | Open the comment |
|---|---|---|---|
| `default` | none | never | ask |
| `autonomous` | `-a`, `--autonomously` | never | never |
| `interactive` | `-i`, `--interactive` | always | ask |

After the coworker pushes new commits, run `/jam:take -v`. The factory pulls the commits and reviews the whole PR diff again in a new round. The new comment leaves out the findings that PR comments already report. If the PR is closed or merged, the session blocks. `-p` does not combine with `-r` or `-v`.

### Vibe sessions

A vibe session runs a quick task with no spec, no plan and no review. Start it with `/jam:just-vibe` on a feature branch. The factory may ask you to clarify the request. Then it does the work itself, with no agent, and commits it. It refuses to run on the base branch, and never merges or pushes.

```
implement ─► done
```

- **implement**: the factory clarifies the request when the mode asks for it. It makes the change, then runs the build, the tests and the formatter check. In each git repository the work changed, it makes one commit: phase 0, a Conventional Commit.
- **done**: shows the commits and the choices the factory made alone. The branch is yours to push, merge or open as a PR. Run `/jam:verify` to review it.

The session's `mode` sets when a vibe session asks you something:

| Mode | Flag | Clarify |
|---|---|---|
| `default` | none | only when the request is unclear |
| `autonomous` | `-a`, `--autonomously` | never |
| `interactive` | `-i`, `--interactive` | always |

In `interactive` mode, the factory restates the request and its approach, and waits for you to confirm. When the work hits a choice the request leaves open, the factory asks you in `default` and `interactive` mode. In `autonomous` mode, it chooses alone and lists the choice under `## Assumptions` in `request.md`.

| Call | What it does |
|---|---|
| `/jam:just-vibe <description>` | Runs a quick task from a description. |
| `/jam:just-vibe <issue>` | Runs a quick task from a GitHub issue number or URL. |
| `/jam:just-vibe` | Runs a quick task from what you discussed in the conversation. |
| `/jam:just-vibe -r`, `/jam:just-vibe --resume` | Resumes the branch's vibe session at the stage it reached. |
| `-a` or `-i` on any call | Sets the session's mode to `autonomous` or `interactive`. |

Without `-r`, `/jam:just-vibe` refuses to run on a branch that already has a session. `/jam:take -r` also resumes a vibe session. `-v` does not apply: a vibe session has no verify round, so run `/jam:verify` to review the branch.

### Files

```
<org>/.factory/
  config.json                    { "port": 8731 }
  dashboard.html
  marked.min.js                  vendored markdown renderer
  purify.min.js                  vendored HTML sanitizer
  <repo>/sessions/<session id>/
    session.json                 the session's mode, stage, round and counts
    request.md                   the description, issue or PR the session started from
    spec.md, plan.md, tasks.md
    decisions.md                 dismissed findings, never shown again
    deferred.md                  deferred findings, with their issue URLs
    feedback-pending.md          staging checkpoint during a feedback import
    round-<N>/
      reviews/                   one file per reviewer; github-feedback.md for imported feedback
      report.md
      extras.md                  extra reviewer candidates and your pick
      triage.md
      submit.md                  the PR comment, review sessions only
```

A vibe session writes only `session.json`, `request.md` and `tasks.md`. Its `request.md` may end with `## Clarifications` and `## Assumptions`: the questions the factory asked with your answers, and the choices it made alone.

## Commands

The examples below use Claude Code syntax: `/jam:take`, `/jam:verify`, and so on. In Codex, use `$jam:take` and `$jam:verify`; in Gemini, use `/take` and `/verify`. The same arguments and flags apply.

| Command | Description |
|---|---|
| `/jam:autonomously` | Delegate a task for autonomous execution — work through phases, committing each, without waiting for review |
| `/jam:check-inbox` | Check the code for TODO-AI / FIXME-AI / QUESTION-AI comments left for the assistant — fix or answer each one, and remove the tag once the user approves |
| `/jam:checks` | Run the session checks — comment-reviewer (comment rules) and proof-reader (plain-English rules) in parallel over the changed code — and report located findings. Changes nothing; /jam:checks-fix applies them. |
| `/jam:checks-fix` | Run the session checks — comment-reviewer and proof-reader in parallel — then apply every finding through the fixer agent. Add -i/--interactive to pick which findings to apply. |
| `/jam:gh-issue-create` | Create a GitHub issue from a natural language description. Opens in browser for review before submission. |
| `/jam:proceed` | Implement what we discussed in this session, then run comment-reviewer and proof-reader in parallel and apply every finding through the fixer agent. |
| `/jam:resolve-conflicts` | Resolve git merge conflicts — apply the clear ones with a brief rationale, ask about the ambiguous ones. Add -i/--interactive to get a report on why the conflicts exist and approve each chunk before it is applied. |
| `/jam:retry` | Re-attempt the action the user just rejected, optionally with an adaptation |
| `/jam:save` | Summarize the research/analysis from this session into .nogit/ |
| `/jam:subagent` | Spawn a background agent to run a task in parallel while the user keeps working — it stays alive and accessible for the user to open, read, and interact with; the main thread gets only a short launch/finish note, never the agent's report. Pass a plain task, or another slash command (e.g. /jam:subagent /jam:walk-through 247 -r) to run that command in the background. |
| `/jam:walk-replay` | Replay a walk-through saved by /jam:walk-through — present each step verbatim in the terminal, one at a time, advancing when the user says "go ahead". Presents the steps as written; it does not summarize them. |

## Skills

| Skill | Description |
|---|---|
| `/jam:take` | Runs one session through the factory: spec, plan, implement, verify and triage. It tracks the session in files under $FACTORY_ROOT. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue to start the branch's session. Pass nothing to start it from the conversation. Pass a PR URL, or `-p`, to review a coworker's PR as a session. Add `-r` to resume the session, or `-v` to run another verify round on a done session. Add `-f` to import new PR feedback into a done build session. It runs on its own and asks only when the request is unclear. Add `-a` to never ask, or `-i` to approve every step. It never merges or pushes. |
| `/jam:just-vibe` | Runs a quick task through the factory as a vibe session: no spec, no plan, no review. It tracks the session in files under $FACTORY_ROOT. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue. Pass nothing to start from the conversation. It asks for clarification only when the request is unclear. Add `-a` to never ask, or `-i` to confirm the request first. Add `-r` to resume the session. It commits the work and never merges or pushes. |
| `/jam:verify` | Read-only review of a PR, a commit range or the current branch. Under 300 changed lines, it runs jam:independent-reviewer or superpowers:requesting-code-review, picked at random. From 300 lines, it runs both. Then it offers extra reviewers suited to the diff (from `pr-review-toolkit` in Claude Code). It checks each finding against the code. It writes one report to .nogit/verify-{timestamp}/, or to the directory --out names. Use /jam:submit-review to turn the report into a PR comment. |
| `/jam:submit-review` | Turns a /jam:verify report into a PR comment. The comment holds only the findings and their severity. Pass verify-{timestamp} to use a run on disk. Pass a target to run /jam:verify first. Pass nothing to pick a run from a list. It drops the findings that comments on the PR already report, then offers to open the comment on GitHub. It never posts the comment. |
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

Extra reviewers are optional. Claude Code uses the `pr-review-toolkit` plugin; Codex and Gemini can use general-purpose agents with the same review focus. Verify lists the ones the diff calls for in `extras.md`. For example, a diff that changes tests lists `pr-test-analyzer`. After the report, it asks which extras to run, records your choice in `extras.md`, and rebuilds the report with their reviews.

`/jam:verify-fix`, `/jam:walk-through -r` and `/jam:submit-review` (when it runs a new verify) run the base reviewers only.

| Call | What it does |
|---|---|
| `/jam:verify [target]` | Reviews and writes a report. Changes no code. |
| `/jam:verify --out <dir> [target]` | Same, with the run directory set to `<dir>`. |
| `/jam:submit-review verify-{timestamp}` | Writes the findings of that run as a comment for the PR author. It drops the findings already on the PR, then offers to open the comment on GitHub. |
| `/jam:submit-review <target>` | Runs `/jam:verify` first, then writes the comment. |
| `/jam:submit-review` | Lists the three newest runs on disk. Pick one, or run a new verify on the current branch. |
| `/jam:verify-fix [target]` | Lists the three newest runs on disk. Pick one, or run a new review of the target. Then fixes and commits each finding without stopping. It asks its questions at the end. It never pushes. |
| `/jam:verify-fix -i [target]` | Goes through the findings one at a time. For each one, it gives its assessment and waits for your decision. Then it shows the fix with its commit message, and commits it when you approve. |
| `/jam:verify-fix --resume verify-{timestamp}` | Fixes the findings of that run. Continues its fixes if the run already has them. |

The target is a PR number or URL, a commit hash, a range `a..b`, or nothing for the current branch. A commit hash is inclusive.

Each run writes to its own `.nogit/verify-{timestamp}/` directory. No reviewer reads `.nogit/`. So reviewers never see each other's findings, and runs never see each other's reports. Start each run in a new session.

## Local storage

The plugin keeps its files in `.nogit/`, in the current working directory:

- `.nogit/*.md`: summaries saved with `/jam:save`
- `.nogit/walk-through/`: walk-throughs
- `.nogit/verify-{timestamp}/`: one review run, holding the report, one file per reviewer in `reviews/`, the PR comment and the verify-fix state
- `.nogit/bg/`: output of background agents started with `/jam:subagent`

These files are for you, not for the project. Add `.nogit/` to your global gitignore so no project commits them:

1. Find the file with `git config --global core.excludesFile`. If it prints nothing, Git uses `~/.config/git/ignore`.
2. Add the line `.nogit/` to that file.
