---
name: submit-review
disable-model-invocation: true
description: Turns an /oc:verify report into a PR comment. The comment holds only the findings and their severity. Pass verify-{timestamp} to use a run on disk. Pass a target to run /oc:verify first. Pass nothing to pick a run from a list. It offers to open the comment on GitHub. It never posts the comment.
argument-hint: [verify-{timestamp} | PR number or URL | commit hash (inclusive) | range a..b | empty to pick a run]
allowed-tools: Bash(date *), Bash(find *), Bash(*/skills/submit-review/open-comment.sh *)
effort: xhigh
---

Write the findings of a /oc:verify run as a comment for the PR author. Change no code.

What I gave you: $ARGUMENTS

Run timestamp: !`date +%Y%m%d-%H%M%S`

Runs on disk:
!`find . -maxdepth 2 -type d -path './.nogit/verify-*'`

## Mode

- **`verify-{timestamp}`:** use the run in `.nogit/verify-{timestamp}/`. Ignore the run timestamp above. If `report.md` is missing there, tell me and stop.
- **Nothing:** pick a run, as **Pick a run** describes.
- **Anything else:** read it as the target. The run directory is `.nogit/verify-{run timestamp}/`, in the repository that owns the target.

## Pick a run

If no run is on disk, run a new verify on the current branch.

Otherwise, ask me with **AskUserQuestion**. Offer the three newest runs, newest first, then `Run a new verify on the current branch`. The names sort by time. Label each run with its directory name.

- **A run:** use it as if I passed its `verify-{timestamp}`.
- **A new verify:** read the target as empty. The run directory is `.nogit/verify-{run timestamp}/`.
- **Other:** read my text as the argument, and pick the mode again.

Read nothing under `.nogit/` outside the run directory.

## Review

Without a `verify-{timestamp}` argument, read `${CLAUDE_PLUGIN_ROOT}/skills/verify/review.md` and follow it from start to end, with the target and the run directory.

With a `verify-{timestamp}` argument, skip the review. Compare the report's `head` with the short hash of `HEAD` in the report's `repo`. If they differ, tell me both hashes and ask whether to continue. The locations may point at moved code.

## Submit

Read `${CLAUDE_PLUGIN_ROOT}/skills/submit-review/submit.md` and follow it from start to end, with the run directory.
