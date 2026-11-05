---
name: walk-through
description: Walk-through of a code path or a set of changes, broken into logical steps. By default it saves the walk as a factory session, and Jam Board shows each step as soon as it is written; add -i/--interactive to walk through it one step at a time in the terminal instead.
argument-hint: [a code path/behavior, OR a commit hash, OR a PR number] [-i|--interactive]
allowed-tools: Bash(git rev-parse *), Bash(git branch --show-current), Bash(date *), mcp__plugin_jam_jam-factory
---

I want to **understand code by its logical steps**, not as a wall of text. By default, work autonomously and **save the steps to a factory session** so I can read them on Jam Board at my own pace; with `-i` you instead **guide me through them one at a time** in the terminal.

What I gave you: $ARGUMENTS

Worktree: !`git rev-parse --show-toplevel`

Current branch: !`git branch --show-current`

HEAD: !`git rev-parse --short HEAD`

If `$ARGUMENTS` is empty, ask me what I want to discover — a code path, a commit hash, or a PR number — and stop. Don't start a walk-through without a target.

## First, decide what we're walking

Look at `$ARGUMENTS` and pick the mode:

- **A commit hash or git ref** (a hex sha like `a1b2c3d`, or something like `HEAD~3`) → **Changes mode.** Walk me through the code changes from that commit through `HEAD`. Per my rules the hash is **inclusive**, so the range is `<hash>^..HEAD` — the parent of the hash through HEAD. Gather the diff with `git diff <hash>^ HEAD` and the commit context with `git log <hash>^..HEAD`.
- **A PR number** (e.g. `247` or `#247`) → **Changes mode.** Walk me through that PR's changes. Resolve `base` and `head` as **Resolve the target** says. Gather the diff with `git diff {base} {head}` and the context with `gh pr view <number>`.
- **Anything else** (a description of a behavior or code path) → **Code-path mode.** Trace the live flow through the code, as below.

If you can't tell which mode `$ARGUMENTS` means, ask me before doing any research.

## Flags

Strip the flag out of `$ARGUMENTS` before interpreting the rest as the target. The target itself never starts with a lone `-`, so there's no ambiguity.

- **`-i` / `--interactive`** — turn on **interactive mode**: instead of writing files, walk me through the steps one at a time in the terminal. Without it, the default is **auto mode** (save the walk to a factory session).

## Then, map the route (quietly)

In auto mode, resolve the target and create the session first, as **Auto mode** says.

Before presenting anything, work out the route so every pointer you give me is a real file and line, never invented.

- **Code-path mode:** trace the actual code path with **Explore**/Grep/Read. Follow the real flow — entry point → the calls it makes → where it ends.
- **Changes mode:** read the **full diff** and the commit message(s) / PR description as one body of work — **not commit by commit**. The commits are just how the work was recorded; I want to understand the change by its **logical steps**, which rarely line up with the commit boundaries. Build a model of what the change accomplishes as a whole, then separate the **substantive** changes (the new behavior, the real design decisions) from the **mechanical** ones (renames propagated across call sites, signature changes threaded through, import shuffles, formatting — edits made only to keep the code compiling). Order the substantive changes into a sensible **reading sequence** — the core change first, then what builds on it, tests last — and fold the mechanical churn into a brief mention rather than its own steps. Read the surrounding current code where you need to so each pointer lands on a real, present-day line.

**Do not dump this research on me.** The research is for *you* to plan the route. What I see is the guided walk, not the transcript.

## Interactive mode (`-i`): present the itinerary, then stop

**This section and everything below it up to `Auto mode (default)` — the itinerary, the per-step walk, and the closing recap — is the interactive walk and applies only in interactive mode (`-i`). In the default auto mode, skip all of it and jump to `Auto mode (default): save the walk to the factory`.**

Once you've mapped it, show me a short **numbered list of the steps** — one line each, naming what that step covers. This is the map of where we're going. Then present **Step 1** and stop.

Keep the itinerary tight: enough steps to follow the path (or the change) without skipping anything important, but each step a sensible unit to digest in one read. In changes mode, the steps are **logical units of the change, not commits** — give me a one-line summary of what the change accomplishes as a whole before the step list, so I have the destination in mind, and if there's meaningful mechanical churn, note it in one line so I know it exists without it eating a step.

## Shape of each step

For the current step:

- **Title it** — `Step N: <what this step covers>`.
- **Point me at the code** — give the concrete locations to open, as bare relative paths with line numbers in plain text (e.g. `src/main/scala/foo/Bar.scala:42`), following my citation rules. These are the pointers *I* navigate to. In changes mode, cite the **post-change** line so I open the file at its current state.
- **Walk me through what's there** — explain what this code does and how it connects to the previous step and the next one. In changes mode, explain what the change does, why it's there, and how it serves the overall goal of the PR/commit — not just the mechanics of the diff. A few focused paragraphs at most. This is a guided tour, not a full exposition — surface what matters at this stop and leave the rest for my questions.
- **Hand the pen back** — end by inviting me to ask questions about this step, or to say "go ahead" / "next" to move on. Close with a one-line progress marker showing how many steps remain (e.g. `Step 2 of 5 — 3 to go`).

## How the walk proceeds

- When I **ask a question**, elaborate on the current step — go deeper, read more code if needed — but **stay on this step**. Don't advance.
- When I say **"go ahead"**, **"next"**, or similar, present the **next step** and stop again.
- If I ask to **jump** (back to a step, or ahead), go there.
- Keep track of where we are. If I lose the thread, restate the itinerary with the current step marked.

## Closing

After the final step, give me a short **recap** — the path (or the set of changes) we walked end to end, in the order we saw it, so I leave with the whole shape in my head.

## Auto mode (default): save the walk to the factory

This mode runs when I don't pass `-i`. Do not walk me through the steps. Map the same route, save the walk as a factory session and print where it is. I read it on Jam Board, or replay it with `/jam:walk-replay`, and come back with questions later.

If the `jam-factory` tools are missing, tell me to run `just install` in the `app/` folder of the jam-factory repository. Then stop.

Read `${CLAUDE_PLUGIN_ROOT}/skills/walk-through/state.md` before you write any file. It holds the tools and the format of every file of a walk.

- **Worktree**: the value above.
- **Branch**: the last segment of the current branch above. For `octonato/rate-limit`, it is `rate-limit`. The walk runs on any branch, the base branch included.- **Repo**: the name of the worktree's parent folder.
- **Org**: the name of the folder above the repo folder. For the worktree `~/Sources/octonato/jam-factory/main`, the org is `octonato` and the repo is `jam-factory`.
- **Slug**: `pr-{n}` for a PR, `commit-{short sha}` for a commit hash or ref, or a short kebab-case slug of the topic for a code path, such as `login-auth-flow`. It names the walk in `overview.md` and in `session.json`.
- **Session id**: the id `start_session` returns, such as `walk-20261104-0912-3f00486`.
- **Session**: the session named by `org`, `repo`, `branch` and `session id`. Every file path below, such as `walk.json`, is relative to the session.

If the current branch is empty, HEAD is detached. Tell me to check out a branch, or to pass `-i`, and stop.

Each run creates a new session. A walk has no resume.

After you create `session.json`, each change to it, such as "set `stage: done`", is an `update_session` call.

### Resolve the target

Resolve the target before you write any file. If a command fails here, tell me why and stop.

- **A commit hash or ref**: `base` is `git rev-parse {hash}^`. `head` is `git rev-parse HEAD`.
- **A PR**: run `gh pr view {n} --json url,title,baseRefName,headRefOid`. The remote is `upstream` when `git remote` lists it, otherwise `origin`. If `git cat-file -e {headRefOid}^{commit}` fails, the head commit is not local: run `git fetch {remote} pull/{n}/head`. Run `git fetch {remote} {baseRefName}`. `base` is `git merge-base {remote}/{baseRefName} {headRefOid}`. `head` is `headRefOid`.
- **A code path**: there is no `base` or `head`.

In changes mode, map the route from `git diff {base} {head}` and `git log {base}..{head}`. For a PR, read its description with `gh pr view {n}` too. Cite each line as it is at `head`.

### Write the walk

Write the files in this order. `state.md` shows the format of each one.

1. Call `start_session` with `category: walk` and HEAD as `head`, as `skills/take/state.md` says. Pass `fields` with the repo, the worktree, the full current branch and the slug. Set `flow: walk`, `stage: walk`, `stepsPlanned` and `stepsDone` to `0` and `blocked: null`. Use the returned `session` as the session id.
2. Create `request.md` with `write_file`.
3. Append the `request` line to `runs.jsonl` with `append_file`. Then take the `started` time of the `walk` line.
4. Map the route, as **Then, map the route** says.
5. Create `walk.json` and `overview.md` with `write_file`. Set `stepsPlanned` to the number of steps.
6. For each step, in order:
   1. Create `drafts/NN.md` with `append_file`, starting with `## Step N: {title}`. Add prose with `append_file`, and code with `append_diff` or `append_code`.
   2. Move `drafts/NN.md` to `steps/NN.md` with `move_file`.
   3. Add one to `stepsDone`.
7. Set `stage: done`. Append the `walk` line to `runs.jsonl` with `outcome: "done"`.

If a step's title or summary changes while you write it, edit its entry in `walk.json` with `edit_file`.

A step carries the content of **Shape of each step**: the code pointers as bare `path:line` and the explanation. Leave out the interactive tail: no "hand the pen back", no "go ahead" invitation and no progress marker. Keep every heading after the first at `###` or deeper.

**Never write diff or code text yourself.** The tools copy it from git and the worktree:

- **Changes mode**: give each change the step explains an `append_diff` block, with `base`, `head` and the changed files. Name a renamed file by its old or its new path: the block shows the rename and only the edited hunks. When the step covers only part of a file, pass `lines`: the ranges the step covers on the `head` side.
- **Code-path mode**: give each stop on the path an `append_code` block, with the file and its lines in the worktree.

A failed tool call writes nothing. Fix its arguments and call it again. An `empty_diff` with `lines` means no hunk meets the ranges.

### End

Print:

- the session, as `factory:{org}/{repo}/{branch}/{session id}/`
- the step count
- each step's title, one per line

Add one line: I can read it on Jam Board, or replay it with `/jam:walk-replay {session id}`. Say nothing else. Keep the mapped route in context. When I ask about a step, answer in the terminal, on that step. Don't change the session's files unless I ask you to.

On an error you cannot resolve after `session.json` exists, set `stage: blocked` and `blocked: { "stage": "walk", "reason": "<the reason>" }`. Append the `walk` line to `runs.jsonl` with `outcome: "blocked"`. Tell me the reason and stop.
