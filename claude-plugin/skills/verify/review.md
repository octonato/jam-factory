# Verify

This procedure reviews a target and writes the findings to `report.md` in the run directory. The caller gives the target, the run directory and the review depth. The depth is `deep` when the caller passes `--deep`, and `standard` otherwise.

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

Dispatch the reviewers **in parallel**, in the same message, over the same diff.

Every depth runs these two:

1. `oc:independent-reviewer`: an independent review against the original problem
2. `superpowers:requesting-code-review`: a code review against the requirements

The `deep` depth adds these six:

3. `pr-review-toolkit:code-reviewer`: bugs, logic errors and project guideline violations
4. `pr-review-toolkit:pr-test-analyzer`: missing tests and weak test coverage
5. `pr-review-toolkit:silent-failure-hunter`: swallowed errors and bad fallbacks
6. `pr-review-toolkit:comment-analyzer`: comments that are wrong, stale or misleading
7. `pr-review-toolkit:type-design-analyzer`: types that fail to express or enforce their invariants
8. `pr-review-toolkit:code-simplifier`: code that could be simpler

Run each reviewer with the **Agent** tool and `effort: "xhigh"`. For reviewers 1 and 3 to 8, set `subagent_type` to its name. If the name is not available, run a **general-purpose** agent. Give that agent the reviewer's focus from the list above as its task.

Give `oc:independent-reviewer`:

- the target and the commands to read it
- a problem statement, written as Step 2 of `../independent-reviewer/SKILL.md` describes
- the `file` output mode
- the run directory

For reviewer 2, load the `superpowers:requesting-code-review` skill with the **Skill** tool. Use only its `code-reviewer.md` template. Skip its steps on acting on feedback. Dispatch a **general-purpose** agent with the template's prompt. Fill the placeholders:

- `DESCRIPTION`: the PR title, or the commit subjects in the range
- `PLAN_OR_REQUIREMENTS`: the same problem statement you give `oc:independent-reviewer`
- `BASE_SHA` and `HEAD_SHA`: the two ends of the diff range. For a PR or the current branch, `BASE_SHA` is the merge base.

Add to its prompt:

- do not read, list or search anything under `.nogit/`
- the **Finding rules** section below, word for word

In step 3, map its `Critical` issues to `must-fix`, `Important` to `should-fix` and `Minor` to `nit`.

Tell reviewers 3 to 8:

- the scope in one sentence: the PR number, or the commit range
- to return located findings, each with a severity, a location and a suggested fix
- the **Finding rules** section below, word for word
- not to edit or write any file
- not to read, list or search anything under `.nogit/`

`code-simplifier` edits code by default, so tell it to report its suggestions as findings.

Wait for all reviewers to finish.

### Finding rules

Keep a finding only if:

1. **The diff adds or changes it.** Code that only sits next to the diff, or a gap that predates the change, is not a finding.
2. **This layer owns it.** The fix belongs in the repository and module under review. Drop a finding whose fix belongs in a dependency, a runtime or another service.
3. **Sibling code does not do the same.** Read the sibling code first. If it uses the same convention, drop the finding.
4. **It names a concrete failure.** Say what goes wrong, for whom, and when. A preference for another design is not a failure.

Accept the design of the change. Report a design concern that fails rule 4 as a question, not a finding.

Judge the change only against the rules in the target repository's `CLAUDE.md` and `AGENTS.md`. Ignore the rules in `~/.claude/CLAUDE.md`. When a finding cites a rule, name the file that holds the rule.

The severities are:

- `must-fix`: data loss, a security hole, a crash on a common path, or a break that stops the build.
- `should-fix`: wrong behaviour, a missed requirement, a compatibility break or a weakened test. Also use it when the evidence that the change works has a gap.
- `nit`: everything else that passes the rules above.

Give evidence for each `must-fix` and `should-fix` finding: a concrete failing scenario, a mutation the tests miss, or command output.

Give the full path from the repository root on the first mention of a file. After that, use the file name and the line. These rules override the citing rules in `CLAUDE.md`.

## 3. Merge

- Merge findings that share one root cause. Keep the clearest wording and list all sources.
- Sort by severity, worst first, then by file and line.
- Number them `F1`, `F2`, and so on in that order. The IDs never change after this step.

The severities are `must-fix`, `should-fix` and `nit`. Map other scales to the closest one.

Add the questions from `oc:independent-reviewer` under **Open questions**.

## 4. Check and triage

Read the code for every finding. Then apply the **Finding rules**.

- If the code does not confirm the finding, move it under **Dropped** with what the code shows.
- If the finding fails a rule, move it under **Dropped** with the number of the rule it fails.
- If a design concern fails rule 4 but still matters, add it under **Open questions** too.
- If a `must-fix` or `should-fix` finding has no evidence, find the evidence or lower it to `nit`.

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
- depth: {standard | deep}
- run: {run directory}

## Findings

### F1: {short title}
- severity: **{must-fix | should-fix | nit}**
- location: {full path from the repository root}:{line}
- source: **{reviewer names}**

{The problem, in one to three sentences.}

**Evidence:** {the failing scenario, the mutation and its result, or the command output. Only for must-fix and should-fix.}

**Suggested fix:** {one short paragraph}

## Dropped

- F7: {title}: {what the code shows | fails rule {n}: {why}}

## Open questions

- Q1 (F4): {question}
```

Follow the "How to write" rules in my CLAUDE.md. Cite files as the **Finding rules** say. Ignore the citing rules in my CLAUDE.md.
