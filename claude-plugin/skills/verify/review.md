# Verify

This procedure reviews a target and writes the findings to `report.md` in the run directory. The caller gives the target and the run directory.

Read nothing under `.nogit/` outside the run directory.

## 1. Resolve the target

The target is one of:

- **A PR number or URL.** Run `gh pr view <pr> --json number,title,headRefName,baseRefName`. The diff is `gh pr diff <pr>`. Check that the current branch is the PR's head branch. If it is not, ask me whether to run `gh pr checkout <pr>`. Run it only if I approve and `git status` shows a clean tree. Otherwise stop.
- **A commit hash.** The hash is inclusive: review `<hash>^..HEAD`. If the current repository lacks the hash, check the sibling repositories. Run `git -C <dir> cat-file -e <hash>^{commit}` in each. Use the repository that owns it. If none owns it, tell me and stop.
- **A range `a..b`.** Review it as given.
- **Nothing.** Review the current branch against its base. The remote is `upstream` when `git remote` lists it, otherwise `origin`. Take the base from `gh pr view --json baseRefName` when the branch has a PR. Otherwise use the remote's default branch. Run `git fetch <remote> <base>`. The diff is `git diff <remote>/<base>...HEAD`.

Find the PR for the branch with `gh pr view --json number` when the target is not a PR. Record `none` if there is no PR.

If the diff is empty, say `Nothing to review.` and stop.

If `git status` shows uncommitted changes, tell me they are not part of the review.

Run `mkdir -p {run directory}`.

## 2. Run the reviewers

Each reviewer runs as an agent on a fresh context. Pass it only what this step lists. Pass nothing else from this session.

Dispatch these seven reviewers **in parallel**, in the same message, over the same diff:

1. `pr-review-toolkit:code-reviewer`: bugs, logic errors and project guideline violations
2. `pr-review-toolkit:pr-test-analyzer`: missing tests and weak test coverage
3. `pr-review-toolkit:silent-failure-hunter`: swallowed errors and bad fallbacks
4. `pr-review-toolkit:comment-analyzer`: comments that are wrong, stale or misleading
5. `pr-review-toolkit:type-design-analyzer`: types that fail to express or enforce their invariants
6. `pr-review-toolkit:code-simplifier`: code that could be simpler
7. `oc:independent-reviewer`: an independent review against the original problem

Run each reviewer with the **Agent** tool. Set `subagent_type` to its name and `effort` to `"xhigh"`. If the name is not available, run a **general-purpose** agent with the same effort. Give that agent the reviewer's focus from the list above as its task.

Tell reviewers 1 to 6:

- the scope in one sentence: the PR number, or the commit range
- to return located findings, each with a severity, a location as `path:line` and a suggested fix
- not to edit or write any file
- not to read, list or search anything under `.nogit/`

`code-simplifier` edits code by default, so tell it to report its suggestions as findings.

Give `oc:independent-reviewer`:

- the target and the commands to read it
- a problem statement, written as Step 2 of `../independent-reviewer/SKILL.md` describes
- the `file` output mode
- the run directory

Wait for all seven to finish.

## 3. Merge

- Merge findings that point at the same problem. Keep the clearest wording and list all sources.
- Sort by severity, worst first, then by file and line.
- Number them `F1`, `F2`, and so on in that order. The IDs never change after this step.

The severities are `critical`, `high`, `medium` and `low`. Map other scales to the closest one.

## 4. Check the findings

Read the code for every `critical` and `high` finding, and for any other finding you doubt.

- If the code confirms the finding, set `checked: confirmed`.
- If the code does not confirm it, move it under **Not confirmed** with what the code shows.
- If you did not read the code for it, set `checked: not checked`.

Some findings raise a behaviour question that only I can answer. For example: "should a cancelled stream still record the interaction?" Add each one under **Open questions**, linked to its finding.

## 5. Write the report

Write `{run directory}/report.md` in this layout. Keep the field names exactly as shown. Leave a section empty when it has no entries, and keep its heading.

```markdown
# Verify: {target}

- target: {PR #5726 | 6e77038^..HEAD | 6e77038..a1b2c3d | feature/foo vs main}
- repo: {absolute path of the repository}
- branch: {local branch}
- head: {short hash of HEAD}
- pr: {number | none}
- run: {run directory}

## Findings

### F1: {short title}
- severity: **{critical | high | medium | low}**
- location: {path:line}
- source: **{reviewer names}**
- checked: **{confirmed | not checked}**

{The problem, in one to three sentences.}

**Suggested fix:** {one short paragraph}

## Not confirmed

- F7: {title}: {what the code shows}

## Open questions

- Q1 (F4): {question}
```

Follow the "How to write" rules in my CLAUDE.md.
