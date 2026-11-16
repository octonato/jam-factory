---
name: just-vibe
description: Run a quick task through the Jam Factory as a vibe session — no spec, no plan, no review — tracked in files that the jam-factory MCP server keeps. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue. Pass nothing to start from the conversation. It asks for clarification only when the request is unclear. Add -a/--autonomously to never ask, or -i/--interactive to confirm the request first. Add -r/--resume to resume the session. It commits the work and never merges or pushes.
allowed-tools: Bash(echo *), Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *), mcp__jam-factory
---

Run a quick task through the factory, as a vibe session.

What I gave you: the arguments supplied when invoking this skill

Run `git rev-parse --show-toplevel` to get the worktree.

Run `git branch --show-current` to get the current branch.

Run `git rev-parse --short HEAD` to get HEAD.

Read the plugin root's `skills/take/state.md` before you start. It holds the tools and the format of every file this skill writes.

## Where things are

- **Worktree**: the value above. It is the main checkout or a linked worktree.
- **Git repositories**: the worktree, plus each folder below it that holds a `.git` entry. A project can hold subprojects with their own git repositories. Every `git status`, restore and commit in this skill runs in each git repository.
- **Branch**: the last segment of the current branch. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Org**: the name of the folder above the repo folder. For the worktree `~/Sources/octonato/jam-factory/rate-limit`, the org is `octonato` and the repo is `jam-factory`.
- **Session id**: the id of the branch's live session, such as `session-20261104-0912-3f00486`. **Pick the command** finds it, or `start_session` returns it.
- **Session**: the session named by `org`, `repo`, `branch` and `session id`. The tools of the `jam-factory` MCP server read and write its files, as `state.md` says. Every file path in this skill, such as `request.md`, is relative to the session.

If the `jam-factory` tools are missing, tell me to run `just install` in the `app/` folder of the jam-factory repository. Then stop.

After this skill creates `session.json`, each change to it, such as "set `stage: done`", is an `update_session` call.

## Modes

The session's `mode` in `session.json` sets when the factory asks me something:

| Mode | Clarify |
|---|---|
| `default` | only when the request is unclear |
| `autonomous` | never |
| `interactive` | always |

I may edit `mode` by hand at any time. The **Modes** section of the plugin root's `skills/take/vibe.md` holds the details.

## Pick the command

If the current branch is empty, HEAD is detached. Tell me to switch to a feature branch and stop.

Take the flags out of what I gave you:

- `-a` or `--autonomously`: the mode is `autonomous`.
- `-i` or `--interactive`: the mode is `interactive`.
- `-r` or `--resume`: resume the session.

If I passed both mode flags, tell me they conflict and stop. The rest is the argument.

Find the session: call `list_sessions` with `org`, `repo`, `branch` and `category: session`. When it lists a session, use its `session` value as the session id. The session holds `session.json`. When it lists none, the session holds no `session.json`.

- **The session holds `session.json` with `archived: true`**: tell me the session is archived and stop.
- **The session holds `session.json`, and I passed `-r`**: if its `flow` is not `vibe`, tell me to resume it with `$jam:take -r` and stop. Otherwise go to **Resume**.
- **The session holds `session.json`**: tell me the branch already has the live session `{session id}`. Tell me to pass `-r` to resume it, or to archive it on Jam Board to start a new one. Stop.
- **I passed `-r`**: tell me the branch has no live session. Stop.
- **The argument is a PR URL**: tell me that `$jam:review` reviews PRs. It takes a PR number or URL, or reviews the current branch's PR when I pass no argument. Stop. A PR URL holds `/pull/`.
- **Anything else**: read the plugin root's `skills/take/vibe.md` and go to its **New vibe session** section.

When you go to **Resume** and the argument is not empty, tell me the session already exists and ignore the argument.

## Resume

Read `session.json` with `read_file`. If I passed a mode flag, set `mode` to it.

Tell me the session id, the mode and the stage in one line.

Read the plugin root's `skills/take/vibe.md`. Go to its section for the `stage`: `implement`, `done` or `blocked`.

## Usage

Show this list when a step says to show the usage:

- `$jam:just-vibe <description | issue>`: run a quick task from a description or a GitHub issue.
- `$jam:just-vibe`: run a quick task from what we discussed in this conversation.
- `$jam:just-vibe -r`: resume the branch's vibe session.
- `-a` or `-i` on any call: set the session's mode to `autonomous` or `interactive`.

## Rules

- You never merge, push or open a PR.
- Read and write the session's files only with the `jam-factory` tools, as `state.md` says.
- Update `session.json` with `update_session` on every stage change.
- Ask me only where the session's mode allows it. A blocked session is the one exception.
- Keep me posted with one short line per stage change. For example: `fix-typo: implement done, 1 commit → done`.
