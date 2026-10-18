# Factory state files

The factory keeps all the state of one session in its session folder, `$FACTORY_ROOT/<repo>/sessions/<id>/`. A new conversation reads these files to pick up the work. The dashboard reads them too.

Write every file in the session folder with the **Write** or **Edit** tool, never with Bash. Re-read a file before you change it. I may have edited it by hand.

## Layout

```
<session folder>/
  session.json
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
    submit.md
```

`round-N/extras.md` lists the extra reviewer candidates and my decision on them. `skills/verify/review.md` defines its format.

A review session writes only `session.json`, `request.md` and the round folders. Its round folders add `submit.md`, the PR comment, and hold no `triage.md`. `skills/submit-review/submit.md` defines its format.

A vibe session writes only `session.json`, `request.md` and `tasks.md`.

## session.json

Only the factory skill writes it, with two exceptions. The dashboard's Archive button sets `archived` and `updated`. I may edit `mode` by hand. Rewrite the whole file on every change, and set `updated` to the current UTC time.

```json
{
  "id": "feature-1",
  "repo": "sdk",
  "worktree": "/Users/renato/Sources/akka/sdk/feature-1",
  "branch": "feature-1",
  "base": "main",
  "mode": "default",
  "flow": "build",
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
- `mode`: `default`, `autonomous` or `interactive`. The skill's **Modes** section says when each one asks me something. A missing field means `default`.
- `flow`: `build`, `review` or `vibe`. A missing field means `build`.
- `pr`: the PR number. Only review sessions set it.
- `stage`: `spec`, `plan`, `implement`, `verify`, `triage`, `done` or `blocked`. A review session uses `verify`, `submit`, `done` and `blocked`. A vibe session uses `implement`, `done` and `blocked`.
- `round`: the last verify round, `0` before the first.
- `lastRound`: `true` when triage picked `last round`. `-r` sets it back to `false`.
- `clarified`: `true` once a vibe session's Clarify step ran in `default` or `interactive` mode. Only vibe sessions set it. A missing field means `false`.
- `tasksOpen`, `tasksDone`: counted from `tasks.md`.
- `mustFix`, `shouldFix`, `nit`: the current round, after the triage filter. `0` before the first round.
- `dismissed`, `deferred`: the entries in `decisions.md` and `deferred.md`.
- `issued`: the count of entries in `deferred.md` with an issue URL. Recount it on every write. I set an entry's `issue` by hand.
- `blocked`: `null`, or `{ "stage": "<stage>", "reason": "<text>" }`.
- `archived`: `true` when the session is archived. The dashboard hides archived sessions. A missing field means `false`.

## request.md

```markdown
# Request: {id}

- source: {description | issue URL | PR URL | conversation}

{My description verbatim, the issue's title and body, the PR's title and body, or the summary of the conversation.}
```

A vibe session may append two sections:

- `## Clarifications`: the questions it asked me and my answers.
- `## Assumptions`: each choice the request left open that it made alone.

## tasks.md

```markdown
# Tasks: {id}

### T1: {title}
- source: {plan task 1 | round 1 F2 | request}
- status: {open | done | failed}
- commit: {none | {path}@{short hash}, one per git repository the task changed}

{What to do. A plan task holds the task's text from the plan. A fix task holds the finding's problem, location and suggested fix.}
```

Task IDs never change. New tasks get the next free number.

A vibe session holds one task, `T1`, with the source `request`. Its text points to `request.md`.

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
