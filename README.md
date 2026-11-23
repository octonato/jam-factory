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
- `/jam:walk-through` explains a code path or a change, one step at a time. It saves the walk as a factory session that you read on Jam Board.

## Requirements

- One supported assistant installed and working: Claude Code, Codex, Gemini CLI, or Antigravity (agy).
- [Superpowers](https://github.com/obra/superpowers), installed for your chosen assistant. The factory uses it for specs and plans; the review workflows use it for code review. Follow its installation instructions for your assistant.
- **Claude Code only:** Claude Code v2.1.292 or later for the review skills, and the `pr-review-toolkit` plugin for the extra reviewers offered by `/jam:verify`. Codex, Gemini and Antigravity do not require Claude Code or its `pr-review-toolkit` plugin.
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

### Antigravity (agy)

Clone the repository and install its Antigravity plugin:

```sh
git clone https://github.com/octonato/jam-factory
agy plugin install /absolute/path/to/jam-factory/antigravity-plugin
```

Replace the example path with your checkout's absolute path. You can manage installed plugins with `agy plugin list` or remove with `agy plugin uninstall jam`.

Antigravity invokes these skills with `/jam:take`, `/jam:verify`, and so on. The agents read your rules from `~/.gemini/GEMINI.md`, and from the project's `GEMINI.md` and `AGENTS.md`.

### Factory setup

The factory keeps session state in files outside your Git repositories. The `jam-factory-mcp` server owns those files. Each plugin starts it as the `jam-factory` MCP server. The skills read and write session files only through that server.

Install the server from a clone of this repository. The build needs [rustup](https://rustup.rs/) and [mise](https://mise.jdx.dev/). `mise install` adds `just`, Node and the Tauri CLI:

```sh
cd /absolute/path/to/jam-factory
mise install
just install
```

`just install` builds Jam Board, `jam-factory-mcp`, `jam-factory-server` and `jb`. It copies the three binaries to `~/.local/bin`, which must be on your `PATH`.

`just install` also installs a launchd agent that keeps `jam-factory-server` running on port 8731. Jam Board and the editors read sessions from `jam-factory-server`. Jam Board shows an error when `jam-factory-server` is down. The log is `~/.jam-factory/logs/server.log`.

`jam-factory-server` serves the dashboard at `http://127.0.0.1:8731/dashboard.html`. The port in that URL follows `port` in `~/.jam-factory/config.json`.

Press `?` on any Jam Board page to list the keyboard shortcuts of every page. Press `?` or `Esc` to close the list.

To stop the agent and remove it with `jam-factory-server`, run this at the repository root:

```sh
just app::uninstall-server
```

`jam-factory-mcp` keeps every session under one root folder, `~/.jam-factory/factory` by default. To use another folder, set `root` in `~/.jam-factory/config.json`. A leading `~/` expands to your home folder. To run `jam-factory-server` on another port, set `port` to a number from 1 to 65535:

```json
{ "root": "~/Sources/factory", "port": 9000 }
```

The environment variable `JAM_FACTORY_PORT` overrides `port` for `jam-factory-server` and Jam Board. It must be a number from 1 to 65535. `./try-branch` uses it to run a second server and board, as [Try a branch's board](#try-a-branchs-board) shows.

To list main checkouts in the Jam Board pop-ups (⇧⌘B, ⇧⌘W and ⇧⌘G) and on its PRs pages, set `repos` to a list of their paths. A leading `~` expands to your home folder. Each path must be absolute after expansion:

```json
{
  "repos": [
    "~/Sources/akka/sdk/main",
    "~/Sources/octonato/jam-factory/main"
  ]
}
```

The ⇧⌘B and ⇧⌘W pop-up lists each checkout with its linked worktrees. A worktree's page lists its live sessions and runs a shell or lazygit in each tab. When the worktree's branch has an open PR, the page's card is the PR's row as the PRs page shows it, with the reviewed and draft chips and the worktree's actions. `jam-factory-server` reads `repos` on each request, so a change to `repos` needs no restart.

To create a worktree, press `b` on the PRs page of a main checkout. Enter the worktree name. Jam Board creates a new branch at the main checkout's HEAD in a sibling folder named after the worktree, then opens the worktree page. To prefix the branch with your GitHub user, set `githubUser`. With `"githubUser": "octonato"`, the name `rate-limit` gives the branch `octonato/rate-limit` in the folder `rate-limit`:

```json
{ "githubUser": "octonato" }
```

The GitHub tab of Jam Board shows your pull requests on github.com, and ⌘R reloads it.

A worktree's GitHub tab shows the worktree's GitHub page inside Jam Board: the branch's PR, open, closed or merged, or the repo page when the branch has no PR. A worktree opens on its GitHub tab, unless it has a terminal tab. The tab keeps GitHub pages and opens other links in your default browser. If your GitHub organization signs in through an identity provider such as Okta, set `githubSignInHosts` to the provider's hosts. The tab then keeps the sign-in pages too. Each entry is a host, such as `sso.example.org`, or `*.` plus a domain, such as `*.okta.com`. A `*.` entry matches every subdomain of the domain, but not the domain itself:

```json
{ "githubSignInHosts": ["*.okta.com"] }
```

Jam Board reads `githubSignInHosts` when a window first shows its GitHub tab. Reopen the window after a change. The tab shows an error when an entry is not a valid host.

Press ⇧⌘G while a GitHub tab shows to open the GitHub pop-up over it. Type to filter the rows. Press ↑ and ↓ to move, `Enter` to run the selected row and `Esc` to close. When the tab shows a PR or an issue of a repo in `repos`, the pop-up lists an action row first:

- **Check out PR #N** checks out the PR into a new sibling worktree, or finds the worktree that holds it. Then it opens the worktree in its own window.
- **Start work on issue #N** asks for the worktree name, with a suggested name in the field. The suggestion joins the first 4 words of the issue title with `_`, in lowercase, and appends `_N`. Every character that is not a letter or a digit splits words. The issue `[SDK] Add rate-limit: per tenant` #45 gives `sdk_add_rate_limit_45`, and a title with no words gives `issue_45`. Edit the name, or press `Enter` to keep it. Jam Board creates the worktree as `b` does on the PRs page. It writes the issue title and link to `.nogit/issue.md` in the worktree, then opens the worktree in its own window.

Then the pop-up lists each repo in `repos` by its local `org/repo`, with its GitHub `owner/name` when the two differ. `Enter` on a repo loads its GitHub page in the tab.

The pop-up finds the repo of a PR or an issue by the repo's GitHub name. The local `org/repo` of a checkout comes from its path: `~/Sources/akka/runtime/main` gives `akka/runtime`. When the repo has another name on GitHub, set `github` to map each local `org/repo` to its GitHub `owner/name`. If `akka/runtime` lives on GitHub as `lightbend/akka-runtime`, its PR and issue pages show no action row until you add this entry:

```json
{ "github": { "akka/runtime": "lightbend/akka-runtime" } }
```

A repo missing from `github` uses its local `org/repo` as its GitHub name. Jam Board ignores a key that names no repo in `repos`. Each key and each value must be two non-empty parts split by one `/`. A bad entry is a config error: Jam Board lists no repos and shows the error. `jam-factory-server` reads `github` on each request, as it reads `repos`.

The Go menu of Jam Board holds the board-wide keys: ⇧⌘B, ⇧⌘W, ⇧⌘T, ⇧⌘G, ⌥⌘[, ⌥⌘], ⌘N and ⌘0. The menu gets these keys before the focused view does, so they work while the GitHub tab has focus. Every other key goes to the focused view.

To open a worktree from a terminal, run `jb` in it, or pass its path or a folder inside it. `jb` opens the worktree in its own Jam Board window, or focuses the window when it is open. It starts Jam Board when it is not running. The worktree must be a linked worktree of a checkout in `repos`:

```sh
jb ~/Sources/octonato/jam-factory/rate-limit
```

To remove a worktree, press `d` on its worktree page. Confirm in the dialog. Jam Board runs `git worktree remove`, which refuses a worktree with modified or untracked files. It then deletes the branch when it is merged, and closes the worktree's terminals. Jam Board then returns to the GitHub tab. A standalone window closes.

Restart `jam-factory-server` after you change `root` or `port`:

```sh
launchctl kickstart -k "gui/$(id -u)/io.github.octonato.jam-factory.server"
```

Each session lives in `<root>/<org>/<repo>/sessions/<branch>/<session id>/`. `<repo>` is the name of the worktree's parent folder, and `<org>` is the name of the folder above it. For the worktree `~/Sources/my-team/sdk/rate-limit`, `<org>` is `my-team` and `<repo>` is `sdk`. `<branch>` is the last segment of the branch name: `octonato/rate-limit` gives `rate-limit`.

A branch holds one folder per session. The session id is `<category>-<YYYYMMDD-HHMM>-<short sha>`. It uses the UTC minute and HEAD at the start, such as `review-20261104-0905-3f00486`. The category tells the sessions of a branch apart, and Jam Board shows it:

- `session`: a build or vibe session of `/jam:take` or `/jam:just-vibe`.
- `review`: a review session of `/jam:review`.
- `walk`: a walk of `/jam:walk-through`.

A branch holds at most one live `session` and one live `review`, and any number of walks. So a review and a walk can run on the same branch at once, from two agent sessions. Archive a session on Jam Board to start a new one of its category.

The `jam-factory-mcp` server moves sessions of the older `<root>/<org>/<repo>/sessions/<session id>/` layout into their branch folder when it starts. A moved session's id ends with `-migrated`.

The factory shares this folder across orgs, repositories and worktrees. Create a feature branch before starting a session:

```sh
git switch -c my-feature
```

Then invoke the factory with a description: `/jam:take Add rate limiting` in Claude Code or Antigravity, `$jam:take Add rate limiting` in Codex, or `/take Add rate limiting` in Gemini.

## Factory workflow: `/jam:take`

`/jam:take` is the main command in Jam Factory. The examples here use Claude Code and Antigravity syntax (`/jam:take`); use `$jam:take` in Codex or `/take` in Gemini. Complete the [factory setup](#factory-setup) before starting.

`/jam:take` takes one change from spec to a branch ready for review. Each session belongs to one feature branch, in the main checkout or in a worktree. A branch holds one live session at a time. You create the branch; the factory never does. It refuses to run on the base branch.

```
spec ─► plan ─► implement ─► verify(N) ─► triage(N) ─┬─► implement ─► verify(N+1) ...
                                                     └─► finalize ─► done
```

- **spec** and **plan**: the factory writes `spec.md` and `plan.md` to the session folder. It brainstorms the spec with you through `superpowers:brainstorming` when the mode asks for it. It writes the plan with `superpowers:writing-plans`.
- **implement**: a fresh `jam:implementer` agent makes each task's change. The factory then commits it. Every commit is a Conventional Commit. In each git repository, the session's first commit describes the whole session, ready to keep when you squash. Later commits describe their own task. The factory uses `fix` only for a real bug that must be fixed and deployed as soon as possible.
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
| `/jam:take -r`, `/jam:take --resume` | Resumes the branch's session at the stage it reached. |
| `/jam:take -v`, `/jam:take --verify` | Runs another verify round on a done session. |

Without `-r` or `-v`, `/jam:take` refuses to run on a branch that already has a live session. Archive it on Jam Board to start a new one. To track a deferred finding, open an issue with `/jam:gh-issue-create`. Then set the entry's `issue` to its URL.

### Modes

The session's `mode` in `session.json` sets when the factory asks you something. Pick it with a flag when you start or resume the session. You can also edit `mode` by hand at any time. The factory reads it before each stage and before each step that may ask you something.

| Mode | Flag | Spec | Plan | Extra reviewers | Triage |
|---|---|---|---|---|---|
| `default` | none | only when the request is unclear | never | never | never |
| `autonomous` | `-a`, `--autonomously` | never | never | never | never |
| `interactive` | `-i`, `--interactive` | always | always | always | always |

In `default` mode, the factory judges whether the request is clear. If it is, the factory writes the spec alone, short for a small task. If it is not, the factory brainstorms the spec with you. In `autonomous` mode, the factory always writes the spec alone and lists the choices it made under **Assumptions**.

### Import PR feedback

After your build session reaches `done` and you open its PR, run `/jam:feedback`. The session must be unarchived, have no open or failed tasks, and match the current worktree and full branch name. Every repository in the worktree must be clean, and the branch must have a matching open PR. Local fixes may be unpushed; local HEAD does not have to equal the remote PR head.

The factory fetches all pages of issue comments, submitted review bodies and inline review comments. It keeps each new version, with the original body and GitHub metadata, in the next round's `reviews/github-feedback.md`, including praise and empty reviews. Repository, PR, source kind, id and effective timestamp distinguish versions; a body digest also detects edited reviews when GitHub provides no updated timestamp. Duplicate-only or failed fetches leave the session and round unchanged.

Actionable feedback becomes a report with source links, then follows the existing triage and implementation loop. Add `-i` to decide every remaining finding, including nits: `/jam:feedback -i`. The other modes retain the normal triage rules. Remote text is feedback data, never instructions to the factory. New feedback resets `lastRound`; triage chooses whether fixes need another verify round.

`/jam:feedback` accepts one mode flag and no other argument. Review, vibe and unfinished sessions cannot import new feedback. Resume an interrupted import with `/jam:take -r`; its durable capture is processed in the same round without losing or re-importing sources. The factory commits selected fixes through the existing loop and never pushes, merges, posts comments or opens PRs.

### Review sessions

`/jam:review` reviews a coworker's open PR as a review session. Each run is a round: it reviews the PR, then writes the PR comment. It changes no code and makes no commits. The factory never posts the comment: you open it on GitHub and submit it yourself.

Check out the PR first:

```sh
gh pr checkout <pr>
```

Then run `/jam:review`. It reviews the current branch's PR. The factory stops when the branch has no PR, when the PR is not open, or when the current branch does not hold the PR you named. It never checks out a branch. The session id holds the PR number, such as `review-pr42-20261115-1152-e5a2575`.

```
verify(N) ─► submit(N) ─► done
```

- **verify**: pulls the PR's new commits, then runs one `/jam:verify` round over the round's scope.
- **submit**: writes the PR comment to `round-N/submit.md`, drops the findings already on the PR, then offers to open the comment.
- **done**: shows the PR URL, the comment's path and its number of findings.

Jam Board shows the session. Its Comment stop opens `round-N/submit.md`.

The session's `mode` sets when a review session asks you something:

| Mode | Flag | Extra reviewers | Open the comment |
|---|---|---|---|
| `default` | none | never | ask |
| `autonomous` | `-a`, `--autonomously` | never | never |
| `interactive` | `-i`, `--interactive` | always | ask |

The first round reviews the whole PR. After the coworker pushes new commits, run `/jam:review` again. The factory pulls the commits and reviews only the commits since the last round. After a rebase, or after a merge such as GitHub's **Update branch**, it reviews the whole PR again and tells you why. Each comment leaves out the findings that PR comments already report. If the PR is closed or merged, the session blocks.

| Call | What it does |
|---|---|
| `/jam:review` | Starts a review session for the current branch's PR, or runs its next round. |
| `/jam:review <PR number or URL>` | Same, after it checks that the current branch holds that PR. |
| `/jam:review <commit hash>` | Runs a round over `<hash>^..HEAD`. The hash is inclusive and must be a commit of the PR. |
| `-a` or `-i` on any call | Sets the session's mode to `autonomous` or `interactive`. |

When a round stopped before `done`, `/jam:review` resumes it. A commit hash then stops the command: finish the round first. A commit hash still works when the last run stopped before its round started, such as on a dirty worktree. A hash outside the PR stops the command before it changes the session. `/jam:review` takes no range or description. Run `/jam:verify` to review your own changes.

### Vibe sessions

A vibe session runs a quick task with no spec, no plan and no review. Start it with `/jam:just-vibe` on a feature branch. The factory may ask you to clarify the request. Then it does the work itself, with no agent, and commits it. It refuses to run on the base branch, and never merges or pushes.

```
implement ─► done
```

- **implement**: the factory clarifies the request when the mode asks for it. It makes the change, then runs the build, the tests and the formatter check. In each git repository the work changed, it makes one Conventional Commit.
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

Without `-r`, `/jam:just-vibe` refuses to run on a branch that already has a live session. Archive it on Jam Board to start a new one. `/jam:take -r` also resumes a vibe session. `-v` does not apply: a vibe session has no verify round, so run `/jam:verify` to review the branch.

### Walk-through sessions

`/jam:walk-through` saves each walk as a walk-through session, with `flow: walk` in `session.json`. It runs on any branch, the base branch included. Each run creates a new session under the current branch, with an id such as `walk-20261104-0912-3f00486`. It refuses to run on a detached HEAD. `session.json` holds the walk's slug: `pr-<n>`, `commit-<short sha>` or a short name for the code path. `/jam:walk-replay <slug>` replays the newest walk with that slug.

```
walk ─► done
```

- **walk**: the skill maps the route, then writes the steps one at a time. Jam Board shows a card for each step in `walk.json`. A card opens its step as soon as the step is written.
- **done**: the skill prints the session and the step titles. Read the walk on Jam Board, or replay it in the terminal with `/jam:walk-replay <session id>`.

On an error it cannot resolve, the skill sets the stage to `blocked`, with the reason in `session.json`. A walk has no resume: run `/jam:walk-through` again.

A step in changes mode holds git diffs. A step in code-path mode holds lines of code. The `jam-factory-mcp` server copies both from git and the worktree. The model never types them. Jam Board highlights both. Each diff has its own Unified / Split toggle, with Unified as the default.

`-i` walks you through the steps in the terminal instead. It writes no files and creates no session.

### Files

```
~/.jam-factory/
  config.json                    optional: { "root": "<root>", "port": <port>, "repos": ["<path>", ...], "githubUser": "<login>", "githubSignInHosts": ["<host>", ...], "github": { "<org/repo>": "<owner/name>", ... } }
<root>/                          ~/.jam-factory/factory by default
  <org>/<repo>/sessions/<branch>/<session id>/
    session.json                 the session's mode, stage, round and counts
    request.md                   the description, issue or PR the session started from
    spec.md, plan.md, tasks.md
    decisions.md                 dismissed findings, never shown again
    deferred.md                  deferred findings, with their issue URLs
    runs.jsonl                   one line per agent run and per stage
    feedback-pending.md          staging checkpoint during a feedback import
    round-<N>/
      reviews/                   one file per reviewer; github-feedback.md for imported feedback
      report.md
      extras.md                  extra reviewer candidates and your pick
      triage.md
      submit.md                  the PR comment, review sessions only
```

The `jam-factory-mcp` server writes every file under `<root>`. A vibe session writes only `session.json`, `request.md`, `tasks.md` and `runs.jsonl`. Its `request.md` may end with two sections. `## Clarifications` holds the questions the factory asked and your answers. `## Assumptions` holds the choices the factory made alone.

A walk-through session writes only these files:

```
<root>/<org>/<repo>/sessions/<branch>/walk-<YYYYMMDD-HHMM>-<short sha>/
  session.json                   the walk's stage and step counts
  request.md                     the PR URL, commit range or code path
  walk.json                      the target, the summary and the steps
  overview.md                    the target, the summary and a note on mechanical churn
  drafts/<NN>.md                 a step while it is being written
  steps/<NN>.md                  a finished step
  runs.jsonl                     one line when the walk starts, one when it ends
```

`walk.json` lists the steps in order, each with its number, title and summary. `<NN>` is the step number in two digits, such as `03`.

### Export a session

To keep a session in the repo, run `jam-factory-mcp export` in the session's worktree:

```sh
jam-factory-mcp export                  # copies to factory/<session id>/
jam-factory-mcp export docs/rate-limit  # copies to docs/rate-limit/
```

It finds the session the way the skills do: `<org>` and `<repo>` from the folder names, then the branch's live `session`. It copies every session file except dot files, such as `.lock`. The default destination is under the worktree root. A relative destination is relative to the current folder. It fails and copies nothing on a detached HEAD, when the session does not exist, when the destination exists or when the factory root cannot be resolved. It never stages or commits: review the files, then commit them yourself.

Export after the last verify or `-f` round. Files exported earlier would sit in the worktree, where the reviewers of later rounds can read them.

## Commands

The examples below use Claude Code and Antigravity syntax: `/jam:take`, `/jam:verify`, and so on. In Codex, use `$jam:take` and `$jam:verify`; in Gemini, use `/take` and `/verify`. The same arguments and flags apply.

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
| `/jam:walk-replay` | Replay a walk-through session from the factory in the terminal. Prints the overview, then prints one step, verbatim, each time the user says "go ahead". It does not summarize the steps. |

## Skills

| Skill | Description |
|---|---|
| `/jam:take` | Runs one session through the factory: spec, plan, implement, verify and triage. The `jam-factory` MCP server keeps its session files. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue to start the branch's session. Pass nothing to start it from the conversation. Add `-r` to resume the session, or `-v` to run another verify round on a done session. It runs on its own and asks only when the request is unclear. Add `-a` to never ask, or `-i` to approve every step. It never merges or pushes. |
| `/jam:feedback` | Imports new feedback from the branch's open GitHub PR into its done build session, as a new round. Actionable feedback becomes a report, then goes through triage and the fix loop. Add `-a` to never ask, or `-i` to decide every remaining finding, including nits. Resume an interrupted import with `/jam:take -r`. It never pushes, merges or posts comments. |
| `/jam:just-vibe` | Runs a quick task through the factory as a vibe session: no spec, no plan, no review. The `jam-factory` MCP server keeps its session files. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue. Pass nothing to start from the conversation. It asks for clarification only when the request is unclear. Add `-a` to never ask, or `-i` to confirm the request first. Add `-r` to resume the session. It commits the work and never merges or pushes. |
| `/jam:verify` | Read-only review of a PR, a commit range or the current branch. Under 300 changed lines, it runs jam:independent-reviewer or superpowers:requesting-code-review, picked at random. From 300 lines, it runs both. Then it offers extra reviewers suited to the diff (from `pr-review-toolkit` in Claude Code). It checks each finding against the code. It writes one report to .nogit/verify-{timestamp}/, or to the directory --out names. |
| `/jam:review` | Reviews a coworker's open PR as a factory review session that Jam Board shows. Each run is a round. The first round reviews the whole PR. A later round reviews the commits since the last round, or the whole PR after a rebase. Pass a commit hash to review from that commit. Each round writes the PR comment to round-N/submit.md, drops the findings already on the PR and offers to open the comment on GitHub. It never posts the comment. Add `-a` to never ask, or `-i` to pick extra reviewers. |
| `/jam:independent-reviewer` | Independent, read-only code review of a PR, a commit range or the current branch. A fresh subagent reviews the change and never learns how it was built. It writes the report to .nogit/verify-{timestamp}/independent-review.md. Add --report-in-chat to get the report in chat instead. |
| `/jam:verify-fix` | Fix the findings of a /jam:verify run, each in its own commit. Runs on its own by default; add -i to fix one at a time. Pass --resume verify-{timestamp} to use a run on disk. Otherwise pick a run from a list, or run a new review. Never pushes. |
| `/jam:walk-through` | Walk-through of a code path or a set of changes, broken into logical steps. By default it saves the walk as a factory session. Jam Board shows each step as soon as it is written. Replay the walk in the terminal with `/jam:walk-replay`. Add `-i` to walk through it one step at a time in the terminal instead. `-i` writes no files. |

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

Extra reviewers are optional. Claude Code uses the `pr-review-toolkit` plugin. Codex, Gemini and Antigravity can use general-purpose agents with the same review focus. Verify lists the ones the diff calls for in `extras.md`. For example, a diff that changes tests lists `pr-test-analyzer`. After the report, it asks which extras to run, records your choice in `extras.md`, and rebuilds the report with their reviews.

`/jam:verify-fix` runs the base reviewers only.

| Call | What it does |
|---|---|
| `/jam:verify [target]` | Reviews and writes a report. Changes no code. |
| `/jam:verify --out <dir> [target]` | Same, with the run directory set to `<dir>`. |
| `/jam:review [PR number, URL or commit hash]` | Reviews a coworker's open PR as a review session, and writes the PR comment to `round-N/submit.md`. See [Review sessions](#review-sessions). |
| `/jam:verify-fix [target]` | Lists the three newest runs on disk. Pick one, or run a new review of the target. Then fixes and commits each finding without stopping. It asks its questions at the end. It never pushes. |
| `/jam:verify-fix -i [target]` | Goes through the findings one at a time. For each one, it gives its assessment and waits for your decision. Then it shows the fix with its commit message, and commits it when you approve. |
| `/jam:verify-fix --resume verify-{timestamp}` | Fixes the findings of that run. Continues its fixes if the run already has them. |

The target is a PR number or URL, a commit hash, a range `a..b`, or nothing for the current branch. A commit hash is inclusive.

Each run writes to its own `.nogit/verify-{timestamp}/` directory. No reviewer reads `.nogit/`. So reviewers never see each other's findings, and runs never see each other's reports. Start each run in a new session.

## Local storage

The plugin keeps its files in `.nogit/`, in the current working directory:

- `.nogit/*.md`: summaries saved with `/jam:save`
- `.nogit/verify-{timestamp}/`: one review run, holding the report, one file per reviewer in `reviews/`, the PR comment and the verify-fix state
- `.nogit/bg/`: output of background agents started with `/jam:subagent`

These files are for you, not for the project. Add `.nogit/` to your global gitignore so no project commits them:

1. Find the file with `git config --global core.excludesFile`. If it prints nothing, Git uses `~/.config/git/ignore`.
2. Add the line `.nogit/` to that file.

## Try a branch's board

`./try-branch` runs a branch's `jam-factory-server` and Jam Board beside the installed ones. Run it at the root of the branch's worktree:

```sh
./try-branch              # port 8732
./try-branch --port 8740
```

It builds both in release mode in the worktree's `app/target`. It leaves `/Applications` and `~/.local/bin` untouched. It starts the branch server with `JAM_FACTORY_PORT` set to the port, and writes the server log to `.nogit/try-branch/server.log`. Once the server answers, it runs the branch board in the foreground on the same port. It stops the branch server when the board quits or on Ctrl+C.

The branch pair shares these with the installed board:

- The factory sessions and the `repos` in `~/.jam-factory/config.json`. An edit in one board shows up in the other.
- The GitHub sign-in cookies.
- The installed `jam-factory-mcp`. The branch server runs the one in `~/.local/bin`, so it does not test MCP changes of the branch.
- `jam-board://` links and `jb`. They keep opening the installed app.

Each server runs its own terminals.

## Release

`just release` cuts a release of Jam Board. Run it at the repository root on a clean worktree:

```sh
just release              # bumps the patch version
just release minor
just release major
just release 0.2.0
```

It sets the new version in `app/Cargo.toml` and runs the gates. It commits `rel: vX.Y.Z` and tags `vX.Y.Z`. Then it installs Jam Board as `just install` does, and offers to push the commit and the tag. When the gates fail, it restores `app/Cargo.toml` and `app/Cargo.lock` and stops before the commit. When the install fails, the commit and the tag stay. Fix the cause, run `just install`, then push the branch and the tag `vX.Y.Z`.

The board footer shows the build version. A build of a clean tagged commit shows `vX.Y.Z`. Any other build shows `vX.Y.Z-{8 characters of the commit}`, including `./try-branch` and a build with uncommitted changes. There, `X.Y.Z` is the highest `vX.Y.Z` tag reachable from the commit, or the version in `app/Cargo.toml` when no such tag exists.
