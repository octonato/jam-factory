---
description: Independent, read-only code review of a PR, a commit range or the current branch. A fresh subagent reviews the change without being told how it was built, and writes the report to .nogit/independent-review.md. Add --report-in-chat to get the report in chat instead.
argument-hint: [PR number or URL | commit hash (inclusive) | range a..b | empty for the current branch] [--report-in-chat]
---

Run an **independent, read-only code review** of a change.

What I gave you: $ARGUMENTS

The review runs in the `oc:independent-reviewer` agent, on a fresh context. The agent must not see this session's context. It gets the problem the change was meant to solve, and nothing about how it was solved. It forms its own judgment.

## Step 1: Resolve the target

If `$ARGUMENTS` contains `--report-in-chat`, remove it and use the `chat` output mode. Otherwise use the `file` output mode.

Read the rest of `$ARGUMENTS` and pick one case:

- **Empty** → the current branch against `main`. The base is `git merge-base main HEAD`. The diff is `git diff main...HEAD`.
- **A PR number or URL** (`247`, `#247`, `https://github.com/.../pull/247`) → that PR. Get its base and head with `gh pr view <n> --json number,title,body,baseRefName,headRefName,headRefOid,closingIssuesReferences`. Get the diff with `gh pr diff <n>`.
- **A range** `a..b` → the commits in that range. The diff is `git diff a..b`.
- **A commit hash** → that commit through `HEAD`. The hash is inclusive, so the range is `<hash>^..HEAD`.

If you can't tell which case applies, ask me and stop.

For a PR, check whether its head commit is `HEAD` in this checkout. If it is not, run `git fetch origin pull/<n>/head` so the agent can read files with `git show FETCH_HEAD:<path>`. Never switch branches, stash or reset in my checkout.

## Step 2: Write the problem statement

Write a short statement of the **original problem and the intended behaviour**. Take it from:

- the linked issue, when there is one (`gh issue view <n>`);
- the PR title and the problem part of the PR body;
- the commit messages, for a hash, a range or the current branch.

Keep **only** the problem: the symptom or goal, the requirements, and how you'd know it's solved. Cut every mention of the approach, the files changed, the fix, or the design. That detail biases the reviewer. If the sources say nothing about the problem, write "No problem statement is available. Infer the intent from the code and the tests."

End the statement with: "That is the problem and intent only. Work out the cause and judge the solution yourself."

Do not show me this statement before the review. Do not add your own opinion of the change.

## Step 3: Dispatch the reviewer

Launch the `oc:independent-reviewer` agent. Pass it only these four things:

- **The target**: "PR #<n> (<headRefName> into <baseRefName>)", "commits <a>..<b>", or "branch <name> against main".
- **The commands** to read it: the `git log` and `git diff` (or `gh pr diff`) commands for this target. For a PR not checked out locally, say that it must read files with `git show FETCH_HEAD:<path>`.
- **The problem statement** from Step 2.
- **The output mode** from Step 1.

Pass nothing else from this session.

## Step 4: Report

In `file` mode, print the verdict line and the path `.nogit/independent-review.md` when the agent finishes. Do not repeat the findings. I read the report myself.

In `chat` mode, print the agent's review as it returned it.
