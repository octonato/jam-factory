---
name: take
description: Run one job through the Jam Factory — spec, plan, implement, verify and triage — tracked in files under $FACTORY_ROOT. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue to start the branch's job. Pass nothing to start it from the session. Pass a PR URL, or -p/--pr, to review a coworker's PR as a job. Add -r/--resume to resume the job, or -v/--verify to run another verify round on a done job. It runs on its own and asks only when the request is unclear. Add -a/--autonomously to never ask, or -i/--interactive to approve every step. Never merges or pushes.
allowed-tools: Bash(echo *), Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *)
---

Run my current job through the factory.

What I gave you: the arguments supplied when invoking this skill

Run `echo "${FACTORY_ROOT:-unset}"` to get the factory root.

Run `git rev-parse --show-toplevel` to get the worktree.

Run `git branch --show-current` to get the current branch.

Read the plugin root's `skills/take/state.md` before you start. It holds the format of every file this skill writes.

## Where things are

- **Worktree**: the value above. It is the main checkout or a linked worktree.
- **Git repositories**: the worktree, plus each folder below it that holds a `.git` entry. A project can hold subprojects with their own git repositories. Every `git status`, restore and commit in this skill runs in each git repository.
- **Factory root**: the value above. If it is `unset`, tell me to export `FACTORY_ROOT` as `<org>/.factory` in my shell alias. Also tell me to start Codex with `--add-dir "$FACTORY_ROOT"`. Then stop. Never guess the path.
- **Job id**: the last segment of the current branch. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Job folder**: `{factory root}/{repo}/jobs/{job id}/`.

Run every agent in this skill with `effort: "xhigh"`.

## Modes

The job's `mode` in `job.json` sets when the factory asks me something:

| Mode | Spec | Plan | Extra reviewers | Triage |
|---|---|---|---|---|
| `default` | only when the request is unclear | never | never | never |
| `autonomous` | never | never | never | never |
| `interactive` | always | always | always | always |

I may edit `mode` by hand at any time. Re-read `job.json` before each stage and before each step that may ask me something. Follow the mode it holds at that moment.

## Pick the command

If the current branch is empty, HEAD is detached. Tell me to switch to a feature branch and stop.

Take the flags out of what I gave you:

- `-a` or `--autonomously`: the mode is `autonomous`.
- `-i` or `--interactive`: the mode is `interactive`.
- `-r` or `--resume`: resume the job.
- `-v` or `--verify`: run another verify round on a done job.
- `-p` or `--pr`, with an optional PR number right after it: start a review job for that PR.

If I passed both mode flags, tell me they conflict and stop. If I passed `-p` with `-r` or `-v`, tell me they conflict and stop. The rest is the argument.

- **The job folder holds `job.json` with `archived: true`**: tell me the job is archived and stop.
- **The job folder holds `job.json`, and I passed `-v`**: if its `stage` is `done`, set `lastRound: false` and `stage: verify`, then go to **Resume**. Otherwise tell me the job is not done and stop.
- **The job folder holds `job.json`, and I passed `-r`**: go to **Resume**.
- **The job folder holds `job.json`**: tell me the job already exists, and to pass `-r` to resume it. Stop.
- **I passed `-r` or `-v`**: tell me the branch has no job. Stop.
- **I passed `-p`, or the argument is a PR URL**: read the plugin root's `skills/take/review.md` and go to its **New review job** section. A PR URL holds `/pull/`.
- **Anything else**: go to **New job**.

When you go to **Resume** and the argument is not empty, tell me the job already exists and ignore the argument.

## Usage

Show this list when a step says to show the usage:

- `$jam:take <description | issue>`: start the branch's job from a description or a GitHub issue.
- `$jam:take`: start the branch's job from what we discussed in this session.
- `$jam:take <PR URL>` or `$jam:take -p [number]`: start a review job for a coworker's PR. Without a number, `-p` takes the current branch's PR.
- `$jam:take -r`: resume the branch's job.
- `$jam:take -v`: run another verify round on the branch's done job.
- `-a` or `-i` on any call: set the job's mode to `autonomous` or `interactive`.

## New job

1. Find the base, the branch the job merges into. Take the first that works:
   1. The remote's default branch: `git symbolic-ref --short refs/remotes/{remote}/HEAD`, with the remote prefix dropped. The remote is `upstream` when `git remote` lists it, otherwise `origin`.
   2. `main`, when `git rev-parse --verify --quiet main` finds it.
2. If none works, ask me for the base. Otherwise tell me the base in one line, and don't ask.
3. If the current branch is the base, tell me to create a feature branch and run the skill there. Stop. The factory never creates branches.
4. If the argument is empty and we have not discussed what to build in this session, show the usage and stop.
5. Read the request:
   - **An empty argument**: the request is what we discussed in this session. Write the goal, the decisions we made and the constraints. A reader who never saw the session must be able to act on it.
   - **An issue number, `#n` or an issue URL**: run `gh issue view {issue} --json number,title,body,url`. The request is the issue's title, URL and body.
   - **Anything else**: the request is the argument, verbatim.
6. Write `{job folder}/request.md` with the request, as `state.md` shows.
7. Write `{job folder}/job.json` with this worktree, the current branch and the base. Set `mode` from the flag, or `default` without one. Set `flow: build`, `stage: spec`, `round: 0`, `lastRound: false`, every count `0`, `blocked: null` and `archived: false`. The Write tool creates the folders.
8. Go to **Spec**.

## Resume

Read `job.json`. If I passed a mode flag, set `mode` to it.

If its `flow` is `review`, read the plugin root's `skills/take/review.md`. Go to its section for the `stage`: `verify`, `submit`, `done` or `blocked`.

Otherwise, go to the section for its `stage`:

- `spec`: **Spec**
- `plan`: **Plan**
- `implement`: **Implement**
- `verify`: **Verify**
- `triage`: **Triage**
- `done`: **Done**
- `blocked`: show me the `blocked` reason. Ask me whether I fixed the cause. On yes, set `stage` to `blocked.stage` and `blocked` to `null`. Then go to that stage.

Tell me the job id, the mode, the stage and the round in one line before you go on.

## Spec

If `spec.md` exists:

- **`autonomous`**: accept it.
- **`default` or `interactive`**: show me its path and ask me to approve it or ask for changes. Apply the changes until I approve.

Otherwise pick how to write it:

- **`interactive`**: brainstorm it with me.
- **`autonomous`**: write it alone.
- **`default`**: judge whether the request is clear. A request is clear when it has one reading, a bounded scope and a checkable result. If it is clear, write it alone. Otherwise brainstorm it with me. Tell me your judgement in one line.

To brainstorm it with me, load `superpowers:brainstorming` with the Codex skill system, and follow it with these overrides:

- Start from the request in `{job folder}/request.md`.
- Write the spec to `{job folder}/spec.md`. Write a spec file on every path the skill offers, bounded ones included.
- Never commit the spec. Never write under `docs/superpowers/`.
- When I approve the spec, stop following the skill. Do not invoke `writing-plans` from it.

To write it alone, read the request and the code it touches. Ask me nothing. Write `{job folder}/spec.md`, sized to the task. A small, obvious task gets a short spec: the problem, the change and how to check it. List each choice the request left open under **Assumptions**. Never commit the spec.

Once the spec is approved, accepted or written, set `stage: plan` and go to **Plan**.

## Plan

If `plan.md` exists:

- **`interactive`**: show me its path and ask me to approve it or ask for changes. Apply the changes until I approve.
- **`default` or `autonomous`**: accept it.

Otherwise load `superpowers:writing-plans` with the Codex skill system, and follow it with these overrides:

- The spec is `{job folder}/spec.md`.
- Write the plan to `{job folder}/plan.md`. Never write under `docs/superpowers/`.
- Never commit the plan.
- Skip its execution handoff. In `interactive` mode, ask me to approve the plan instead. In the other modes, ask me nothing and accept the plan.

When I approve the plan or you accept it, write `tasks.md`. Add one `open` task per `### Task N` heading in the plan, in order. Give each task the heading's title and the source `plan task N`. Set its text to the task's full text. Set `tasksOpen`, set `stage: implement`, and go to **Implement**.

## Implement

Run `git status --porcelain`. If it prints anything, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "dirty worktree" }`. List the files and stop.

Set every `failed` task in `tasks.md` back to `open`. Then take each `open` task in order. For each one:

### 1. Implement

Run the `jam:implementer` agent with the **collaboration** tools. Pass it only:

- the worktree
- the path of `spec.md`
- the path of `plan.md`
- the task's ID, title and text

Pass nothing else from this session. Read the last line of its reply.

### 2. On `FAILED`

1. In each repository, restore the tracked files with `git restore --staged --worktree .`.
2. In each repository, delete only the untracked files the task created, as `git status --porcelain` lists them.
3. Set the task to `status: failed`.
4. Set `stage: blocked` and `blocked: { "stage": "implement", "reason": "<the agent's reason>" }`. Stop.

### 3. On `READY`

Commit as section **4. Commit** of the plugin root's `skills/autonomously/SKILL.md` says. The task is the phase. The job is the task that section names:

- In each repository, the job's first commit is phase 0. Take its subject and body from `spec.md`: they describe the whole job.
- The phase number `N` counts the job's commits already recorded in `tasks.md` for that repository.
- For a fix task, the body names the finding it fixes.

If the commit fails, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "<git's error>" }`. Leave the changes in place. Stop.

Record each commit in the task's `commit` field, as `state.md` shows, and set the task to `status: done`. Update `tasksOpen` and `tasksDone`.

### When no task is open

If `lastRound` is `true`, set `stage: done` and go to **Done**. Otherwise set `stage: verify` and go to **Verify**.

## Verify

If `round-{round}/report.md` exists and `round-{round}/triage.md` does not, the round's review is done. Skip to step 7.

1. Run `git status --porcelain`. If it prints anything, list the files, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "dirty worktree" }`, then stop.
2. Add one to `round`. The round folder is `{job folder}/round-{round}/`.
3. The target is the range `{merge base}..HEAD`, where the merge base is `git merge-base {base} HEAD`. If the base branch is not local, use `{remote}/{base}`.
4. Write a problem statement from `spec.md`, as Step 2 of the plugin root's `skills/independent-reviewer/SKILL.md` describes. Keep only the problem and the intended behaviour. Cut the design.
5. Read the plugin root's `skills/verify/review.md` and follow its steps 1 to 3. Use the target, the round folder as the run directory, and the problem statement.
6. If `report.md` is missing from the round folder, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "no report" }`. Lower `round` by one. Stop.
7. Follow step 4 of `review.md`, **Extra reviewers**, with the same target and the round folder as the run directory. `round-{round}/extras.md` holds the decision. Before you follow it, re-read `mode`. If it is not `interactive` and the decision is `pending`, set `decision: none`.
8. Set `stage: triage` and go to **Triage**.

## Triage

Re-read `mode`. Read the plugin root's `skills/take/triage.md` and follow it, with the job folder, the current round and the mode.

Then update the counts. `mustFix`, `shouldFix` and `nit` count the findings left after triage step 2, by severity. Then branch on the outcome:

- `next round`: set `stage: implement` and go to **Implement**.
- `last round`: set `lastRound: true` and `stage: implement`. Go to **Implement**.
- `finalize`: set `stage: done` and go to **Done**.

## Done

Show me:

- the commits, one line each, from `tasks.md`: short hash and subject
- the tasks still `open` or `failed`
- the deferred findings without an issue, from `deferred.md`
- the job folder

If any deferred finding has no issue, tell me that `$jam:gh-issue-create` opens one. Tell me to run `$jam:take -v` for another verify round. Remind me that the branch is ready for me to push, merge or open a PR.

## Rules

- You never merge, push or open a PR.
- A review job never commits, never checks out a branch and never posts the comment.
- Update `job.json` on every stage change, as `state.md` says.
- Ask me only where the job's mode allows it. A blocked job is the one exception.
- Keep me posted with one short line per stage change. For example: `feature-1: round 2 verify done, 1 must-fix, 2 should-fix → triage`.
