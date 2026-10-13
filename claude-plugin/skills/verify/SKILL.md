---
name: verify
disable-model-invocation: true
description: Read-only review of a PR, a commit range or the current branch. Runs oc:independent-reviewer and superpowers:requesting-code-review in parallel at xhigh effort. Add --deep to also run the pr-review-toolkit reviewers. It checks each finding against the code, and writes one report to .nogit/verify-{timestamp}/. Use /oc:submit-review to turn the report into a PR comment. Changes no code.
argument-hint: [--deep] [PR number or URL | commit hash (inclusive) | range a..b | empty for the current branch]
allowed-tools: Bash(date *)
effort: xhigh
---

Review my changes and write the findings to a report. Change no code.

What I gave you: $ARGUMENTS

Run timestamp: !`date +%Y%m%d-%H%M%S`

The run directory is `.nogit/verify-{run timestamp}/`, in the repository that owns the target.

Strip `--deep` out of `$ARGUMENTS` before reading the rest as the target. The review depth is `deep` when I pass it, and `standard` otherwise.

## Review

Read `${CLAUDE_PLUGIN_ROOT}/skills/verify/review.md` and follow it from start to end, with the target, this run directory and the review depth.

## Report

When the review finishes, print:

- the path of `report.md`
- the number of `must-fix`, `should-fix` and `nit` findings
- the number of dropped findings
- the number of open questions

Do not repeat the findings. I read the report myself.
