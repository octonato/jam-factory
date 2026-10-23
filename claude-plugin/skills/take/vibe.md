# Factory vibe session

This procedure runs a vibe session. `skills/just-vibe/SKILL.md` and `skills/take/SKILL.md` route here. The formats are in `state.md`.

## Modes

The session's `mode` in `session.json` sets when a vibe session asks me something:

| Mode | Clarify |
|---|---|
| `default` | only when the request is unclear |
| `autonomous` | never |
| `interactive` | always |

Re-read `session.json` before each stage and before each step that may ask me something. Follow the mode it holds at that moment.

## New vibe session

`SKILL.md` already checked that the session folder holds no `session.json`.

1. Find the base, the branch the session merges into. Take the first that works:
   1. The remote's default branch: `git symbolic-ref --short refs/remotes/{remote}/HEAD`, with the remote prefix dropped. The remote is `upstream` when `git remote` lists it, otherwise `origin`.
   2. `main`, when `git rev-parse --verify --quiet main` finds it.
2. If none works, ask me for the base. Otherwise tell me the base in one line, and don't ask.
3. If the current branch is the base, tell me to create a feature branch and run the skill there. Stop. The factory never creates branches.
4. If the argument is empty and we have not discussed what to build in this conversation, show the **Usage** list of `${CLAUDE_PLUGIN_ROOT}/skills/just-vibe/SKILL.md` and stop.
5. Read the request:
   - **An empty argument**: the request is what we discussed in this conversation. Write the goal, the decisions we made and the constraints. A reader who never saw the conversation must be able to act on it.
   - **An issue number, `#n` or an issue URL**: run `gh issue view {issue} --json number,title,body,url`. The request is the issue's title, URL and body.
   - **Anything else**: the request is the argument, verbatim.
6. Write `{session folder}/request.md` with the request, as `state.md` shows.
7. Write `{session folder}/tasks.md`, as `state.md` shows, with one task `T1`. Its title names the request in a few words. Set `source: request`, `status: open` and `commit: none`. Its text points to `request.md`.
8. Write `{session folder}/session.json` with this worktree, the current branch and the base. Set `mode` from the flag, or `default` without one. Set `flow: vibe`, `stage: implement`, `round: 0`, `lastRound: false`, `clarified: false`, `tasksOpen: 1`, every other count `0`, `blocked: null` and `archived: false`. The Write tool creates the folders.
9. Append a `request` line to `runs.jsonl` with `agent: "main"`. Set its `started` and `ended` to the current time.
10. Go to **Implement**.

## Implement

1. If `T1` in `tasks.md` is `done`, set `stage: done` and go to **Done**. If it is `failed`, set it back to `open`.
2. Run `git status --porcelain` in each git repository. If it prints anything, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "dirty worktree" }`. List the files and stop.
3. **Clarify.** Follow the mode:
   - **`default`**: skip this step when `session.json` holds `clarified: true`. Otherwise judge whether the request is clear. A request is clear when it has one reading, a bounded scope and a checkable result. Tell me your judgement in one line. If it is unclear, ask me until it is clear.
   - **`autonomous`**: ask me nothing.
   - **`interactive`**: restate the request and your approach in a few lines, with the `## Clarifications` and `## Assumptions` that `request.md` holds. Ask me to confirm or correct them, until I confirm.

   In `default` and `interactive` mode, append any answers I gave under `## Clarifications` in `request.md`, as `state.md` shows. Then set `clarified: true` in `session.json`.
4. **Do the work** in this conversation, with no agent. Take the `started` time for `runs.jsonl` first. Follow the **Implement**, **Test** and **Never** sections of `${CLAUDE_PLUGIN_ROOT}/agents/implementer.md`. The task is the request in `request.md`, with my answers. When the work hits a choice the request does not settle, follow the mode:
   - **`default` and `interactive`**: ask me.
   - **`autonomous`**: choose alone.
5. Append the answers I gave during the work under `## Clarifications` and the choices you made alone under `## Assumptions` in `request.md`, as `state.md` shows.
6. Append an `implement` line to `runs.jsonl` with `agent: "main"` and `task: "T1"`. Set `outcome: "failed"` when you cannot finish the work or make the checks pass, and `outcome: "done"` otherwise.
7. **On failure**, when you cannot finish the work or make the checks pass:
   1. In each git repository, restore the tracked files with `git restore --staged --worktree .`.
   2. In each git repository, delete only the untracked files the session created, as `git status --porcelain` lists them.
   3. Set `T1` to `status: failed`.
   4. Set `stage: blocked` and `blocked: { "stage": "implement", "reason": "<the failure's reason>" }`. Stop.
8. **Commit** as section **4. Commit** of `${CLAUDE_PLUGIN_ROOT}/commands/autonomously.md` says. The request is the task. In each git repository the work changed, the session makes one commit: phase 0, with a Conventional Commit subject. Take its subject and body from the request and the change. If a commit fails, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "<git's error>" }`. Leave the changes in place. Stop.
9. Record each commit in the `commit` field of `T1`, as `state.md` shows, and set `T1` to `status: done`. Update `tasksOpen` and `tasksDone`. Set `stage: done` and go to **Done**.

## Done

Show me:

- the commits, one line each, from `tasks.md`: short hash and subject
- the assumptions, from `## Assumptions` in `request.md`, when it holds that section
- the session folder

Tell me that `/jam:verify` reviews the branch. Remind me that the branch is ready for me to push, merge or open a PR.

## Blocked

Show me the `blocked` reason. Ask me whether I fixed the cause. On yes, set `stage` to `blocked.stage` and `blocked` to `null`. Then go to that stage.
