---
name: walk-replay
description: Replay a walk-through session from the factory in the terminal. Prints the overview, then prints one step, verbatim, each time the user says "go ahead". It does not summarize the steps.
---

I want you to **replay a walk-through** that `/jam:walk-through` saved to the factory. Present the pre-written steps to me **one at a time**, exactly as they were written. Do **not** summarize, condense, or rephrase a step — I wrote these on purpose and I want to read them **as-is**. You are the pager here, not the author.

What I gave you: the arguments supplied with the invocation

Run `git rev-parse --show-toplevel 2>/dev/null || echo none` to get the worktree.

## Find the walk

A walk is a factory session with `flow: walk`. Read it only with the `jam-factory` tools `list_sessions`, `list_files` and `read_file`. Never write to it.

If the `jam-factory` tools are missing, tell me to run `just install` in the repository's `app/` folder. Then stop.

Resolve the arguments supplied with the invocation to one walk session, with its org, repo and branch. Every `list_sessions` call below passes `category: walk`.

- **A session id** (it starts with `walk-`, such as `walk-20261104-0912-3f00486`): call `list_sessions` with `include_archived: true` and no `org`, `repo` or `branch`. Use the entry whose `session` is the id, and take its `org`, `repo` and `branch`. If several entries match, list them as `{org}/{repo}/{branch}` and ask me to pick. Stop until I pick.
- **A slug** (such as `pr-247`, `commit-a1b2c3d` or `login-auth-flow`): call `list_sessions` with the worktree's org and repo. Use the walk with the newest `updated` whose `slug` in `data` is the slug.
- **Empty**: call `list_sessions` with the worktree's org and repo. Sort the walks by newest `updated` first. If there is one, use it. If there are several, list them and ask me to pick. Stop until I pick.

Only a slug and an empty argument use the worktree. Its folder names give the org and repo:

- **Repo**: the name of the worktree's parent folder.
- **Org**: the name of the folder above the repo folder. For the worktree `~/Sources/octonato/jam-factory/main`, the org is `octonato` and the repo is `jam-factory`.

If the worktree is `none`, this folder is not in a git repository. For a slug or an empty argument, say so, ask me for a full session id, and stop.

If nothing resolves, say so and stop. Don't invent steps.

## Know the running order

`state.md` in the plugin root's `skills/walk-through/` folder describes each file of a walk.

- The overview is `overview.md`.
- The steps are the entries of `steps` in `walk.json`, in `n` order.
- Step `n` lives in `steps/NN.md`, with `n` zero-padded to two digits: step 3 is `steps/03.md`.

Read `walk.json` and `overview.md` with `read_file`. If either is missing, the walk has no route yet: say so and stop.

That ordered list is the walk. Note how many steps there are so you can mark progress.

## Present the overview, then stop

Print `overview.md` **verbatim**. Don't add commentary before it and don't rewrite it.

Then print the itinerary from `walk.json`: one line per step, as `Step {n}: {title} — {summary}`.

Then add one short line: how many steps there are, and that I should say **"go ahead"** / **"next"** to see Step 1. Stop.

## Present each step, then stop

When I say **"go ahead"** / **"next"** (or similar), list the session's files with `list_files` and re-read `walk.json`. Then:

- **The step's file is listed**: read `steps/NN.md` with `read_file`. Print it **in full, verbatim**: its title, pointers, explanation, diff blocks and code blocks, exactly as written. No summary, no paraphrase, no "in short". You may help me open a pointer, but the step's own text must appear unaltered.
- **The step's file is not listed yet**: read `session.json`. If its `stage` is `blocked`, tell me its `blocked.reason` and stop. Otherwise, say the step is still being written and offer to wait. Check again when I say "go ahead".

**One exception: the file paths in the code pointers.** The steps were written with paths relative to the walk's worktree, and I sometimes run the replay from a **parent folder** (or another spot) where those relative paths no longer resolve. So you **may rewrite the path in a `path:line` pointer** to whatever is valid from my current working directory — keep the line number and everything else identical. This is the only thing you may change; the prose, titles, explanations, diff blocks and code blocks stay verbatim. If a path already resolves as-is, leave it untouched.

Close each step with a single navigational line — e.g. `Step 2 of 5 — 3 to go` — and stop. That marker is the only thing you add; it is not a summary of the step.

## How the replay proceeds

- When I **ask a question** about the current step, answer it — read the code the step points at if that helps — but **stay on this step** and don't advance. Answering a question is fine; replacing the step's text with your own summary is not.
- When I say **"go ahead"** / **"next"**, present the next step and stop.
- If I ask to **jump** (back to a step, or ahead to step N), print that step verbatim and continue from there.
- Keep track of where we are. If I lose the thread, restate the ordered step list with the current one marked.

## Closing

After the last step, stop. If I want a recap, I'll ask — this command **replays** what was pushed, it doesn't rewrite it.
