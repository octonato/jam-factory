---
name: take
description: Run one session through the Jam Factory — spec, plan, implement, verify and triage — tracked in files that the jam-factory MCP server keeps. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue to start the branch's session. Pass nothing to start it from the conversation. Pass a PR URL, or -p/--pr, to review a coworker's PR as a session. Add -r/--resume to resume the session, or -v/--verify to run another verify round on a done session. Add -f/--feedback to import new GitHub PR feedback into a done build session. It runs on its own and asks only when the request is unclear. Add -a/--autonomously to never ask, or -i/--interactive to approve every step. It never merges or pushes.
allowed-tools: Bash(echo *), Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *), mcp__jam-factory
---

Run my current session through the factory.

What I gave you: the arguments supplied when invoking this skill

Run `git rev-parse --show-toplevel` to get the worktree.

Run `git branch --show-current` to get the current branch.

Read the plugin root's `skills/take/state.md` before you start. It holds the tools and the format of every file this skill writes.

## Where things are

- **Worktree**: the value above. It is the main checkout or a linked worktree.
- **Git repositories**: the worktree, plus each folder below it that holds a `.git` entry. A project can hold subprojects with their own git repositories. Every `git status`, restore and commit in this skill runs in each git repository.
- **Session id**: the last segment of the current branch. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Org**: the name of the folder above the repo folder. For the worktree `~/Sources/octonato/jam-factory/rate-limit`, the org is `octonato` and the repo is `jam-factory`.
- **Session**: the session named by `org`, `repo` and `session id`. The tools of the `jam-factory` MCP server read and write its files, as `state.md` says. Every file path in this skill, such as `spec.md` or `round-{round}/report.md`, is relative to the session.

If the `jam-factory` tools are missing, tell me to run `just install` in the `app/` folder of the jam-factory repository. Then stop.

After this skill creates `session.json`, each change to it, such as "set `stage: plan`", is an `update_session` call.

Run every agent in this skill with `effort: "xhigh"`.

## Modes

The session's `mode` in `session.json` sets when the factory asks me something:

| Mode | Spec | Plan | Extra reviewers | Triage |
|---|---|---|---|---|
| `default` | only when the request is unclear | never | never | never |
| `autonomous` | never | never | never | never |
| `interactive` | always | always | always | always |

I may edit `mode` by hand at any time. Re-read `session.json` before each stage and before each step that may ask me something. Follow the mode it holds at that moment.

## Pick the command

If the current branch is empty, HEAD is detached. Tell me to switch to a feature branch and stop.

Take the flags out of what I gave you:

- `-a` or `--autonomously`: the mode is `autonomous`.
- `-i` or `--interactive`: the mode is `interactive`.
- `-r` or `--resume`: resume the session.
- `-v` or `--verify`: run another verify round on a done session.
- `-f` or `--feedback`: import new GitHub PR feedback into a done build session.
- `-p` or `--pr`, with an optional PR number right after it: start a review session for that PR.

If I passed both mode flags, tell me they conflict and stop. If I passed `-p` with `-r` or `-v`, tell me they conflict and stop. The rest is the argument. If I passed `-f` with `-r`, `-v`, `-p`, a PR selection, any positional argument or an unknown flag, tell me they conflict and stop. Only one mode flag may accompany `-f`. Reject these combinations before changing any file or mode.

Read `session.json` with `read_file`. A `not_found` error means the session holds no `session.json`. When it exists, list the session's files with `list_files`.

- **The session holds `session.json` with `archived: true`**: tell me the session is archived and stop.
- **The session holds `session.json` and a pending feedback checkpoint**: `feedbackImport` is not null, or `list_files` lists `feedback-pending.md` or `round-{round + 1}/reviews/github-feedback.md`. If I passed `-r`, read the plugin root's `skills/take/feedback.md` and follow **Resume import**. Otherwise tell me to run `$jam:take -r` and stop. This check runs before `-v` and the existing-session refusal.
- **The session holds `session.json`, and I passed `-f`**: read the plugin root's `skills/take/feedback.md` and follow **New import**. Do not change mode or any session file before its preflight and fetch succeed.
- **The session holds `session.json`, and I passed `-v`**: If its `flow` is `vibe`, tell me a vibe session has no verify round. Say that `$jam:verify` reviews the branch. Stop. If its `stage` is `done`, set `lastRound: false` and `stage: verify`, then go to **Resume**. Otherwise tell me the session is not done and stop.
- **The session holds `session.json`, and I passed `-r`**: go to **Resume**.
- **The session holds `session.json`**: tell me the session already exists, and to pass `-r` to resume it. Stop.
- **I passed `-r`, `-v` or `-f`**: tell me the branch has no session. Stop.
- **I passed `-p`, or the argument is a PR URL**: read the plugin root's `skills/take/review.md` and go to its **New review session** section. A PR URL holds `/pull/`.
- **Anything else**: go to **New session**.

When you go to **Resume** and the argument is not empty, tell me the session already exists and ignore the argument.

## Usage

Show this list when a step says to show the usage:

- `$jam:take <description | issue>`: start the branch's session from a description or a GitHub issue.
- `$jam:take`: start the branch's session from what we discussed in this conversation.
- `$jam:take <PR URL>` or `$jam:take -p [number]`: start a review session for a coworker's PR. Without a number, `-p` takes the current branch's PR.
- `$jam:take -r`: resume the branch's session.
- `$jam:take -v`: run another verify round on the branch's done session.
- `$jam:take -f` or `$jam:take --feedback`: import new feedback from the branch's open PR into a done build session; add `-i` to decide every remaining finding, including nits. Resume an interrupted import with `-r`.
- `-a` or `-i` on any call: set the session's mode to `autonomous` or `interactive`.

## New session

1. Find the base, the branch the session merges into. Take the first that works:
   1. The remote's default branch: `git symbolic-ref --short refs/remotes/{remote}/HEAD`, with the remote prefix dropped. The remote is `upstream` when `git remote` lists it, otherwise `origin`.
   2. `main`, when `git rev-parse --verify --quiet main` finds it.
2. If none works, ask me for the base. Otherwise tell me the base in one line, and don't ask.
3. If the current branch is the base, tell me to create a feature branch and run the skill there. Stop. The factory never creates branches.
4. If the argument is empty and we have not discussed what to build in this conversation, show the usage and stop.
5. Read the request:
   - **An empty argument**: the request is what we discussed in this conversation. Write the goal, the decisions we made and the constraints. A reader who never saw the conversation must be able to act on it.
   - **An issue number, `#n` or an issue URL**: run `gh issue view {issue} --json number,title,body,url`. The request is the issue's title, URL and body.
   - **Anything else**: the request is the argument, verbatim.
6. Create `request.md` with `write_file`, holding the request, as `state.md` shows.
7. Create `session.json` with `write_file`, holding this worktree, the current branch and the base. Set `mode` from the flag, or `default` without one. Set `flow: build`, `stage: spec`, `round: 0`, `lastRound: false`, every count `0`, `blocked: null` and `archived: false`.
8. Append a `request` line to `runs.jsonl` with `append_file` and `agent: "main"`. Set its `started` and `ended` to the current time. Set `durationMs: null`.
9. Go to **Spec**.

## Resume

Read `session.json` with `read_file`. Before changing mode or dispatching a stage, check the pending feedback checkpoints listed above. If one exists, read the plugin root's `skills/take/feedback.md` and follow **Resume import**. Otherwise, if I passed a mode flag, set `mode` to it.

If its `flow` is `review`, read the plugin root's `skills/take/review.md`. Go to its section for the `stage`: `verify`, `submit`, `done` or `blocked`.

If its `flow` is `vibe`, read the plugin root's `skills/take/vibe.md`. Go to its section for the `stage`: `implement`, `done` or `blocked`.

Otherwise, go to the section for its `stage`:

- `spec`: **Spec**
- `plan`: **Plan**
- `implement`: **Implement**
- `verify`: **Verify**
- `triage`: **Triage**
- `done`: **Done**
- `blocked`: show me the `blocked` reason. Ask me whether I fixed the cause. On yes, set `stage` to `blocked.stage` and `blocked` to `null`. Then go to that stage.

Tell me the session id, the mode, the stage and the round in one line before you go on.

## Spec

Take the `started` time for `runs.jsonl`.

If `list_files` lists `spec.md`:

- **`autonomous`**: accept it.
- **`default` or `interactive`**: show me its name, `factory:{org}/{repo}/{session id}/spec.md`, and ask me to approve it or ask for changes. Apply the changes with `edit_file` until I approve.

Otherwise pick how to write it:

- **`interactive`**: brainstorm it with me.
- **`autonomous`**: write it alone.
- **`default`**: judge whether the request is clear. A request is clear when it has one reading, a bounded scope and a checkable result. If it is clear, write it alone. Otherwise brainstorm it with me. Tell me your judgement in one line.

To brainstorm it with me, load `superpowers:brainstorming` with the Codex skill system, and follow it with these overrides:

- Start from the request in `request.md`. Read it with `read_file`.
- Save the spec as `spec.md` with `write_file`, and change it with `edit_file`. Always write `spec.md`, including when the skill offers a bounded path.
- Show me the spec as `factory:{org}/{repo}/{session id}/spec.md` where the skill shows its path.
- Never commit the spec. Never write under `docs/superpowers/`.
- When I approve the spec, stop following the skill. Do not invoke `writing-plans` from it.

To write it alone, read the request and the code it touches. Ask me nothing. Save `spec.md` with `write_file`, sized to the task. A small, obvious task gets a short spec: the problem, the change and how to check it. List each choice the request left open under **Assumptions**. Never commit the spec.

After I approve the spec, or after you accept or write it, append a `spec` line to `runs.jsonl` with `append_file` and `agent: "main"`. Set `stage: plan` and go to **Plan**.

## Plan

Take the `started` time for `runs.jsonl`.

If `list_files` lists `plan.md`:

- **`interactive`**: show me its name, `factory:{org}/{repo}/{session id}/plan.md`, and ask me to approve it or ask for changes. Apply the changes with `edit_file` until I approve.
- **`default` or `autonomous`**: accept it.

Otherwise load `superpowers:writing-plans` with the Codex skill system, and follow it with these overrides:

- The spec is `spec.md`. Read it with `read_file`.
- Save the plan as `plan.md` with `write_file`, and change it with `edit_file`. Never write under `docs/superpowers/`.
- Never commit the plan.
- Skip its execution handoff. In `interactive` mode, ask me to approve the plan instead. In the other modes, ask me nothing and accept the plan.

When I approve the plan or you accept it, create `tasks.md` with `write_file`. Add one `open` task per `### Task N` heading in the plan, in order. Give each task the heading's title and the source `plan task N`. Set its text to the task's full text. Append a `plan` line to `runs.jsonl` with `append_file` and `agent: "main"`. Set `tasksOpen`, set `stage: implement`, and go to **Implement**.

## Implement

Run `git status --porcelain`. If it prints anything, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "dirty worktree" }`. List the files and stop.

Read `tasks.md` with `read_file`. Set every `failed` task back to `open` with `edit_file`. Then take each `open` task in order. For each one:

### 1. Implement

Take the `started` time for `runs.jsonl`. Run the `jam:implementer` agent with the **collaboration** tools. Pass it only:

- the worktree
- the session's `org`, `repo` and `session`, the session id
- the task's ID, title and text

Pass nothing else from this conversation. Read the last line of its reply.

Append an `implement` line to `runs.jsonl` with `append_file`, `agent: "jam:implementer"`, `round` set to the open round, the task's ID as `task`, and `outcome: "done"` on `READY` or `outcome: "failed"` on `FAILED`.

### 2. On `FAILED`

1. In each repository, restore the tracked files with `git restore --staged --worktree .`.
2. In each repository, delete only the untracked files the task created, as `git status --porcelain` lists them.
3. Set the task to `status: failed` in `tasks.md` with `edit_file`.
4. Set `stage: blocked` and `blocked: { "stage": "implement", "reason": "<the agent's reason>" }`. Stop.

### 3. On `READY`

Commit as section **4. Commit** of the plugin root's `skills/autonomously/SKILL.md` says. The task is the phase. The session is the task that section names:

- In each repository, the session's first commit is phase 0. Take its subject and body from `spec.md`: they describe the whole session.
- The phase number `N` counts the session's commits already recorded in `tasks.md` for that repository.
- For a fix task, the body names the finding it fixes.

If the commit fails, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "<git's error>" }`. Leave the changes in place. Stop.

With `edit_file`, record each commit in the task's `commit` field, as `state.md` shows, and set the task to `status: done`. Update `tasksOpen` and `tasksDone`.

### When no task is open

If `lastRound` is `true`, set `stage: done` and go to **Done**. Otherwise set `stage: verify` and go to **Verify**.

## Verify

If `list_files` lists `round-{round}/report.md` and not `round-{round}/triage.md`, the round's review is done. Skip to step 7.

1. Run `git status --porcelain`. If it prints anything, list the files, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "dirty worktree" }`, then stop.
2. Add one to `round`. The run directory is `factory:{org}/{repo}/{session id}/round-{round}/`.
3. The target is the range `{merge base}..HEAD`, where the merge base is `git merge-base {base} HEAD`. If the base branch is not local, use `{remote}/{base}`.
4. Read `spec.md` with `read_file`. Write a problem statement from it, as Step 2 of the plugin root's `skills/independent-reviewer/SKILL.md` describes. Keep only the problem and the intended behaviour. Cut the design.
5. Read the plugin root's `skills/verify/review.md` and follow its steps 1 to 3. Use the target, the run directory and the problem statement.
   - Take the `started` time for `runs.jsonl` before each dispatch.
   - When a reviewer returns, append a `review` line with `append_file`, the reviewer's name as `agent` and `file` set to its review file, such as `round-{round}/reviews/independent-review.md`.
   - When `jam:report-writer` returns, append a `report` line.
6. If `list_files` does not list `round-{round}/report.md`, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "no report" }`. Lower `round` by one. Stop.
7. Run step 4 of `review.md`, **Extra reviewers**, with the same target and run directory. `round-{round}/extras.md` holds the decision.
   - Before you run step 4, re-read `mode`. If it is not `interactive` and the decision is `pending`, set `decision: none`.
   - Append the `review` and `report` lines of the agents it runs, as in step 5.
8. Set `stage: triage` and go to **Triage**.

## Triage

Re-read `mode`. If `feedbackImport` is not null, read the plugin root's `skills/take/feedback.md` and follow **Triage imported feedback**. Use the `started` time it returns for this invocation. Otherwise take the `started` time for `runs.jsonl`, read the plugin root's `skills/take/triage.md` and follow it, with the session, the current round and the mode.

For a feedback import, **Triage imported feedback** in `feedback.md` replaces the ordinary triage call. On resume, it reconciles existing decisions before processing the rest.

After triage returns:

1. Take `ended`.
2. Update the counts. `mustFix`, `shouldFix` and `nit` count the findings left after triage step 2, by severity.
3. Clear `feedbackImport` in the same `update_session` call that applies the outcome below.
4. Append exactly one `triage` line to `runs.jsonl` with `append_file` and `agent: "main"`. Use this invocation's `started` and `ended`. This is the single append path, including direct import resumes.

Then branch on the outcome:

- `next round`: set `stage: implement` and go to **Implement**.
- `last round`: set `lastRound: true` and `stage: implement`. Go to **Implement**.
- `finalize`: set `stage: done` and go to **Done**.

## Done

Show me:

- the commits, one line each, from `tasks.md`: short hash and subject
- the tasks still `open` or `failed`
- the deferred findings without an issue, from `deferred.md`
- the session, as `factory:{org}/{repo}/{session id}/`

If any deferred finding has no issue, tell me that `$jam:gh-issue-create` opens one. Tell me to run `$jam:take -v` for another verify round. Remind me that the branch is ready for me to push, merge or open a PR.

## Rules

- You never merge, push or open a PR.
- A review session never commits, never checks out a branch and never posts the comment.
- Read and write the session's files only with the `jam-factory` tools, as `state.md` says.
- Update `session.json` with `update_session` on every stage change.
- Append a line to `runs.jsonl` with `append_file`, as `state.md` defines it: one per agent run, and one per main-conversation stage. For an agent run, record the `tokens`, `toolUses` and `durationMs` the **collaboration** tools report when the agent finishes, and `null` for each one they do not report.
- Ask me only where the session's mode allows it. A blocked session is the one exception.
- Keep me posted with one short line per stage change. For example: `feature-1: round 2 verify done, 1 must-fix, 2 should-fix → triage`.
