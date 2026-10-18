# Factory review job

This procedure runs a review job. `SKILL.md` routes here. The formats are in `state.md`.

## Modes

The job's `mode` in `job.json` sets when a review job asks me something:

| Mode | Extra reviewers | Open the comment |
|---|---|---|
| `default` | never | ask |
| `autonomous` | never | never |
| `interactive` | always | ask |

Re-read `job.json` before each stage and before each step that may ask me something. Follow the mode it holds at that moment.

## New review job

`SKILL.md` already checked that the job folder holds no `job.json`.

1. Find the PR:
   - **A PR URL**: the PR is the URL.
   - **`-p` with a number**: the PR is the number.
   - **`-p` alone**: run `gh pr view --json number`. If the current branch has no PR, tell me so and stop.
2. Run `gh pr view {pr} --json number,title,body,url,headRefName,baseRefName,state`.
3. If `state` is not `OPEN`, tell me the PR is `{state}` and stop.
4. If the current branch is not `headRefName`, tell me to run `gh pr checkout {number}` and stop. The factory never checks out or creates a branch.
5. Write `{job folder}/request.md`, as `state.md` shows. The `source` is the PR's `url`. The body holds the PR's title and body.
6. Write `{job folder}/job.json` with this worktree and the current branch. Set `flow: review`, `pr` to the PR's `number` and `base` to its `baseRefName`. Set `mode` from the flag, or `default` without one. Set `stage: verify`, `round: 0`, `lastRound: false`, every count `0`, `blocked: null` and `archived: false`. The Write tool creates the folders.
7. Go to **Verify**.

## Verify

If `round-{round}/report.md` holds an entry under **Findings** and `round-{round}/submit.md` does not exist, the round's review is done. Skip to step 7.

1. Run `git status --porcelain`. If it prints anything, list the files, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "dirty worktree" }`, then stop.
2. Sync with the PR. Run `gh pr view {pr} --json headRefOid,state`.
   - If `state` is not `OPEN`, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "PR is {state}" }`. Stop.
   - If `git rev-parse HEAD` differs from `headRefOid`, run `git pull --ff-only`. If it fails, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "git pull failed: run gh pr checkout {pr} --force" }`. Stop.
   - After the pull, run `git rev-parse HEAD` again. If it still differs from `headRefOid`, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "HEAD is not the PR head: run gh pr checkout {pr} --force" }`. Stop.
3. Add one to `round`. The round folder is `{job folder}/round-{round}/`.
4. The target is the PR number. Write a problem statement from the linked issue and the PR's title and body, as Step 2 of `${CLAUDE_PLUGIN_ROOT}/skills/independent-reviewer/SKILL.md` describes.
5. Read `${CLAUDE_PLUGIN_ROOT}/skills/verify/review.md` and follow its steps 1 to 3. Use the target, the round folder as the run directory, and the problem statement.
6. If `report.md` is missing from the round folder, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "no report" }`. Lower `round` by one. Stop.
7. Follow step 4 of `review.md`, **Extra reviewers**, with the PR number as the target and the round folder as the run directory. `round-{round}/extras.md` holds the decision. Before you follow it, re-read `mode`. If it is not `interactive` and the decision is `pending`, set `decision: none`.
8. Set `mustFix`, `shouldFix` and `nit` from the findings in `report.md`, by severity. Set `stage: submit` and go to **Submit**.

## Submit

1. Read `${CLAUDE_PLUGIN_ROOT}/skills/submit-review/submit.md` and follow it, with the round folder as the run directory and the job's `pr`.
2. The offer to open the comment follows the mode. Re-read `mode` before step 4 of `submit.md`, **Offer to open**:
   - **`default` and `interactive`**: follow steps 4 and 5. Step 4 asks me.
   - **`autonomous`**: follow `submit.md` steps 1 to 3 only. Do not open the comment.
3. If `submit.md` picks no finding, the job still goes to `done`. The same holds when step 3, **Drop the findings already on the PR**, drops every finding. If `submit.md` stops on an error, set `stage: blocked` and `blocked: { "stage": "submit", "reason": "<the error>" }`. Stop.
4. Set `stage: done` and go to **Done**.

## Done

Show me:

- the PR URL, from `request.md`
- the path of `round-{round}/submit.md` and the number of findings in it, or that the round had no findings
- when `submit.md` holds findings and you did not open it, the command that opens it, with absolute paths, prefixed with `! `: `${CLAUDE_PLUGIN_ROOT}/skills/submit-review/open-comment.sh {pr} {path of submit.md}`
- the job folder

Tell me to run `/jam:take -v` after the coworker pushes new commits. Do not remind me to push, merge or open a PR.

## Blocked

Show me the `blocked` reason. Ask me whether I fixed the cause. On yes, set `stage` to `blocked.stage` and `blocked` to `null`. Then go to that stage.
