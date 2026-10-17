---
name: submit-review
description: Turns a $jam:verify report into a PR comment. The comment holds only the findings and their severity. It drops findings that comments on the PR already report. Pass verify-{timestamp} to use a run on disk. Pass a target to run $jam:verify first. Pass nothing to pick a run from a list. It offers to open the comment on GitHub. It never posts the comment.
allowed-tools: Bash(date *), Bash(find *), Bash(*/skills/submit-review/open-comment.sh *), Bash(*/skills/submit-review/fetch-comments.sh *)
---

Write the findings of a $jam:verify run as a comment for the PR author. Change no code.

What I gave you: the arguments supplied when invoking this skill

Run `date +%Y%m%d-%H%M%S` and use its output as the run timestamp.

Runs on disk:
Run `find . -maxdepth 2 -type d -path './.nogit/verify-*'` to list the available runs.

## Mode

- **`verify-{timestamp}`:** use the run in `.nogit/verify-{timestamp}/`. Ignore the run timestamp above. If `report.md` is missing there, tell me and stop.
- **Nothing:** pick a run, as **Pick a run** describes.
- **Anything else:** read it as the target. The run directory is `.nogit/verify-{run timestamp}/`, in the repository that owns the target.

## Pick a run

If no run is on disk, run a new verify on the current branch.

Otherwise, ask me with **request_user_input**. Offer the three newest runs, newest first, then `Run a new verify on the current branch`. The names sort by time. Label each run with its directory name.

- **A run:** use it as if I passed its `verify-{timestamp}`.
- **A new verify:** read the target as empty. The run directory is `.nogit/verify-{run timestamp}/`.
- **Other:** read my text as the argument, and pick the mode again.

Read nothing under `.nogit/` outside the run directory.

## Review

Without a `verify-{timestamp}` argument, read the plugin root's `skills/verify/review.md` and follow it from start to end, with the target and the run directory.

With a `verify-{timestamp}` argument, skip the review. Compare the report's `head` with the short hash of `HEAD` in the report's `repo`. If they differ, tell me both hashes and ask whether to continue. The locations may point at moved code.

## Submit

Read the plugin root's `skills/submit-review/submit.md` and follow it from start to end, with the run directory.
