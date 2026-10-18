---
name: just-vibe
disable-model-invocation: true
description: Run a quick task through the Jam Factory as a vibe session — no spec, no plan, no review — tracked in files under $FACTORY_ROOT. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue. Pass nothing to start from the conversation. It asks for clarification only when the request is unclear. Add -a/--autonomously to never ask, or -i/--interactive to confirm the request first. Add -r/--resume to resume the session. It commits the work and never merges or pushes.
argument-hint: [-a|--autonomously | -i|--interactive] [-r|--resume] [description | issue number or URL]
allowed-tools: Bash(echo *), Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *)
effort: xhigh
---

Run a quick task through the factory, as a vibe session.

What I gave you: $ARGUMENTS

Factory root: !`echo "${FACTORY_ROOT:-unset}"`

Worktree: !`git rev-parse --show-toplevel`

Current branch: !`git branch --show-current`

Read `${CLAUDE_PLUGIN_ROOT}/skills/take/state.md` before you start. It holds the format of every file this skill writes.

## Where things are

- **Worktree**: the value above. It is the main checkout or a linked worktree.
- **Git repositories**: the worktree, plus each folder below it that holds a `.git` entry. A project can hold subprojects with their own git repositories. Every `git status`, restore and commit in this skill runs in each git repository.
- **Factory root**: the value above. If it is `unset`, tell me to export `FACTORY_ROOT` as `<org>/.factory` in my shell alias. Also tell me to start Claude with `--add-dir "$FACTORY_ROOT"`. Then stop. Never guess the path.
- **Session id**: the last segment of the current branch. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Session folder**: `{factory root}/{repo}/sessions/{session id}/`.

## Modes

The session's `mode` in `session.json` sets when the factory asks me something:

| Mode | Clarify |
|---|---|
| `default` | only when the request is unclear |
| `autonomous` | never |
| `interactive` | always |

I may edit `mode` by hand at any time. The **Modes** section of `${CLAUDE_PLUGIN_ROOT}/skills/take/vibe.md` holds the details.

## Pick the command

If the current branch is empty, HEAD is detached. Tell me to switch to a feature branch and stop.

Take the flags out of what I gave you:

- `-a` or `--autonomously`: the mode is `autonomous`.
- `-i` or `--interactive`: the mode is `interactive`.
- `-r` or `--resume`: resume the session.

If I passed both mode flags, tell me they conflict and stop. The rest is the argument.

- **The session folder holds `session.json` with `archived: true`**: tell me the session is archived and stop.
- **The session folder holds `session.json`, and I passed `-r`**: if its `flow` is not `vibe`, tell me to resume it with `/jam:take -r` and stop. Otherwise go to **Resume**.
- **The session folder holds `session.json`**: tell me the session already exists, and to pass `-r` to resume it. Stop.
- **I passed `-r`**: tell me the branch has no session. Stop.
- **The argument is a PR URL**: tell me that `/jam:take` reviews PRs. Stop. A PR URL holds `/pull/`.
- **Anything else**: read `${CLAUDE_PLUGIN_ROOT}/skills/take/vibe.md` and go to its **New vibe session** section.

When you go to **Resume** and the argument is not empty, tell me the session already exists and ignore the argument.

## Resume

Read `session.json`. If I passed a mode flag, set `mode` to it.

Tell me the session id, the mode and the stage in one line.

Read `${CLAUDE_PLUGIN_ROOT}/skills/take/vibe.md`. Go to its section for the `stage`: `implement`, `done` or `blocked`.

## Usage

Show this list when a step says to show the usage:

- `/jam:just-vibe <description | issue>`: run a quick task from a description or a GitHub issue.
- `/jam:just-vibe`: run a quick task from what we discussed in this conversation.
- `/jam:just-vibe -r`: resume the branch's vibe session.
- `-a` or `-i` on any call: set the session's mode to `autonomous` or `interactive`.

## Rules

- You never merge, push or open a PR.
- Update `session.json` on every stage change, as `state.md` says.
- Ask me only where the session's mode allows it. A blocked session is the one exception.
- Keep me posted with one short line per stage change. For example: `fix-typo: implement done, 1 commit → done`.
