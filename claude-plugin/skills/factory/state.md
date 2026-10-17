# Factory state files

The factory keeps all the state of one job in its job folder, `$FACTORY_ROOT/<repo>/jobs/<id>/`. A new session reads these files to pick up the work. The dashboard reads them too.

Write every file in the job folder with the **Write** or **Edit** tool, never with Bash. Re-read a file before you change it. I may have edited it by hand.

## Layout

```
<job folder>/
  job.json
  request.md
  spec.md
  plan.md
  tasks.md
  decisions.md
  deferred.md
  round-<N>/
    reviews/
      independent-review.md
      <reviewer>.md
    report.md
    extras.md
    triage.md
```

`round-N/extras.md` lists the extra reviewer candidates and my decision on them. `skills/verify/review.md` defines its format.

## job.json

Only the factory skill writes it. The one exception is the dashboard's Archive button, which sets `archived` and `updated`. Rewrite the whole file on every change, and set `updated` to the current UTC time.

```json
{
  "id": "feature-1",
  "repo": "sdk",
  "worktree": "/Users/renato/Sources/akka/sdk/feature-1",
  "branch": "feature-1",
  "base": "main",
  "stage": "triage",
  "round": 2,
  "lastRound": false,
  "counts": {
    "tasksOpen": 0, "tasksDone": 7,
    "mustFix": 1, "shouldFix": 2, "nit": 3,
    "dismissed": 4, "deferred": 1, "issued": 0
  },
  "blocked": null,
  "archived": false,
  "updated": "2026-10-15T14:02:11Z"
}
```

- `id`: the last segment of the branch.
- `repo`: the name of the worktree's parent folder.
- `worktree`: an absolute path.
- `stage`: `spec`, `plan`, `implement`, `verify`, `triage`, `done` or `blocked`.
- `round`: the last verify round, `0` before the first.
- `lastRound`: `true` when I chose `last round` at triage.
- `tasksOpen`, `tasksDone`: counted from `tasks.md`.
- `mustFix`, `shouldFix`, `nit`: the current round, after the triage filter. `0` before the first round.
- `dismissed`, `deferred`: the entries in `decisions.md` and `deferred.md`.
- `issued`: the entries in `deferred.md` with an issue URL.
- `blocked`: `null`, or `{ "stage": "<stage>", "reason": "<text>" }`.
- `archived`: `true` when the job is archived. The dashboard hides archived jobs. A missing field means `false`.

## request.md

```markdown
# Request: {id}

- source: {description | issue URL}

{My description verbatim, or the issue's title and body.}
```

## tasks.md

```markdown
# Tasks: {id}

### T1: {title}
- source: {plan task 1 | round 1 F2}
- status: {open | done | failed}
- commit: {none | {path}@{short hash}, one per git repository the task changed}

{What to do. A plan task holds the task's text from the plan. A fix task holds the finding's problem, location and suggested fix.}
```

Task IDs never change. New tasks get the next free number.

`{path}` is the git repository's path relative to the worktree, and `.` for the worktree itself. For example: `commit: .@a1b2c3d, sdk@9f8e7d6`.

## decisions.md

```markdown
# Dismissed: {id}

### D1: {finding title}
- round: {N} (F{n})
- location: {path:line, as the report gave it}

{The problem, as the report stated it.}

**Reason:** {my reason}
```

## deferred.md

```markdown
# Deferred: {id}

### E1: {finding title}
- round: {N} (F{n})
- location: {path:line, as the report gave it}
- issue: {URL | none}

{The problem, as the report stated it.}

**Suggested fix:** {as the report gave it}
```

## round-N/triage.md

```markdown
# Triage: round {N}

- F1: fix → T5
- F2: dismiss → D3
- F3: defer → E1
- F4: auto-dismissed: matches D1
- F5: auto-deferred: matches E1

outcome: {next round | last round | finalize}
```
