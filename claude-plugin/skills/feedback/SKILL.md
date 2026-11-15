---
name: feedback
disable-model-invocation: true
description: Import new feedback from the branch's open GitHub PR into its done build session, as a new factory round. Actionable feedback becomes a report, then goes through triage and the fix loop. Add -a/--autonomously to never ask, or -i/--interactive to decide every remaining finding, including nits. Resume an interrupted import with /jam:take -r. Never pushes, merges or posts comments.
argument-hint: "[-a|--autonomously | -i|--interactive]"
allowed-tools: Bash(echo *), Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *), mcp__plugin_jam_jam-factory
effort: high
---

Import new PR feedback into the branch's done build session.

What I gave you: $ARGUMENTS

Worktree: !`git rev-parse --show-toplevel`

Current branch: !`git branch --show-current`

HEAD: !`git rev-parse --short HEAD`

Read `${CLAUDE_PLUGIN_ROOT}/skills/take/state.md` before you start. It holds the tools and the format of every file this skill writes.

## Where things are

- **Worktree**: the value above. It is the main checkout or a linked worktree.
- **Git repositories**: the worktree, plus each folder below it that holds a `.git` entry. A project can hold subprojects with their own git repositories. Every `git status`, restore and commit in this skill runs in each git repository.
- **Branch**: the last segment of the current branch. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Org**: the name of the folder above the repo folder. For the worktree `~/Sources/octonato/jam-factory/rate-limit`, the org is `octonato` and the repo is `jam-factory`.
- **Session id**: the id of the branch's live session, such as `session-20261104-0912-3f00486`. **Find the session** finds it.
- **Session**: the session named by `org`, `repo`, `branch` and `session id`. The tools of the `jam-factory` MCP server read and write its files, as `state.md` says. Every file path in this skill, such as `feedback-pending.md` or `round-{round}/report.md`, is relative to the session.

If the `jam-factory` tools are missing, tell me to run `just install` in the `app/` folder of the jam-factory repository. Then stop.

Each change to `session.json`, such as "set `stage: implement`", is an `update_session` call.

Run every agent in this skill with `effort: "xhigh"`.

## Modes

The session's `mode` in `session.json` sets when the factory asks me something. The **Modes** table of `${CLAUDE_PLUGIN_ROOT}/skills/take/SKILL.md` lists when. In `interactive` mode, triage asks about every remaining finding, including nits.

I may edit `mode` by hand at any time. Re-read `session.json` before each stage and before each step that may ask me something. Follow the mode it holds at that moment.

## Arguments

Take the flags out of what I gave you:

- `-a` or `--autonomously`: the mode is `autonomous`.
- `-i` or `--interactive`: the mode is `interactive`.

If I passed both mode flags, tell me they conflict and stop.

If I passed any other flag or any other word, a number or `#n` included, tell me that `/jam:feedback` takes only a mode flag. Show the usage and stop. A number is not a PR selector: the import finds the branch's open PR.

Reject these before changing any file or mode.

## Find the session

Take these steps in order:

1. If the current branch is empty, HEAD is detached. Tell me to switch to the session's branch and stop.
2. Call `list_sessions` with `org`, `repo`, `branch` and `category: session`. If it lists none, tell me the branch has no live session, and that `/jam:take` starts one. Stop. Otherwise use its `session` value as the session id.
3. Read `session.json` with `read_file`, and list the session's files with `list_files`. If `archived` is `true`, tell me the session is archived and stop.
4. If the session holds a pending feedback checkpoint, tell me to run `/jam:take -r` and stop. A pending checkpoint is a `feedbackImport` that is not null, or `list_files` listing `feedback-pending.md` or `round-{round + 1}/reviews/github-feedback.md`.
5. Otherwise read `${CLAUDE_PLUGIN_ROOT}/skills/take/feedback.md` and follow **New import**. It continues through **Capture and attach**, **Normalize** and **Triage imported feedback**. Do not change mode or any session file before its preflight and fetch succeed. Pass it the mode flag I passed, if any, as the requested mode flag. Only **Capture and attach** sets `mode` from it.

## After triage

**Triage imported feedback** returns a fresh `started` timestamp and the triage outcome. Then read `${CLAUDE_PLUGIN_ROOT}/skills/take/SKILL.md` and go to its **Triage** section.

Triage already ran. Skip the first paragraph of that section, and start at "Take `ended` after triage returns". Apply the outcome as it says: update the counts, clear `feedbackImport` and append exactly one `triage` line, with the returned `started`.

Then follow the outcome into its **Implement**, **Verify**, **Triage** and **Done** sections, with this session.

## Usage

Show this list when a step says to show the usage:

- `/jam:feedback`: import new feedback from the branch's open PR into its done build session.
- `/jam:feedback -i`: the same, and decide every remaining finding, including nits.
- `/jam:feedback -a`: the same, and never ask.
- `/jam:take -r`: resume an interrupted import.

## Rules

- You never merge, push, open a PR or post a comment on the PR.
- Read and write the session's files only with the `jam-factory` tools, as `state.md` says.
- Update `session.json` with `update_session` on every stage change.
- Append a line to `runs.jsonl` with `append_file`, as `state.md` defines it: one per agent run, and one per main-conversation stage. For an agent run, take `tokens`, `toolUses` and `durationMs` from the `totalTokens`, `toolUses` and `durationMs` the **Agent** tool reports when the agent finishes.
- Ask me only where the session's mode allows it. A blocked session is the one exception.
- Keep me posted with one short line per stage change. For example: `feature-1: round 3 feedback imported, 2 must-fix, 1 should-fix → triage`.
