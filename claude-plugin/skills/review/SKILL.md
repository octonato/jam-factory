---
name: review
disable-model-invocation: true
description: Review a coworker's open PR as a factory review session that Jam Board shows. Each run is a round. The first round reviews the whole PR. A later round reviews the commits since the last round, or the whole PR after a rebase. Pass a commit hash to review from that commit. Each round writes the PR comment to round-N/submit.md, drops the findings already on the PR and offers to open the comment on GitHub. It never posts the comment. Add -a/--autonomously to never ask, or -i/--interactive to pick extra reviewers.
argument-hint: "[-a|--autonomously | -i|--interactive] [PR number or URL | commit hash]"
allowed-tools: Bash(echo *), Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *), Bash(*/skills/review/open-comment.sh *), Bash(*/skills/review/fetch-comments.sh *), mcp__plugin_jam_jam-factory
effort: xhigh
---

Review a coworker's open PR as a factory review session. Change no code.

What I gave you: $ARGUMENTS

Worktree: !`git rev-parse --show-toplevel`

Current branch: !`git branch --show-current`

HEAD: !`git rev-parse --short HEAD`

Read `${CLAUDE_PLUGIN_ROOT}/skills/take/state.md` before you start. It holds the tools and the format of every file this skill writes.

## Where things are

- **Worktree**: the value above. It is the main checkout or a linked worktree.
- **Branch**: the last segment of the current branch. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Org**: the name of the folder above the repo folder. For the worktree `~/Sources/octonato/jam-factory/rate-limit`, the org is `octonato` and the repo is `jam-factory`.
- **Session id**: the id of the branch's live review, such as `review-pr42-20261115-1152-e5a2575`. **Routing** finds it, or `start_session` returns it.
- **Session**: the session named by `org`, `repo`, `branch` and `session id`. The tools of the `jam-factory` MCP server read and write its files, as `state.md` says. Every file path in this skill, such as `round-{round}/report.md`, is relative to the session.

If the `jam-factory` tools are missing, tell me to run `just install` in the `app/` folder of the jam-factory repository. Then stop.

After this skill creates `session.json`, each change to it, such as "set `stage: submit`", is an `update_session` call.

## Modes

The session's `mode` in `session.json` sets when the review session asks me something:

| Mode | Extra reviewers | Open the comment |
|---|---|---|
| `default` | never | ask |
| `autonomous` | never | never |
| `interactive` | always | ask |

I may edit `mode` by hand at any time. Re-read `session.json` before each stage and before each step that may ask me something. Follow the mode it holds at that moment.

## Arguments

Take the flags out of what I gave you:

- `-a` or `--autonomously`: the mode is `autonomous`.
- `-i` or `--interactive`: the mode is `interactive`.

If I passed both mode flags, tell me they conflict and stop. The rest is the argument. It is one of:

- **Nothing**: the PR is the current branch's PR.
- **A PR number or URL**: the PR is that number or URL. A PR URL holds `/pull/`. `#n` and a number of up to 6 digits are PR numbers.
- **A commit hash**: 7 to 40 hex characters. The PR is the current branch's PR. The hash is inclusive: the round reviews `{hash}^..HEAD`.
- **Anything else**, such as a range, a description or another flag: tell me that `/jam:review` reviews a PR, and that `/jam:verify` reviews my own changes. Stop.

## Checks

Run these checks before you change any session file:

1. If the current branch is empty, HEAD is detached. Tell me to check out the PR's branch with `gh pr checkout` and stop.
2. Without a PR number or URL, run `gh pr view --json number` to find the current branch's PR. If the branch has no PR, tell me so and stop.
3. Run `gh pr view {pr} --json number,title,body,url,headRefName,headRefOid,baseRefName,state`. If `state` is not `OPEN`, tell me the PR is `{state}` and stop. From here on, `{pr}` is its `number`.
4. With a PR number or URL, run `gh pr view --json number` on the current branch. If the branch has no PR, or its `number` is not `{pr}`, tell me to run `gh pr checkout {pr}` and stop. Do not compare branch names: `gh pr checkout` may name the local branch differently from `headRefName`. The factory never checks out or creates a branch.
5. With a commit hash, check that it is a commit of the PR. The remote is `upstream` when `git remote` lists it, otherwise `origin`. Run `git fetch {remote} {base}`, with `{base}` the PR's `baseRefName`. The hash is a commit of the PR when `git merge-base --is-ancestor {hash} HEAD` succeeds and `git merge-base --is-ancestor {hash} {remote}/{base}` fails. Otherwise tell me `{hash}` is not a commit of the PR and stop. If `git rev-parse HEAD` differs from the PR's `headRefOid`, also tell me that HEAD is not the PR head, and that `git pull --ff-only` brings in the PR's new commits.

## Routing

Call `list_sessions` with `org`, `repo`, `branch` and `category: review`. When it lists a session, it is the branch's live review. Use its `session` value as the session id, and read its `session.json` with `read_file`.

Before you route, stop on these cases:

- **The live review's `pr` is not `{pr}`**: tell me the branch already has the live review `{session id}` for another PR. Tell me to archive it on Jam Board first. Stop.
- **I passed a commit hash, and the live review's round is unfinished**: tell me to run `/jam:review` without an argument to finish that round first. Stop. The round is unfinished at stage `verify`, `submit` or `blocked`, unless the last run stopped before its round started. That is the case when `list_files` lists no file under `round-{round + 1}/`, and one of these holds:
  - `stage` is `verify` and `round` is `0`.
  - `stage` is `blocked` and `blocked.stage` is `verify`.

If I passed a mode flag and there is a live review, set `mode` to it.

Then route:

| Live review | Action |
|---|---|
| none | go to **New session** |
| stage `verify` | resume it: go to **Verify** |
| stage `submit` | resume it: go to **Submit** |
| stage `blocked` | resume it: go to **Blocked** |
| stage `done` | start a new round: go to **Verify** step 1 |

When you resume or start a new round, tell me the session id, the mode, the stage and the round in one line before you go on.

## New session

1. Call `start_session` with `category: review`, HEAD as `head` and `{pr}` as `pr`, as `state.md` says. Pass `fields` with this worktree, the repo and the full name of the current branch. Set `flow: review`, `pr` to `{pr}` and `base` to the PR's `baseRefName`. Set `mode` from the flag, or `default` without one. Set `stage: verify`, `round: 0`, `lastRound: false`, every count `0` and `blocked: null`. If the call fails with `exists`, tell me its message and stop. Use the returned `session` as the session id.
2. Create `request.md` with `write_file`, as `state.md` shows. The `source` is the PR's `url`. The body holds the PR's title and body.
3. Append a `request` line to `runs.jsonl` with `append_file` and `agent: "main"`. Set its `started` and `ended` to the current time. Set `durationMs: null`.
4. Go to **Verify** step 1.

## Verify

On a resume at stage `verify`, read `round-{round}/report.md` with `read_file`. If it holds an entry under **Findings** and `list_files` does not list `round-{round}/submit.md`, the round's review is done. Skip to step 9.

1. Run `git status --porcelain`. If it prints anything, list the files, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "dirty worktree" }`, then stop.
2. Sync with the PR. Run `gh pr view {pr} --json headRefOid,state`.
   - If `state` is not `OPEN`, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "PR is {state}" }`. Stop.
   - If `git rev-parse HEAD` differs from `headRefOid`, run `git pull --ff-only`. If it fails, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "git pull failed: run gh pr checkout {pr} --force" }`. Stop.
   - After the pull, run `git rev-parse HEAD` again. If it still differs from `headRefOid`, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "HEAD is not the PR head: run gh pr checkout {pr} --force" }`. Stop.
3. Pick the scope:
   - **A commit hash**: the scope is the range `{hash}^..HEAD`. **Checks** step 5 already checked that the hash is a commit of the PR.
   - **No commit hash, and `round` is `0`**: the scope is the whole PR.
   - **No commit hash, and `round` is above `0`**: the scope is the range `{reviewedHead}..HEAD` when `reviewedHead` is set, `git merge-base --is-ancestor {reviewedHead} HEAD` succeeds and `git rev-list --merges {reviewedHead}..HEAD` prints nothing. Otherwise the scope is the whole PR. Tell me why in one line: the session has no recorded head, the branch was rebased, or the branch has a merge since the last round. A merge, such as GitHub's **Update branch**, brings the base branch's changes into the range.
4. Check the scope's diff. For a range, run `git diff --shortstat {range}`. For the whole PR, run `gh pr diff {pr} --name-only`. If it prints nothing, tell me there is nothing new to review and stop. `round` stays as it is.
5. Add one to `round` and set `stage: verify`. The run directory is `factory:{org}/{repo}/{branch}/{session id}/round-{round}/`.
6. The target is the PR number for the whole PR, or the range. Write a problem statement from the linked issue and the PR's title and body, as Step 2 of `${CLAUDE_PLUGIN_ROOT}/skills/independent-reviewer/SKILL.md` describes. The problem statement comes from the PR for a range too.
7. Read `${CLAUDE_PLUGIN_ROOT}/skills/verify/review.md` and follow its steps 1 to 3. Use the target, the run directory and the problem statement. Tell it that the caller already checked the PR checkout in **Checks**. Its step 1 then skips that check. Give the reviewers no findings from earlier rounds. Take the `started` time for `runs.jsonl` before each dispatch. When a reviewer returns, append a `review` line with `append_file`, the reviewer's name as `agent` and `file` set to its review file, such as `round-{round}/reviews/independent-review.md`. When `jam:report-writer` returns, append a `report` line.
8. If `list_files` does not list `round-{round}/report.md`, set `stage: blocked` and `blocked: { "stage": "verify", "reason": "no report" }`. Lower `round` by one. Stop.
9. Follow step 4 of `review.md`, **Extra reviewers**, with the target and the same run directory. When you skipped to this step, take the target from the `target` field of `report.md`. `round-{round}/extras.md` holds the decision. Before you follow it, re-read `mode`. If it is not `interactive` and the decision is `pending`, set `decision: none`. Append the `review` and `report` lines of the agents it runs, as in step 7.
10. In one `update_session` call, set `mustFix`, `shouldFix` and `nit` from the findings in `report.md`, by severity. Set `reviewedHead` to the full sha that `git rev-parse HEAD` prints. Set `stage: submit`. Go to **Submit**.

## Submit

Take the `started` time for `runs.jsonl`. The run directory is `factory:{org}/{repo}/{branch}/{session id}/round-{round}/`.

1. Read `${CLAUDE_PLUGIN_ROOT}/skills/review/submit.md` and follow it. Pass it the run directory, and the `pr` of `session.json` as its `{pr}`. Steps 3 to 5 of `submit.md` use that PR number, for a range too.
2. The offer to open the comment follows the mode. Re-read `mode` before step 4 of `submit.md`, **Offer to open**:
   - **`default` and `interactive`**: follow steps 4 and 5. Step 4 asks me.
   - **`autonomous`**: follow `submit.md` steps 1 to 3 only. Do not open the comment.
3. If `submit.md` picks no finding, the session still goes to `done`. The same holds when step 3, **Drop the findings already on the PR**, drops every finding. If `submit.md` stops on an error, append a `comment` line to `runs.jsonl` with `append_file`, `agent: "main"` and `outcome: "blocked"`. Set `stage: blocked` and `blocked: { "stage": "submit", "reason": "<the error>" }`. Stop.
4. Append a `comment` line to `runs.jsonl` with `append_file`, `agent: "main"` and `outcome: "done"` on every successful Submit, including when there are no findings or all findings were dropped and no `submit.md` was written. The dashboard treats this line as the round's completion signal. It also accepts an existing `submit.md`.
5. Set `stage: done` and go to **Done**.

## Done

Show me:

- the PR URL, from `request.md`
- `factory:{org}/{repo}/{branch}/{session id}/round-{round}/submit.md` and the number of findings in it, or that the round had no findings
- when `submit.md` holds findings and you did not open it, the command that opens it, with absolute paths, prefixed with `! `: `${CLAUDE_PLUGIN_ROOT}/skills/review/open-comment.sh {pr} {comment file}`. To get the comment file, read `round-{round}/submit.md` with `read_file` and write its content to `$TMPDIR/submit-{pr}.md` with the **Write** tool.
- the session, as `factory:{org}/{repo}/{branch}/{session id}/`

Tell me to run `/jam:review` again after the coworker pushes new commits. Do not remind me to push, merge or open a PR.

## Blocked

Show me the `blocked` reason. Ask me whether I fixed the cause. On yes, set `stage` to `blocked.stage` and `blocked` to `null`. Then go to that stage.

## Rules

- You never commit, never check out a branch and never post the comment.
- Read and write the session's files only with the `jam-factory` tools, as `state.md` says.
- Update `session.json` with `update_session` on every stage change.
- Append a line to `runs.jsonl` with `append_file`, as `state.md` defines it: one per agent run, and one per main-conversation stage. For an agent run, take `tokens`, `toolUses` and `durationMs` from the `totalTokens`, `toolUses` and `durationMs` the **Agent** tool reports when the agent finishes.
- Run every agent in this skill with `effort: "xhigh"`.
- Ask me only where the session's mode allows it. A blocked session is the one exception.
- Keep me posted with one short line per stage change. For example: `rate-limit: round 2 verify done, 1 must-fix, 2 should-fix → submit`.
