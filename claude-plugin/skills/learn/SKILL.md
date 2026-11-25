---
name: learn
description: Generate a whole track of HTML lessons on a topic and save it as a factory lesson session. Jam Board shows each lesson as soon as it is written. Add -a/--autonomously to skip the questions and the outline approval.
argument-hint: <topic> [-a|--autonomously]
allowed-tools: Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *), mcp__plugin_jam_jam-factory
---

Adapted from the `teach` skill of https://github.com/mattpocock/skills (MIT). See `LICENSE`.

I want to learn a topic through a track of short HTML lessons. Write the whole track in one run, one lesson after the other, and save it as a factory lesson session. I read it on Jam Board and ask my questions here.

What I gave you: $ARGUMENTS

Worktree: !`git rev-parse --show-toplevel`

Current branch: !`git branch --show-current`

HEAD: !`git rev-parse --short HEAD`

Read `${CLAUDE_PLUGIN_ROOT}/skills/learn/state.md` before you write any file. It holds the tools, the layout of the session and the format of every file.

## Philosophy

To learn a topic at a deep level, I need three things:

- **Knowledge**, captured from high-quality, high-trust resources.
- **Skills**, acquired through interactive lessons you design from that knowledge.
- **Wisdom**, which comes from working with other learners and practitioners.

Find high-quality resources before you write the first lesson. Never trust your parametric knowledge.

Some topics need more skills than knowledge. Theoretical physics leans on knowledge. Yoga leans on skills.

### Fluency vs storage strength

Keep two kinds of learning apart:

- **Fluency strength**: in-the-moment retrieval of knowledge.
- **Storage strength**: long-term retention of knowledge.

Fluency can give me a false sense of mastery. Storage strength is the real goal. Design lessons that build long-term retention through desirable difficulty:

- Retrieval practice: recall from memory.
- Spacing: practice spread over time. Later lessons bring back the ideas of earlier ones.
- Interleaving: mix different but related topics in practice. Use it for skills practice only.

## Lessons

A lesson is the main thing you produce. It is the unit in which knowledge and skills reach me. Each lesson is one self-contained HTML file.

Make each lesson **beautiful**, with clean, readable typography and layout. I come back to the lessons later to review them. Think Tufte.

Keep each lesson short, so I finish it quickly. My working memory is very small, and the lesson must fit in it. Give each lesson a single tangible win that the next lesson builds on. Tie it to the mission, and keep it in my zone of proximal development.

Never open a lesson with a CLI command. Jam Board shows it.

Link each lesson to the other lessons and the reference pages it builds on, with HTML anchors.

Recommend one primary source in each lesson for me to read or watch. Pick the highest-quality, highest-trust resource you found on its subject.

End each lesson with a box that reminds me to ask follow-up questions in Claude Code. You can help with anything that is unclear. Name no skill in the box.

## The Mission

Tie every lesson to the mission: the reason I want to learn the topic. `mission.md` holds it.

In `default` mode, when the request leaves the reason unclear, ask me why I want to learn this before you write `mission.md`. In `autonomous` mode, derive the mission from the request, the code in the worktree and the earlier lesson sessions.

Without a clear mission, the knowledge has no anchor in real-world goals. The lessons feel too abstract, and you have no way to judge what comes next.

Change the mission only when I ask.

## Zone of proximal development

Each lesson should challenge me just enough.

I may name the exact thing I want to learn. When I don't, find my zone of proximal development from:

- the mission,
- the request,
- the earlier lesson sessions of this repo: what their missions aim at and what their tracks cover.

Start the track with the most relevant thing that fits in that zone. Build each lesson on the ones before it.

## Knowledge

Design each lesson around a skill I am going to learn. Put in only the knowledge that skill needs. Present the knowledge first. Then make me practise the skill through an interactive feedback loop.

Gather the knowledge from trusted resources first. Track them in `resources.md`. Fill each lesson with citations: links to outside sources that back each claim. They make the lesson trustworthy.

For acquiring knowledge, difficulty is the enemy. It eats the working memory I need to understand.

## Skills

Knowledge is about acquisition. Skills are about durability and flexibility. Make the knowledge stick.

For acquiring skills, difficulty is the tool. Effortful retrieval builds storage strength. Build skills through interactive lessons:

- Lessons with quizzes and light in-browser tasks.
- Lessons that guide me through a list of real-world steps, such as yoga poses or commands to run in the worktree.

Base each one on a **feedback loop** that tells me how I did. Keep the loop tight: give feedback at once, and automatically when you can.

For quizzes, give each answer the same number of words, and the same number of characters when you can. Vary the position of the right answer across questions. Give no clue to the answer through formatting or order.

## Acquiring Wisdom

Wisdom comes from real-world practice, outside the lessons.

When I ask a question that needs wisdom, try to answer it. Then point me to a **community**.

A community is a place, online or offline, where I can test my skills in the real world: a forum, a subreddit, a real-world class or a local interest group.

Find communities with a high reputation that I can join, and list them in `resources.md`. If I say I don't want to join a community, respect it.

## Reference Documents

Write reference pages next to the lessons. Lessons link to them. They hold the raw units of knowledge that several lessons use.

I rarely revisit a lesson, but I come back to the reference pages. Make each one the compressed essence of the lessons, laid out for quick reference. Make it beautiful, and make it print well.

Some topics lend themselves to reference:

- Syntax and code snippets for programming.
- Algorithms and flowcharts for processes.
- Poses and sequences for yoga.
- Exercises and routines for fitness.
- Glossaries for any topic with its own nomenclature.

Plan the reference pages with the outline, in `track.json`.

### Glossary pages

A glossary is the canonical language of the track. Pick its terms with the outline, and use them in every lesson.

A glossary page holds a `{Topic} Glossary` title, one or two sentences on the topic it covers, then the terms. Each term holds its name, its definition and the aliases to avoid:

```
**Span**:
One timed operation inside a trace, with a name, a start and an end.
_Avoid_: event, step
```

- **Add a term only when a lesson explains it.** The glossary records compressed knowledge. It is not a dictionary I read to learn.
- **Be opinionated.** When several words name the same concept, pick the best one. List the others as aliases to avoid.
- **Keep definitions tight.** One or two sentences. Define what the term is, not what it does or how to use it.
- **Use the glossary's own terms inside definitions.** Once a term is in the glossary, use it everywhere, other definitions included.
- **Group terms under subheadings** when natural clusters emerge. A flat list is fine when the terms cohere.
- **Flag ambiguities.** When the field uses a term loosely, state the meaning it has in this track.

## Writing rules

- Cite source code as bare `path:line` text, relative to the worktree, such as `app/store/src/sessions.rs:166`. Never make it a link. Jam Board turns it into one that opens the file in Zed or IntelliJ.
- Write lesson prose by the plain-English rules in my `CLAUDE.md`.
- Never give a link a `target` attribute. The board's sandboxed frame cannot open `target="_blank"` links.

## Assets

Build lessons from reusable **components** in `assets/`: stylesheets, quiz widgets, simulators, diagram helpers, and anything else a second lesson could reuse.

The session starts with two components: `assets/course.css`, the shared stylesheet, and `assets/quiz.js`, the quiz widget. Read both before the first lesson. The shared stylesheet makes the track look like one course. Write each quiz in the markup that the header comment of `quiz.js` shows.

Reuse is the default. Before you write a lesson, list the files in `assets/` and build from what is there. When a lesson needs something new and reusable, write it as a component in `assets/` and link to it. Never inline code that a later lesson would duplicate.

## Flags

Strip the flag out of `$ARGUMENTS`. The rest is the topic. The topic never starts with a lone `-`.

- **`-a` / `--autonomously`**: ask me nothing. Skip the questions and the outline approval. It sets `mode: autonomous`. Without it, `mode` is `default`.

## Steps

- **Worktree**: the value above.
- **Branch**: the last segment of the current branch above. For `octonato/rate-limit`, it is `rate-limit`.
- **Repo**: the name of the worktree's parent folder.
- **Org**: the name of the folder above the repo folder. For the worktree `~/Sources/octonato/jam-factory/main`, the org is `octonato` and the repo is `jam-factory`.
- **Slug**: a short kebab-case slug of the topic, such as `tracing`.
- **Session id**: the id `start_session` returns, such as `lesson-20261124-1800-00bab69`.
- **Session**: the session named by `org`, `repo`, `branch` and `session id`. Every file path below, such as `track.json`, is relative to the session.

Each "write" below is a `write_file` call, and each move a `move_file` call. After you create `session.json`, each change to it, such as "set `stage: done`", is an `update_session` call.

1. Resolve the worktree, the org, the repo and the branch. The skill runs on any branch, the base branch included. If the current branch is empty, HEAD is detached: tell me to check out a branch and stop.
2. If the topic is empty, ask me for it and stop.
3. If the `jam-factory` tools are missing, tell me to run `just install` in the `app/` folder of the jam-factory repository. Then stop.
4. Call `start_session` with `category: lesson` and HEAD as `head`, as `skills/take/state.md` says. Pass `fields` with the repo, the worktree, the full current branch, the slug and the mode. Set `flow: lesson`, `stage: lesson`, `counts` with `lessonsPlanned` and `lessonsDone` at `0`, and `blocked: null`. Use the returned `session` as the session id.
5. Write `request.md` with the topic verbatim. Append the `request` line to `runs.jsonl` with `append_file`. Then take the `started` time of the `lesson` line.
6. Read the earlier lesson sessions: call `list_sessions` with `org`, `repo` and `category: lesson`, and no `branch`. Read each one's `mission.md` and `track.json`. Skip this session, and skip a file that does not exist.
7. Research the topic. Read the code in the worktree that it touches. Find high-trust sources. Write `resources.md`.
8. In `default` mode, ask me a few questions, only where the why or my prior knowledge is unclear. Skip this step in `autonomous` mode.
9. Write `mission.md`. Write `track.json` with the outline. Set `lessonsPlanned` to the number of lessons.
10. In `default` mode, show me the mission and the outline once: the lesson titles, each with its one-line summary. Apply my changes to `mission.md` and `track.json`, and to `lessonsPlanned`, until I approve. Skip this step in `autonomous` mode.
11. Write `index.html`, the track's landing page.
12. For each lesson, in order:
    1. Write `drafts/NNNN-slug.html`.
    2. Move it to `lessons/NNNN-slug.html`.
    3. Add one to `lessonsDone`.

    If a lesson's title changes while you write it, edit its entry in `track.json` and `index.html` with `edit_file`.
13. Write each reference page that `track.json` lists.
14. Set `stage: done`. Append the `lesson` line to `runs.jsonl` with `outcome: "done"`.
15. Print:
    - the session, as `factory:{org}/{repo}/{branch}/{session id}/`
    - the lesson count
    - each lesson's title, one per line

    Add one line: I can read the track on Jam Board, and ask my questions here. Say nothing else. When I ask about a lesson, answer in the terminal. Don't change the session's files unless I ask you to.

On an error you cannot resolve after `session.json` exists, set `stage: blocked` and `blocked: { "stage": "lesson", "reason": "<the reason>" }`. Append the `lesson` line to `runs.jsonl` with `outcome: "blocked"`. Tell me the reason and stop.

Each run creates a new session. A lesson session has no resume.
