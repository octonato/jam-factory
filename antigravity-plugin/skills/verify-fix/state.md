# Verify-fix state file

The `verify-fix` skill keeps all its state for one run in a single Markdown file. A new session reads this file to pick up the work.

## Location

- The state file is `.nogit/verify-{timestamp}/fix.md`, in the repository that owns the target.
- The run directory also holds verify's `report.md`, the `reviews/` folder with one file per reviewer, and the commit message file `fix.commit-msg`.
- `--resume verify-{timestamp}` names the run directory. If `fix.md` is there, continue it. If only `report.md` is there, build `fix.md` from it. If both are missing, tell me and stop.

Read nothing under `.nogit/` outside the run directory, apart from the run list in `SKILL.md`.

Re-read the state file before each phase. I may have edited it by hand.

## Layout

The file has these sections, in this order. Keep the field names exactly as shown. The skill finds entries by their headings and field names.

```markdown
# Verify-fix: {target}

- target: {PR #5726 | 6e77038^..HEAD | 6e77038..a1b2c3d | feature/foo vs upstream/main}
- repo: {absolute path of the repository}
- branch: {local branch}
- remote: {remote}/{branch}
- pr: {number | none}
- run: {run directory}
- mode: {auto | interactive}
- gpg: {unknown | available | unavailable}

## Findings

### F1: {short title}
- severity: **{must-fix | should-fix | nit}**
- location: {path:line}
- source: **{reviewer names}**
- status: **{open | left out | deferred | blocked | fixed}**

{The problem, in one to three sentences.}

**Evidence:** {copied from the report. Only for must-fix and should-fix.}

**Suggested fix:** {one short paragraph}

## Plan

### P1: {finding title}
- findings: {F1}
- status: {planned | in review | committed}
- commit: **{short hash | none}**
- subject: {commit subject | none}
- tests: {command} @ {base short hash} → {passed | failed | not run}

## Left out

- F7: {reason}

## IDE refactors

- {rename, move or extract, with a location}

## Open questions

- Q1 (F4): {question} — {open | answered: the answer}
```

Leave a section empty when it has no entries. Keep its heading.

## Build it from the report

After verify, build `fix.md` from `report.md`:

- Copy the header fields. Add `remote`, `mode` and `gpg: unknown`.
- Copy each finding under **Findings** with `status: open`.
- Copy each finding under **Dropped** into **Findings** with `status: left out`. Add it under **Left out** with the reason `dropped: {the reason in the report}`.
- Copy each open question under **Open questions** as `open`. Set its finding to `status: deferred`.

## Statuses

A finding's `status` means:

- `open`: planned and not fixed yet
- `left out`: not planned, with a reason under **Left out**
- `deferred`: waiting on an open question
- `blocked`: a fix was tried and failed, and its changes were discarded
- `fixed`: committed

A phase's `status` means:

- `planned`: not started
- `in review`: implemented and waiting for my approval, in `interactive` mode only
- `committed`: committed

A phase's `tests` field names the test command and the base: `HEAD` when the run started. The result is `passed`, `failed`, or `not run`.
