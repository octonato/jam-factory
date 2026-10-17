---
name: autonomously
description: Delegate a task for autonomous execution — work through phases, committing each, without waiting for review
---

You are **explicitly delegated to execute this task autonomously**. This is the exception my AGENTS.md refers to: for this task, and only this task, you may commit and advance through phases without stopping for my approval.

The task is: the arguments supplied when invoking this skill

If `the arguments supplied when invoking this skill` is empty, ask me what to work on and stop — do not enter autonomous mode without a task.

## Scope of this authorization

- Autonomy applies **only to the task above**. The moment it's finished, you stop.
- You never push.
- My **next instruction that does not come through `/autonomously` returns you to normal mode** — phase-by-phase, present for review, and **you do not commit** (I commit). Don't carry autonomous behavior into it.

## Plan

Divide the task into small, ordered phases as my AGENTS.md requires. Show me the phases in a short list, one line per phase. Then start the loop. Do not wait for an answer.

## Loop

Take each phase in order. For each one:

### 1. Implement

- Implement only this phase.
- Match the style, naming and idioms of the code around the change.
- Keep logical sections separated by blank lines.
- If you must add temporary or incomplete code to keep the build green, mark it with a `FIXME` comment.

### 2. Check the writing

Dispatch both reviewers **in parallel**, in the same message, over the **uncommitted changes**: staged, unstaged, and untracked files from `git status`.

1. `jam:comment-reviewer`: content of added or edited code comments
2. `jam:proof-reader`: English of added or edited comments and documentation

Use the **collaboration** tools with `task name` set to the agent's name. If the name isn't available, find its definition under the plugin's `agents/` folder and run a **general-purpose** agent with that file's body as the prompt. Tell each agent the scope in one sentence: `uncommitted changes`. Don't add instructions of your own.

Wait for both to finish. Merge the results into one list:

- Sort by file, then line.
- If both reviewers flag the same `file:line`, keep one entry. Join the two rules with `;`. If either fix is `delete`, the merged fix is `delete`.

If the list is not empty, hand the findings to `jam:fixer` in one subagent call, verbatim and numbered. Don't apply the edits yourself. Keep the findings it skipped for the report. Don't re-run the checks.

### 3. Test

Build, then run the tests that cover the changed code and the project's formatter check. Wait for the result before you go on.

If the build, the tests or the formatter check fail, fix the cause and run them again. If you cannot make them pass, stop. See **When to stop early**.

### 4. Commit

The project may hold subprojects, each with its own git repository. Find every repository the phase changed: run `git status --porcelain` in the working directory, and in each folder below it that holds a `.git` entry. Commit in each one.

In each repository, stage only the files the phase changed. Never stage `.nogit/`.

Write the commit message for each repository. I squash the commits of a task into its first one, so the first commit carries the final message.

- **Phase 0** is the first commit this task makes in that repository. It describes the whole task, not only this phase.
  - The subject is a Conventional Commit: `{type}({scope}): {summary}`, in the imperative, under 72 characters. The type is one of `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci` and `chore`. The scope is optional.
  - The body explains the whole change for a reader who has not seen this session.
- **Later commits** in that repository have the subject `phase N: {what the phase does}`, in the imperative, under 72 characters. `N` counts this task's commits in that repository, from 1. The body explains this phase only.

For every message:

- Do not repeat the subject in the body.
- Use a list in the body when the change has several separate parts.
- Put code names in backticks: classes, methods, fields, files and commands.
- Follow the plain-English rules in my AGENTS.md.
- Add no `Co-Authored-By` line.

Commit with `git commit --no-gpg-sign -F {message file}`. Write the message file under `$TMPDIR`, and delete it after the commit. I'll squash and sign the commits myself on review.

Then take the next phase.

## When to stop early

Stop on a failure you can't resolve. Examples: the build or the tests fail after your fixes, a commit fails, or `git` reports a conflict. Leave the work in a clear state, and report what failed and what you tried. Do not push past a blocker or paper over it.

## Report

When all phases are committed, show me:

- the commits, one line each: short hash and subject
- the writing findings the fixer skipped
- any `FIXME` left in the code, and any deviation from the plan

The authorization ends here.
