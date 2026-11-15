# Factory state files

The factory keeps all the state of one session in its session folder. The `jam-factory` MCP server owns the folder. A new conversation reads these files to pick up the work. The dashboard reads them too.

## Tools

Read and write the session's files only with the tools of the `jam-factory` server. Never use the **Read**, **Write** or **Edit** tool or the shell on them. Claude Code names the tools `mcp__plugin_jam_jam-factory__<tool>`.

Each tool names the session with `org`, `repo`, `branch` and `session`. `branch` is the last segment of the branch name, such as `rate-limit` for `octonato/rate-limit`. `session` is the session id, such as `session-20261104-0912-3f00486`. A file tool adds `path`, relative to the session folder, such as `round-2/reviews/independent-review.md`.

| Tool | What it does |
|---|---|
| `start_session` | Creates the session folder and its `session.json`, and returns the new `session` id. Takes an optional `pr`, the PR number, only with `category: review`. See **Start a session**. |
| `list_sessions` | Lists sessions, each with `org`, `repo`, `branch`, `session`, `category` and its parsed `session.json` as `data`. Filters by `org`, `repo`, `branch` and `category`. Pass `include_archived: true` to list archived sessions too. |
| `read_file` | Returns the file's `content` and its `version`. |
| `list_files` | Returns the session's `files`, as relative paths. |
| `write_file` | Writes the whole file. Leave out `expected_version` to create a file. Pass the current version as `expected_version` to replace one. |
| `edit_file` | Replaces `old_str` with `new_str`. `old_str` must occur once in the file. |
| `append_file` | Adds `content` to the end of the file. Creates the file when missing. |
| `move_file` | Renames the file `from` to `to`. Fails when `to` exists. |
| `delete_file` | Deletes the file when its version is `expected_version`. |
| `update_session` | Merges `fields` into `session.json` and sets `updated`. |

- A file exists when `list_files` lists it. `read_file` of a missing file or session fails with `not_found`.
- The version is the SHA-256 of the file. Every write returns the new `version`.
- `write_file` without `expected_version` fails with `exists` when the file exists.
- A `conflict` error means the file changed since you read it. Re-read it, apply your change to the new content and retry.
- Writes create the missing folders inside the session. A write to a session that `start_session` did not create fails with `not_found`.
- An error reply holds a `code`, a `message` and the `current_version` on a conflict.

Re-read a file before you change it. I may have edited it by hand.

## Start a session

A branch holds one folder per session. The folder name is `{category}-{YYYYMMDD-HHMM}-{short sha}`, such as `review-20261104-0905-3f00486`. The prefix is the category:

- `session`: a build or vibe session of `/jam:take` or `/jam:just-vibe`.
- `review`: a review session of `/jam:review`.
- `walk`: a walk of `/jam:walk-through`.

A review with a PR number gets the folder name `review-pr{n}-{YYYYMMDD-HHMM}-{short sha}`, such as `review-pr42-20261104-0905-3f00486`.

Call `start_session` with `org`, `repo`, `branch`, `category`, `head` and `fields`, plus `pr` for a review of a PR:

- `head`: the short sha of HEAD, from `git rev-parse --short HEAD`.
- `pr`: the PR number, a positive integer. Only `category: review` takes it. Any other category with `pr` fails with `bad_name`.
- `fields`: the first fields of `session.json`. The tool adds `id`, `category`, `archived: false` and `updated`.

It returns `org`, `repo`, `branch`, the new `session` id and the `version` of `session.json`. Use that `session` id for every later call.

A branch holds at most one live `session` and one live `review`. A live session is one whose `session.json` does not hold `archived: true`. `start_session` fails with `exists` when the branch already holds a live session of the category, and its message names that session. A branch holds any number of walks.

To find the live session of a category, call `list_sessions` with `org`, `repo`, `branch` and `category`. It lists at most one session for `session` and `review`.

## Layout

The server keeps every session under its root: `root` in `~/.jam-factory/config.json`, or `~/.jam-factory/factory`.

A file of a session is written `factory:{org}/{repo}/{branch}/{session}/{path}`. On disk, it is `{root}/{org}/{repo}/sessions/{branch}/{session}/{path}`. A run directory in a session is written `factory:{org}/{repo}/{branch}/{session}/round-{N}/`.

```
{root}/{org}/{repo}/sessions/{branch}/{session}/
  session.json
  request.md
  spec.md
  plan.md
  tasks.md
  decisions.md
  deferred.md
  runs.jsonl
  feedback-pending.md          # only while staging a feedback import
  round-<N>/
    reviews/
      independent-review.md
      github-feedback.md       # imported PR feedback, build sessions only
      <reviewer>.md
    report.md
    extras.md
    triage.md
    submit.md
```

`round-N/extras.md` lists the extra reviewer candidates and my decision on them. `skills/verify/review.md` defines its format.

A review session writes only `session.json`, `request.md`, `runs.jsonl` and the round folders. Its round folders add `submit.md`, the PR comment, and hold no `triage.md`. `skills/review/submit.md` defines its format.

A vibe session writes only `session.json`, `request.md`, `tasks.md` and `runs.jsonl`.

A walk session writes only the files `skills/walk-through/state.md` lists.

## session.json

Only the factory skill writes it, with two exceptions. The dashboard's Archive button sets `archived` and `updated`. I may edit `mode` by hand.

Create it once with `start_session`, as **Start a session** says. After that, change it only with `update_session`. Pass only the fields that change. `update_session` replaces each field whole, merges `counts` per key and sets `updated`. To change `blocked` or `feedbackImport`, pass the whole object.

```json
{
  "id": "session-20261015-1402-3f00486",
  "category": "session",
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

- `id`: the session folder name. `start_session` sets it.
- `category`: `session` or `review`. `start_session` sets it.
- `branch`: the full name of the branch the session runs on.
- `repo`: the name of the worktree's parent folder.
- `worktree`: an absolute path.
- `mode`: `default`, `autonomous` or `interactive`. The skill's **Modes** section says when each one asks me something. A missing field means `default`.
- `flow`: `build`, `review`, `vibe` or `walk`. A missing field means `build`. `skills/walk-through/state.md` defines a walk session.
- `pr`: the PR number. Only review sessions set it.
- `reviewedHead`: the full sha of HEAD when the last round's report was written. Only review sessions set it.
- `stage`: `spec`, `plan`, `implement`, `verify`, `triage`, `done` or `blocked`. A review session uses `verify`, `submit`, `done` and `blocked`. A vibe session uses `implement`, `done` and `blocked`. A walk session uses `walk`, `done` and `blocked`.
- `round`: the last verify or feedback round, `0` before the first.
- `lastRound`: `true` when triage picked `last round`. A new verify or feedback round resets it to `false`; an import resume preserves the recorded outcome.
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

## runs.jsonl

Each line is one JSON object: one agent run or one main-conversation stage.

```json
{"stop":"implement","round":1,"agent":"jam:implementer","task":"T3","model":"opus","started":"2026-10-22T18:02:11Z","ended":"2026-10-22T18:03:12Z","durationMs":60110,"tokens":35550,"toolUses":23,"file":null,"outcome":"done"}
```

| Field | Type | Meaning |
|---|---|---|
| `stop` | string, required | `request`, `spec`, `plan`, `implement`, `review`, `report`, `triage`, `comment`, `walk` |
| `round` | integer, required | the open round when the run started, or `0` before round 1 |
| `agent` | string | the agent name, or `main` for the main conversation |
| `task` | string | the task ID, for an Implement run |
| `model` | string | the model the run used, as the skill knows it |
| `started`, `ended` | UTC ISO-8601 string | from `date -u +%Y-%m-%dT%H:%M:%SZ` |
| `durationMs` | integer | only the duration the CLI reports; `null` when unavailable |
| `tokens` | integer | the total tokens the CLI reports for the run |
| `toolUses` | integer | the tool uses the CLI reports |
| `file` | string | the artifact the run wrote, relative to the session folder, such as a review or report |
| `outcome` | string | `done`, `failed` or `blocked` |

- Only append lines. Never change or remove a line.
- Add each line with `append_file`, ending with a newline. The first line creates the file.
- Every field other than `stop` and `round` may be missing or `null`.
- Write `null` for a field the CLI does not report. Never estimate a value.
- Store only a reported `durationMs`; write `durationMs: null` when the runtime reports none, including main-conversation stages. Never store a duration calculated from timestamps. The dashboard may derive positive elapsed time from valid `started` and `ended` values for display. Request lines always store `durationMs: null`; the dashboard hides their timing.
- Take `started` right before the run or stage starts. Take `ended` right after it ends. On resume, use fresh timestamps for the current invocation; never invent timings across an interruption.
- `model`: for an agent, the model its frontmatter names or the model the skill passed it. For the main conversation, the model name the CLI states as its own.
- `outcome`: how the run or stage ended.

The open round is the round a line belongs to. Set `round` to it:

- `request`, `spec` and `plan`: `0`.
- `implement`: the `round` in `session.json` plus one.
- `review`, `report`, `triage` and `comment`: the `round` in `session.json`.
- Every line of a vibe or walk session: `0`.

## Feedback import checkpoint

Only build sessions use `feedbackImport`. Its value is null (a missing field also means null) or `{ "round": N, "phase": "normalize" | "triage" }`. While it is set, `round` is N and `stage` is `triage`. Resume routes through `feedback.md` before ordinary stage dispatch. Clear it in the same `update_session` call that applies triage's outcome. `round` counts both verify rounds and imported-feedback rounds. Each successful feedback import resets `lastRound` to false. Normal triage may set it true again.

`feedback-pending.md` is a staging snapshot, not a consumed-source ledger. A complete staging snapshot or an installed capture in `round-{round + 1}` is also a resume checkpoint, covering interruption before the session journal is written. The procedure in `feedback.md` validates and attaches it without allocating a second round. An installed capture is immutable. Only complete snapshots participate in dedup; failed fetches and calls with no unseen versions write nothing.

### round-N/reviews/github-feedback.md

Write one header JSON block, then one source JSON block under each `### GF1`, `### GF2`, ... heading. These headings provide stable report links. End with the literal `<!-- github-feedback-capture-complete -->` marker only after all declared source blocks. On read, require valid JSON, the exact source count and unique version tuples as well as the marker; a marker alone does not validate a partial capture. A staging snapshot uses this same format.

Header example:

```json
{
  "format": "jam-github-feedback-v1",
  "previousRound": 2,
  "round": 3,
  "host": "github.com",
  "repository": "org/repo",
  "pr": 42,
  "url": "https://github.com/org/repo/pull/42",
  "worktree": "/path/to/worktree",
  "branch": "feature-1",
  "head": "local-head-sha",
  "fetchedAt": "2026-10-18T12:00:00Z",
  "requestedMode": null,
  "sourceCount": 1
}
```

Source example under `### GF1`:

```json
{
  "ref": "GF1",
  "identity": ["github.com", "org/repo", 42, "issue-comment", "101"],
  "effectiveTimestamp": "2026-10-18T11:00:00Z",
  "bodyDigest": null,
  "version": ["github.com", "org/repo", 42, "issue-comment", "101", "2026-10-18T11:00:00Z", null],
  "reviewer": "reviewer-login",
  "created_at": "2026-10-18T10:00:00Z",
  "submitted_at": null,
  "updated_at": "2026-10-18T11:00:00Z",
  "url": "https://github.com/org/repo/pull/42#issuecomment-101",
  "path": null,
  "line": null,
  "record": {"id": 101, "body": "Original body; the actual capture keeps the full API record."}
}
```

`record` holds the entire unmodified GitHub object, including original body, author, review state, timestamps and URLs; the abbreviated example above only illustrates nesting. The wrapper repeats metadata for reading and indexing. Missing metadata is null. For inline comments, repeat available `path`, `line`, `side`, `start_line`, `start_side`, `original_line`, `original_start_line`, `original_position`, `position`, `commit_id`, `original_commit_id`, `pull_request_review_id` and `in_reply_to_id`; preserve all other fields in `record`. Do not replace an outdated line with an invented current line.

Kinds are `issue-comment`, `review` and `inline-comment`. `identity` includes host, repository, PR, kind and id so different endpoints cannot collide. The effective timestamp uses `updated_at`, else `submitted_at`, else `created_at`. When `updated_at` is absent, `bodyDigest` is the lowercase hexadecimal SHA-256 of these exact bytes: for a string, a single `0x01` byte followed by the original string's UTF-8 bytes; for null, the single `0x00` byte. Do not JSON-quote or escape the string, normalize Unicode, alter line endings, or append a newline. When `updated_at` exists, `bodyDigest` is null. Include it in `version` so edited review bodies with an unchanged submission timestamp still import once. Retain every unseen version, even non-actionable input. Dedup reads these structured tuples, never only report findings.

The normalized report follows `agents/report-writer.md`'s layout. Feedback questions use `Q` ids and the finding fields under Open questions. In feedback triage, substitute the actual `F` or `Q` id for `F{n}` in task sources and decision records. Preserve the full raw source references in each task, dismissed entry and deferred entry. Locations may be a verified path without a line or `PR-wide` when no current line is supported.
