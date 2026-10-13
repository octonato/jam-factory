# Submit review

This procedure turns `report.md` into a comment for the PR author. The `submit-review` skill runs this procedure with the run directory.

If `{run directory}/submit.md` exists, use it as it is. Skip steps 1 and 2, and go to step 3.

## 1. Pick the findings

Read `{run directory}/report.md`. Take every `must-fix` and `should-fix` finding under **Findings**.

Take at most 10 `nit` findings. When there are more, keep the 10 that matter most to the PR author.

## 2. Write the comment

Write `{run directory}/submit.md`.

The file holds only the findings, their severity and their evidence.
Leave out:

- a title, an intro, or a summary
- the names of the agents or skills that ran
- the `source` field
- strengths, praise, or what is solid
- counts, severity totals, or an action plan
- the finding IDs and the **Dropped** and **Open questions** sections
- fixes, suggested changes, or ` ```suggestion ` blocks

Write the `must-fix` findings first, then the `should-fix` findings, then the `nit` findings. Write each one in this shape:

```markdown
### [<severity>] <short title>

`<full path from the repository root>:<line>`

<what is wrong, in one or two sentences>

<why it is a problem: the failure it causes or the rule it breaks>

<the evidence: the code or output that shows the problem, for must-fix and should-fix only>
```

Point out the problem only. Do not say how to fix it.
Separate findings with a blank line.
Write for the PR author. Follow the "How to write" rules in my CLAUDE.md.
Give a file's full path the first time you mention it. After that, give the file name and line. These rules override the citing rules in my CLAUDE.md.

If step 1 picks no finding, write no file. Tell me there are no findings and stop.

## 3. Offer to publish

Give me the path of the file and the number of findings.
If the report's `pr` is `none`, stop here.
Otherwise, ask whether to open it as a PR comment on GitHub.

If I approve, run this command with `dangerouslyDisableSandbox: true`:

```
${CLAUDE_PLUGIN_ROOT}/skills/submit-review/open-comment.sh <number> {run directory}/submit.md
```

The script copies the comment to the clipboard and opens the comment form.
Then tell me the comment is on the clipboard, ready to paste.

If the command fails, give me the same command to run myself, prefixed with `! `.

> [!CAUTION]
> NEVER post the comment. Do not run `gh pr comment` without `--web`. Do not call the GitHub API to create a comment or a PR review. I review and submit it myself.
