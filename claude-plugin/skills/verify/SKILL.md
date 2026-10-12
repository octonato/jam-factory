---
name: verify
disable-model-invocation: true
description: Deep, read-only review of a PR, a commit range or the current branch. Runs the pr-review-toolkit reviewers and oc:independent-reviewer in parallel at xhigh effort, checks each finding against the code, and writes one report to .nogit/verify-{timestamp}/. Use /oc:submit-review to turn the report into a PR comment. Changes no code.
argument-hint: [PR number or URL | commit hash (inclusive) | range a..b | empty for the current branch]
allowed-tools: Bash(date *)
effort: xhigh
---

Review my changes and write the findings to a report. Change no code.

What I gave you: $ARGUMENTS

Run timestamp: !`date +%Y%m%d-%H%M%S`

The run directory is `.nogit/verify-{run timestamp}/`, in the repository that owns the target.

## Review

Read `${CLAUDE_PLUGIN_ROOT}/skills/verify/review.md` and follow it from start to end, with `$ARGUMENTS` as the target and this run directory.

## Report

When the review finishes, print:

- the path of `report.md`
- the number of findings per severity, worst first
- the number of open questions

Do not repeat the findings. I read the report myself.
