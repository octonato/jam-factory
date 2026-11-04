# Walk session files

A walk session holds one walk-through that `/jam:walk-through` saved in auto mode. Jam Board shows it, and `/jam:walk-replay` replays it.

## Tools

Read and write the session's files only with the tools of the `jam-factory` server. `skills/take/state.md` lists them, with their versions and errors. A walk uses two more tools to put code into a step file:

| Tool | What it does |
|---|---|
| `append_diff` | Appends the git diff of `files` from `base` to `head` as a fenced `diff` block. With `lines`, it keeps only the hunks whose new side meets one of the `[start, end]` ranges. |
| `append_code` | Appends lines `start` to `end` of `file`, counted from 1 and inclusive, as a fenced code block. It reads the file in the worktree, or the file at `ref` when you pass one. |

- Both tools name the session with `org`, `repo` and `session`, and the step file with `path`, such as `drafts/03.md`.
- Both read `worktree` from `session.json`. `files` and `file` are paths relative to it.
- `append_diff` keeps a renamed file when `files` holds its old or its new path. Its block shows `rename from`, `rename to` and only the edited hunks.
- A path that is empty or absolute, or holds a `.` or `..` part, fails with `bad_path`.
- `append_diff` fails with `git_failed` when git fails. It fails with `empty_diff` when the diff is empty or no hunk meets `lines`.
- `append_code` fails with `git_failed` when the file is missing or git fails. It fails with `bad_range` when `start` is below 1, `end` is below `start`, or `end` is past the last line.
- A failed call writes nothing. A call that succeeds returns the new `version`.

## Layout

```
{root}/{org}/{repo}/sessions/{id}/
  session.json
  request.md
  walk.json
  overview.md
  drafts/NN.md
  steps/NN.md
  runs.jsonl
```

- `request.md`: the target.
- `walk.json`: the target, the summary and the itinerary.
- `overview.md`: the target line, the summary and the mechanical-churn note.
- `drafts/NN.md`: a step being written.
- `steps/NN.md`: a finished step.

`NN` is the step number, zero-padded to two digits: `01`, `02`, … `10`.

## session.json

Create it once with `write_file`, with `updated` set to the current UTC time. After that, change it only with `update_session`.

```json
{
  "id": "walk-pr-247-20261104-0912",
  "repo": "jam-factory",
  "worktree": "/Users/renato/Sources/octonato/jam-factory/main",
  "branch": "main",
  "flow": "walk",
  "stage": "walk",
  "counts": { "stepsPlanned": 7, "stepsDone": 3 },
  "blocked": null,
  "archived": false,
  "updated": "2026-11-04T09:15:02Z"
}
```

- `id`: `walk-{slug}-{timestamp}`. `{slug}` is `pr-{n}`, `commit-{short sha}` or a short kebab-case slug of the code path. `{timestamp}` is the local time as `YYYYMMDD-HHMM`.
- `repo`: the name of the worktree's parent folder.
- `worktree`: the absolute path where the walk ran. The tools read from it.
- `branch`: the branch where the walk ran, or `null` when HEAD is detached.
- `flow`: always `walk`.
- `stage`: `walk` while the skill writes the steps, then `done`. `blocked` on an error the skill cannot resolve.
- `stepsPlanned`: the steps in `walk.json`. `0` until `walk.json` exists.
- `stepsDone`: the files under `steps/`.
- `blocked`: `null`, or `{ "stage": "walk", "reason": "<text>" }`.
- `archived`: as `skills/take/state.md` says.

A walk session holds no `mode`, `base`, `round` or `lastRound`.

## request.md

```markdown
# Request: {id}

- source: {PR URL | commit range | code path}

{The PR's title, the commit range `{hash}^..HEAD`, or my description of the code path, verbatim.}
```

## walk.json

```json
{
  "mode": "changes",
  "target": { "kind": "pr", "pr": 247, "base": "<full sha>", "head": "<full sha>" },
  "summary": "Adds rate limiting to the gateway.",
  "steps": [
    { "n": 1, "title": "The limiter", "summary": "A token bucket per client key." }
  ]
}
```

- `mode`: `changes` or `code-path`.
- `target.kind`: `pr`, `range` or `code-path`. A `pr` target holds `pr`, the PR number. A `range` target holds `commit`, the hash as given. A `code-path` target holds `description`.
- `target.base`, `target.head`: the full SHAs of a changes target. Every diff in the walk uses them. A code-path target holds neither.
- `summary`: what the change does as a whole, or what the code path does.
- `steps`: the itinerary, in order. `n` counts from 1. `summary` is one or two sentences.

Write it once you have mapped the route. If a step's title or summary changes while you write the step, edit its entry with `edit_file`.

## overview.md

```markdown
# Walk-through: {slug}

{PR #{n}: {title} | Commit range `{hash}^..HEAD` | Code path: {description}}

{The summary from `walk.json`.}

{One line on the mechanical churn, when the change has any.}
```

## Step files

```markdown
## Step {n}: {title}

{The code pointers, as bare `path:line`.}

{The explanation.}

{A diff block from `append_diff`, or a code block from `append_code`.}

{More prose and blocks, in reading order.}
```

- `{n}` is the step's `n` in `walk.json`, not zero-padded.
- Keep every heading after the first at `###` or deeper.
- Build the step in `drafts/NN.md`. Move it to `steps/NN.md` with `move_file` when it is done. Then add one to `stepsDone`.
- Write prose with `append_file`. Write diff and code text only with `append_diff` and `append_code`.

## runs.jsonl

A walk appends two lines, in the format `skills/take/state.md` gives for `runs.jsonl`:

```json
{"stop":"request","round":0,"agent":"main","started":"2026-11-04T09:12:03Z","ended":"2026-11-04T09:12:03Z","durationMs":null,"outcome":"done"}
{"stop":"walk","round":0,"agent":"main","model":"opus","started":"2026-11-04T09:12:04Z","ended":"2026-11-04T09:31:40Z","durationMs":null,"tokens":null,"toolUses":null,"file":null,"outcome":"done"}
```

- `request`: append it right after you create `session.json`. Set `started` and `ended` to the current time.
- `walk`: append it when the walk ends. Take `started` before you map the route, and `ended` after the last write. Set `outcome` to `done`, or to `blocked` when the walk stopped on an error.
- `model`: the model name the CLI states as its own.
- Both lines log no tokens. Write `null` for `tokens`, `toolUses` and `durationMs`.
