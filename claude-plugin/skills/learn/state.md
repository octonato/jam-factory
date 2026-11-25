# Lesson session files

A lesson session holds one track of HTML lessons that `/jam:learn` wrote. Jam Board shows it.

## Tools

Read and write the session's files only with the tools of the `jam-factory` server. `skills/take/state.md` lists them, with their versions and errors.

## Layout

```
{root}/{org}/{repo}/sessions/{branch}/{id}/
  session.json
  request.md
  mission.md
  resources.md
  track.json
  index.html
  assets/course.css
  assets/quiz.js
  assets/*
  drafts/NNNN-slug.html
  lessons/NNNN-slug.html
  reference/*.html
  runs.jsonl
```

- `request.md`: the topic.
- `mission.md`: why I want to learn the topic.
- `resources.md`: the trusted sources for the topic.
- `track.json`: the track's summary, its lessons and its reference pages.
- `index.html`: the track's landing page.
- `assets/course.css`: the shared stylesheet. `start_session` writes it.
- `assets/quiz.js`: the quiz widget. `start_session` writes it.
- `assets/*`: the other components the skill adds.
- `drafts/NNNN-slug.html`: a lesson being written.
- `lessons/NNNN-slug.html`: a finished lesson.
- `reference/*.html`: a reference page, such as a glossary.

`NNNN` is the lesson number, zero-padded to four digits: `0001`, `0002`, … `0010`. `slug` is a short kebab-case slug of the lesson title.

## session.json

Create it once with `start_session`, as `skills/take/state.md` says. After that, change it only with `update_session`.

```json
{
  "id": "lesson-20261124-1800-00bab69",
  "category": "lesson",
  "slug": "tracing",
  "repo": "jam-factory",
  "worktree": "/Users/renato/Sources/octonato/jam-factory/main",
  "branch": "main",
  "mode": "default",
  "flow": "lesson",
  "stage": "lesson",
  "counts": { "lessonsPlanned": 10, "lessonsDone": 3 },
  "blocked": null,
  "archived": false,
  "updated": "2026-11-24T18:10:00Z"
}
```

- `id`: `lesson-{YYYYMMDD-HHMM}-{short sha}`, the UTC minute and HEAD when the track started. `start_session` sets it.
- `category`: always `lesson`. `start_session` sets it.
- `slug`: a short kebab-case slug of the topic.
- `repo`: the name of the worktree's parent folder.
- `worktree`: the absolute path where the skill ran. Code citations are relative to it.
- `branch`: the full name of the branch where the skill ran. The session folder sits under its last segment.
- `mode`: `default` or `autonomous`.
- `flow`: always `lesson`.
- `stage`: `lesson` while the skill writes, then `done`. `blocked` on an error the skill cannot resolve.
- `lessonsPlanned`: the lessons in `track.json`. `0` until `track.json` exists.
- `lessonsDone`: the files under `lessons/`.
- `blocked`: `null`, or `{ "stage": "lesson", "reason": "<text>" }`.
- `archived`: as `skills/take/state.md` says.

A lesson session holds no `pr`, `base`, `round` or `lastRound`.

## request.md

```markdown
# Request: {id}

- source: topic

{The topic, verbatim.}
```

## mission.md

`mission.md` captures the reason I want to learn the topic. Every choice of the track traces back to it: what each lesson covers, which resources it cites and which exercises it holds.

```markdown
# Mission: {Topic}

## Why
{1-3 sentences. The concrete real-world goal I am chasing. What changes in my work when I have this skill? Avoid abstract framings like "to understand X"; name the underlying outcome.}

## Success looks like
- {A specific, observable thing I will be able to do}
- {Another specific thing}
- {…}

## Constraints
- {Time, prior knowledge, learning preferences, anything that bounds the approach}

## Out of scope
- {Adjacent topics I do not want to chase now}
```

- **One mission per session.** Two unrelated topics are two sessions.
- **Concrete over abstract.** "Ship a Rust CLI to my team" beats "learn Rust."
- **Keep it short.** A mission longer than one screen has turned into a plan.

## resources.md

`resources.md` is the curated set of trusted sources for the topic. Draw the knowledge of every lesson from it, not from parametric guesses. Wisdom comes from the communities it lists.

```markdown
# {Topic} Resources

## Knowledge

- [{Kind}: {title} by {author}]({url})
  {What it covers.} Use for: {when to reach for it}.

## Wisdom (Communities)

- [{Community}]({url})
  {What it is.} Use for: {when to reach for it}.

## Gaps

- {An area the mission needs that no good resource covers.}
```

- **High-trust only.** Prefer primary sources, specifications, official docs, recognised experts and peer-reviewed work. Leave out marketing dressed as education.
- **Annotate every entry.** Add one line: what it covers and when to reach for it.
- **Group by Knowledge and Wisdom.** A resource need not appear in both groups.
- **Name the gaps.** Add `## Gaps` only when the mission needs an area that no good resource covers.
- **Prune.** Leave out a resource that turned out wrong, shallow or off-mission. Five sharp sources beat thirty mediocre ones.
- **Record community preferences.** If I say I don't want to join a community, note it here.

## track.json

```json
{
  "summary": "How a trace forms in the runtime and the SDK, and where a user reads it.",
  "lessons": [
    { "n": 1, "title": "What is a trace?", "summary": "Spans, parents, waterfalls.", "file": "lessons/0001-what-is-a-trace.html" }
  ],
  "reference": [
    { "title": "Tracing glossary", "file": "reference/tracing-glossary.html" }
  ]
}
```

- `summary`: what the track covers, in one sentence.
- `lessons`: the lessons, in reading order. `n` counts from 1. `summary` is one sentence. `file` is the lesson's path under `lessons/`, with `NNNN` the zero-padded `n`.
- `reference`: the reference pages, in the order the Track stop shows them. `file` is the page's path under `reference/`.
- Each lesson needs an integer `n`, a `title` and a `file`. Each reference page needs a `title` and a `file`.

Jam Board draws one stop for the track, one per lesson and one per reference page. A stop opens its file as soon as the file exists.

## Lesson files

`index.html`, each lesson and each reference page is one self-contained HTML file.

- `index.html` shows the topic, the track's summary, each lesson with its title and summary, and the reference pages. It links `assets/course.css`, each lesson as `lessons/NNNN-slug.html` and each reference page as `reference/{name}.html`.
- A lesson links `../assets/course.css` and `../assets/quiz.js`, other lessons by file name, such as `0002-context.html`, and reference pages as `../reference/{name}.html`.
- A reference page links `../assets/course.css`, lessons as `../lessons/NNNN-slug.html` and other reference pages by file name.
- Every link to a file stays inside the session folder. No link points to another session.
- Source code is cited as bare `path:line` text, relative to the worktree, such as `app/store/src/sessions.rs:166`. It is never an `<a>` link.

## runs.jsonl

A lesson session appends two lines, in the format `skills/take/state.md` gives for `runs.jsonl`:

```json
{"stop":"request","round":0,"agent":"main","started":"2026-11-24T18:00:03Z","ended":"2026-11-24T18:00:03Z","durationMs":null,"outcome":"done"}
{"stop":"lesson","round":0,"agent":"main","model":"opus","started":"2026-11-24T18:00:04Z","ended":"2026-11-24T18:41:20Z","durationMs":null,"tokens":null,"toolUses":null,"file":null,"outcome":"done"}
```

- `request`: append it right after you create `request.md`. Set `started` and `ended` to the current time.
- `lesson`: append it when the run ends. Take `started` before you read the earlier lesson sessions, and `ended` after the last write. Set `outcome` to `done`, or to `blocked` when the run stopped on an error.
- `model`: the model name the CLI states as its own.
- Both lines log no tokens. Write `null` for `tokens`, `toolUses` and `durationMs`.
