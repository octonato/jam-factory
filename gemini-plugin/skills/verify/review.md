# Verify

This procedure reviews a target and writes the findings to `report.md` in the run directory. The caller gives the target and the run directory. The caller may also give a problem statement.

Read nothing under `.nogit/` outside the run directory. When `$FACTORY_ROOT` is set, read nothing under it outside the run directory.

## 1. Resolve the target

The target is one of:

- **A PR number or URL.** Run `gh pr view <pr> --json number,title,headRefName,baseRefName`. The diff is `gh pr diff <pr>`. Check that the current branch is the PR's head branch. If it is not, ask me whether to run `gh pr checkout <pr>`. Run it only if I approve and `git status` shows a clean tree. Otherwise stop.
- **A commit hash.** The hash is inclusive: review `<hash>^..HEAD`. If the current repository lacks the hash, check the sibling repositories. Run `git -C <dir> cat-file -e <hash>^{commit}` in each. Use the repository that owns it. If none owns it, tell me and stop.
- **A range `a..b`.** Review it as given.
- **Nothing.** Review the current branch against its base. The remote is `upstream` when `git remote` lists it, otherwise `origin`. Take the base from `gh pr view --json baseRefName` when the branch has a PR. Otherwise use the remote's default branch. Run `git fetch <remote> <base>`. The diff is `git diff <remote>/<base>...HEAD`.

Find the PR for the branch with `gh pr view --json number` when the target is not a PR. Record `none` if there is no PR.

If the diff is empty, say `Nothing to review.` and stop.

If `git status` shows uncommitted changes, tell me they are not part of the review.

Run `mkdir -p {run directory}/reviews`.

## 2. Run the reviewers

Each reviewer runs as an agent on a fresh context. Pass it only what this step lists. Pass nothing else from this conversation.

Dispatch the reviewers **in parallel**, in the same message, over the same diff.

The base reviewers:

1. `independent-reviewer`: an independent review against the original problem
2. `superpowers:requesting-code-review`: a code review against the requirements

The extra reviewers, from `pr-review-toolkit`. They run only when I pick them in step 4:

3. `pr-review-toolkit:code-reviewer`: bugs, logic errors and project guideline violations
4. `pr-review-toolkit:pr-test-analyzer`: missing tests and weak test coverage
5. `pr-review-toolkit:silent-failure-hunter`: swallowed errors and bad fallbacks
6. `pr-review-toolkit:comment-analyzer`: comments that are wrong, stale or misleading
7. `pr-review-toolkit:type-design-analyzer`: types that fail to express or enforce their invariants
8. `pr-review-toolkit:code-simplifier`: code that could be simpler

### Pick the base reviewers

Count the changed lines: the insertions plus the deletions that `git diff --shortstat` prints for the target.

- **Under 300 lines**: run one base reviewer, picked at random. Run `echo $((RANDOM % 2))`. On `0`, run `independent-reviewer`. On `1`, run `superpowers:requesting-code-review`.
- **300 lines or more**: run both.

Record each reviewer with one line on why it runs. For example: `random pick, 42 changed lines`, or `342 changed lines`. The report lists them under **Reviewers**.

### List the extra reviewer candidates

Read the diff. A reviewer is a candidate for each row that matches:

| The diff | Candidate |
|---|---|
| adds or changes tests | `pr-test-analyzer` |
| changes production code with no test change | `pr-test-analyzer` |
| adds or changes types, records or classes | `type-design-analyzer` |
| adds or changes error handling, retries or fallbacks | `silent-failure-hunter` |
| adds or changes comments or docs | `comment-analyzer` |
| adds more than about 200 lines of production code | `code-simplifier` |
| changes production code | `code-reviewer` |

Write `{run directory}/extras.md` in this layout. Add one line per candidate, with the reason from its row. If no row matches, write no candidate lines and set `decision: none`.

```markdown
# Extra reviewers

- decision: pending

## Candidates

- [ ] pr-test-analyzer: changes production code with no test change
- [ ] code-reviewer: changes production code
```

### Dispatch

Run each reviewer with the **agent** tool. For reviewers 1 and 3 to 8, use its registered agent name. If the name is not available, run the built-in generalist agent. Give that agent the reviewer's focus from the list above as its task.

The problem statement is the one the caller gives. If the caller gives none, write one as Step 2 of `../independent-reviewer/SKILL.md` describes.

Give `independent-reviewer`:

- the target and the commands to read it
- the problem statement
- the `file` output mode
- `{run directory}/reviews/` as the folder to write into

For reviewer 2, load the `superpowers:requesting-code-review` skill with the skill system. Use only its `code-reviewer.md` template. Skip its steps on acting on feedback. Dispatch the built-in generalist agent with the template's prompt. Fill the placeholders:

- `DESCRIPTION`: the PR title, or the commit subjects in the range
- `PLAN_OR_REQUIREMENTS`: the problem statement
- `BASE_SHA` and `HEAD_SHA`: the two ends of the diff range. For a PR or the current branch, `BASE_SHA` is the merge base.

Add to its prompt:

- do not read, list or search anything under `.nogit/`, or under `$FACTORY_ROOT` when it is set
- the **Finding rules** section below, word for word

Tell reviewers 3 to 8:

- the scope in one sentence: the PR number, or the commit range
- to return located findings, each with a severity, a location and a suggested fix
- the **Finding rules** section below, word for word
- not to edit or write any file
- not to read, list or search anything under `.nogit/`, or under `$FACTORY_ROOT` when it is set

`code-simplifier` edits code by default, so tell it to report its suggestions as findings.

### Save each review

`independent-reviewer` writes `reviews/independent-review.md` itself. Save every other review the moment its reviewer returns. Don't wait for the others. Write its output verbatim to `{run directory}/reviews/{reviewer}.md`. Use the name after the plugin prefix as `{reviewer}`. For example: `requesting-code-review.md`, `pr-test-analyzer.md`.

Wait for all reviewers to finish. Then check that `reviews/` holds one file per reviewer that ran.

### Finding rules

Keep a finding only if:

1. **The diff adds or changes it.** Code that only sits next to the diff, or a gap that predates the change, is not a finding.
2. **This layer owns it.** The fix belongs in the repository and module under review. Drop a finding whose fix belongs in a dependency, a runtime or another service.
3. **Sibling code does not do the same.** Read the sibling code first. If it uses the same convention, drop the finding.
4. **It names a concrete failure.** Say what goes wrong, for whom, and when. A preference for another design is not a failure.

Accept the design of the change. Report a design concern that fails rule 4 as a question, not a finding.

Judge the change only against the rules in the target repository's `GEMINI.md` and `AGENTS.md`. Ignore the rules in `~/.gemini/GEMINI.md`. When a finding cites a rule, name the file that holds the rule.

The severities are:

- `must-fix`: data loss, a security hole, a crash on a common path, or a break that stops the build.
- `should-fix`: wrong behaviour, a missed requirement, a compatibility break or a weakened test. Also use it when the evidence that the change works has a gap.
- `nit`: everything else that passes the rules above.

Give evidence for each `must-fix` and `should-fix` finding: a concrete failing scenario, a mutation the tests miss, or command output.

Give the full path from the repository root on the first mention of a file. After that, use the file name and the line. These rules override the citing rules in `GEMINI.md`.

## 3. Write the report

Run the `report-writer` agent with the **agent** tool. If the name is not available, find its definition under the plugin's `agents/` folder and run the built-in generalist agent with that file's body as the prompt. Pass it only:

- the target and the commands to read it
- the run directory
- the reviewers that ran, each with why it ran
- the **Finding rules** section above, word for word

Pass nothing else from this conversation. The agent merges the reviews, checks each finding against the code, and writes `{run directory}/report.md`.

When it finishes, check that `report.md` exists. If it does not, tell me and stop.

## 4. Extra reviewers

Run this step only when the caller asks for it. `extras.md` holds the decision. Re-read it first: I may have edited it.

- **`decision: none`**: skip this step.
- **`decision: pending`**: show me the report's counts per severity. Then ask me with the user which candidates to run, with `multiSelect: true`. Offer one option per candidate, with its reason as the description. Split them over several questions when there are more than four. Tick the ones I pick in `extras.md`. Set `decision: chosen` when I pick any, and `decision: none` otherwise.
- **`decision: chosen`**: run the ticked candidates.

To run them, dispatch each ticked candidate that has no file in `reviews/` yet, as **Dispatch** and **Save each review** describe. Record each one with the reason `picked as an extra`. Then run step 3 again with every reviewer that ran. The new `report.md` replaces the old one.

