---
name: verify
description: Read-only review of a PR, a commit range or the current branch. Under 300 changed lines, it runs independent-reviewer or superpowers:requesting-code-review, picked at random. From 300 lines, it runs both. Then it offers the pr-review-toolkit reviewers the diff calls for as extras. It checks each finding against the code, and writes one report to .nogit/verify-{timestamp}/, or to the directory --out names. Changes no code.
---

Review my changes and write the findings to a report. Change no code.

What I gave you: the arguments supplied with the invocation

Run timestamp: the output of `date +%Y%m%d-%H%M%S`

Strip `--out <dir>` out of `the arguments supplied with the invocation` before reading the rest as the target. The run directory is `<dir>` when I pass it. Otherwise it is `.nogit/verify-{run timestamp}/`, in the repository that owns the target.

## Review

Read the extension root's `skills/verify/review.md` and follow it from start to end, with the target and this run directory. Run its step 4, **Extra reviewers**.

## Report

When the review finishes, print:

- the path of `report.md`
- the reviewers that ran
- the number of `must-fix`, `should-fix` and `nit` findings
- the number of dropped findings
- the number of open questions

Do not repeat the findings. I read the report myself.
