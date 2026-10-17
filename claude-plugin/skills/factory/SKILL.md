---
name: factory
disable-model-invocation: true
description: Run one job through the software factory — spec, plan, implement, verify and triage — tracked in files under $FACTORY_ROOT. Run it on a feature branch, in the main checkout or in a worktree. Pass a description or a GitHub issue to start the branch's job, or nothing to resume it. Pass issues to turn deferred findings into GitHub issues. Never merges or pushes.
argument-hint: [description | issue number or URL | issues]
allowed-tools: Bash(echo *), Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *)
effort: xhigh
---

Run my current job through the factory.

What I gave you: $ARGUMENTS

Factory root: !`echo "${FACTORY_ROOT:-unset}"`

Worktree: !`git rev-parse --show-toplevel`

Current branch: !`git branch --show-current`

Read `${CLAUDE_PLUGIN_ROOT}/skills/factory/state.md` before you start. It holds the format of every file this skill writes.

## Where things are

- **Worktree**: the value above. It is the main checkout or a linked worktree.
- **Git repositories**: the worktree, plus each folder below it that holds a `.git` entry. A project can hold subprojects with their own git repositories. Every `git status`, restore and commit in this skill runs in each git repository.
- **Factory root**: the value above. If it is `unset`, tell me to export `FACTORY_ROOT` as `<org>/.factory` in my shell alias. Also tell me to start Claude with `--add-dir "$FACTORY_ROOT"`. Then stop. Never guess the path.
- **Job id**: the last segment of the current branch. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Job folder**: `{factory root}/{repo}/jobs/{job id}/`.

Run every agent in this skill with `effort: "xhigh"`.

## Pick the command

If the current branch is empty, HEAD is detached. Tell me to switch to a feature branch and stop.

- **`issues`**: go to **Issues**.
- **The job folder holds `job.json` with `archived: true`**: tell me the job is archived and stop.
- **The job folder holds `job.json`**: go to **Resume**. If I passed anything else, tell me the job already exists and ignore it.
- **Anything else**: go to **New job**.

## New job

1. Find the base, the branch the job merges into. Take the first that works:
   1. The remote's default branch: `git symbolic-ref --short refs/remotes/{remote}/HEAD`, with the remote prefix dropped. The remote is `upstream` when `git remote` lists it, otherwise `origin`.
   2. `main`, when `git rev-parse --verify --quiet main` finds it.
2. If none works, ask me for the base. Otherwise tell me the base in one line, and don't ask.
3. If the current branch is the base, tell me to create a feature branch and run the skill there. Stop. The factory never creates branches.
4. If I passed nothing, ask me what to build, as a description or a GitHub issue.
5. Read the request:
   - **An issue number, `#n` or an issue URL**: run `gh issue view {issue} --json number,title,body,url`. The request is the issue's title, URL and body.
   - **Anything else**: the request is my text, verbatim.
6. Write `{job folder}/request.md` with the request, as `state.md` shows.
7. Write `{job folder}/job.json` with this worktree, the current branch and the base. Set `stage: spec`, `round: 0`, `lastRound: false`, every count `0`, `blocked: null` and `archived: false`. The Write tool creates the folders.
8. Go to **Spec**.

## Resume

Read `job.json`. Go to the section for its `stage`:

- `spec`: **Spec**
- `plan`: **Plan**
- `implement`: **Implement**
- `verify`: **Verify**
- `triage`: **Triage**
- `done`: **Done**
- `blocked`: show me the `blocked` reason. Ask me whether I fixed the cause. On yes, set `stage` to `blocked.stage` and `blocked` to `null`. Then go to that stage.

Tell me the job id, the stage and the round in one line before you go on.

## Spec

If `spec.md` exists, show me its path and ask me to approve it or ask for changes. Apply the changes until I approve.

Otherwise load `superpowers:brainstorming` with the **Skill** tool, and follow it with these overrides:

- Start from the request in `{job folder}/request.md`.
- Write the spec to `{job folder}/spec.md`. Write a spec file on every path the skill offers, bounded ones included.
- Never commit the spec. Never write under `docs/superpowers/`.
- When I approve the spec, stop following the skill. Do not invoke `writing-plans` from it.

When I approve the spec, set `stage: plan` and go to **Plan**.

## Plan

If `plan.md` exists, show me its path and ask me to approve it or ask for changes. Apply the changes until I approve.

Otherwise load `superpowers:writing-plans` with the **Skill** tool, and follow it with these overrides:

- The spec is `{job folder}/spec.md`.
- Write the plan to `{job folder}/plan.md`. Never write under `docs/superpowers/`.
- Never commit the plan.
- Skip its execution handoff. Ask me to approve the plan instead.

When I approve the plan, write `tasks.md`. Add one `open` task per `### Task N` heading in the plan, in order. Give each task the heading's title and the source `plan task N`. Set its text to the task's full text. Set `tasksOpen`, set `stage: implement`, and go to **Implement**.

## Implement

Run `git status --porcelain`. If it prints anything, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "dirty worktree" }`. List the files and stop.

Set every `failed` task in `tasks.md` back to `open`. Then take each `open` task in order. For each one:

### 1. Implement

Run the `oc:implementer` agent with the **Agent** tool. Pass it only:

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

Commit as section **4. Commit** of `${CLAUDE_PLUGIN_ROOT}/commands/autonomously.md` says. The task is the phase. The job is the task that section names:

- In each repository, the job's first commit is phase 0. Take its subject and body from `spec.md`: they describe the whole job.
- The phase number `N` counts the job's commits already recorded in `tasks.md` for that repository.
- For a fix task, the body names the finding it fixes.

If the commit fails, set `stage: blocked` and `blocked: { "stage": "implement", "reason": "<git's error>" }`. Leave the changes in place. Stop.

Record each commit in the task's `commit` field, as `state.md` shows, and set the task to `status: done`. Update `tasksOpen` and `tasksDone`.

### When no task is open

If `lastRound` is `true`, set `stage: done` and go to **Done**. Otherwise set `stage: verify` and go to **Verify**.

## Verify

If `round-{round}/report.md` exists, the round's review is done. Skip to step 7.

1. Run `git status --porcelain`. If it prints anything, list the files, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "dirty worktree" }`, then stop.
2. Add one to `round`. The round folder is `{job folder}/round-{round}/`.
3. The target is the range `{merge base}..HEAD`, where the merge base is `git merge-base {base} HEAD`. If the base branch is not local, use `{remote}/{base}`.
4. Write a problem statement from `spec.md`, as Step 2 of `${CLAUDE_PLUGIN_ROOT}/skills/independent-reviewer/SKILL.md` describes. Keep only the problem and the intended behaviour. Cut the design.
5. Read `${CLAUDE_PLUGIN_ROOT}/skills/verify/review.md` and follow its steps 1 to 3. Use the target, the round folder as the run directory, and the problem statement.
6. If `report.md` is missing from the round folder, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "no report" }`. Lower `round` by one. Stop.
7. Follow step 4 of `review.md`, **Extra reviewers**, with the same target and the round folder as the run directory. `round-{round}/extras.md` holds the decision.
8. Set `stage: triage` and go to **Triage**.

## Triage

Read `${CLAUDE_PLUGIN_ROOT}/skills/factory/triage.md` and follow it, with the job folder and the current round.

Then update the counts. `mustFix`, `shouldFix` and `nit` count the findings shown to me in step 3 of triage, by severity. Then branch on the outcome:

- `next round`: set `stage: implement` and go to **Implement**.
- `last round`: set `lastRound: true` and `stage: implement`. Go to **Implement**.
- `finalize`: set `stage: done` and go to **Done**.

## Done

Show me:

- the commits, one line each, from `tasks.md`: short hash and subject
- the tasks still `open` or `failed`
- the deferred findings without an issue, from `deferred.md`
- the job folder

If any deferred finding has no issue, tell me to run `/oc:factory issues`. Remind me that the branch is ready for me to push, merge or open a PR.

## Issues

Run it on the job's branch. If the job folder holds no `job.json`, tell me and stop.

Take each entry in `deferred.md` with `issue: none`, in order. If no entry has `issue: none`, tell me and stop. For each one:

1. Read `${CLAUDE_PLUGIN_ROOT}/commands/gh.issue-create.md` and follow its instructions. The description is the entry's problem, its location and its suggested fix. The remote is `upstream` when `git remote` lists it, otherwise `origin`.
2. When the issue opens in the browser, ask me with **AskUserQuestion** whether I submitted it. Offer `Submitted` and `Skip`.
3. On `Skip`, take the next entry.
4. On `Submitted`, run `gh issue list --repo {owner/repo} --search "{title} in:title" --state open --json url,title --limit 5`. Take the URL whose title matches exactly. If none matches, ask me for the URL.
5. Set the entry's `issue` to the URL. Increment `issued` in `job.json`.

When all entries are done, list each issue URL with its entry.

## Rules

- You never merge, push or open a PR.
- Update `job.json` on every stage change, as `state.md` says.
- Keep me posted with one short line per stage change. For example: `feature-1: round 2 verify done, 1 must-fix, 2 should-fix → triage`.
