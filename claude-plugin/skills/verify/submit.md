# Verify: submit

This procedure turns `report.md` into a comment for the PR author. The `verify` skill runs it after the review and passes the run directory.

## 1. Check the findings

Read `{run directory}/report.md`. Read the code for each finding that is `checked: not checked`. Drop a finding when the code does not confirm it.

## 2. Write the comment

Write `{run directory}/submit.md`.

The file holds only the findings.
Leave out:

- a title, an intro, or a summary
- the names of the agents or skills that ran
- strengths, praise, or what is solid
- counts, severity totals, or an action plan
- the finding IDs and the **Not confirmed** and **Open questions** sections
- fixes, suggested changes, or ` ```suggestion ` blocks

Write each finding in this shape, worst first:

```markdown
### <short title>

`<path>:<line>`

<what is wrong, in one or two sentences>

<why it is a problem: the failure it causes or the rule it breaks>
```

Point out the problem only. Do not say how to fix it.
Separate findings with a blank line.
Write for the PR author. Follow the "How to write" rules in my CLAUDE.md.

If no finding survives step 1, write no file. Tell me there are no findings and stop.

## 3. Offer to publish

Give me the path of the file and the number of findings.
If the report's `pr` is `none`, stop here.
Otherwise, ask whether to open it as a PR comment on GitHub.

If I approve:

1. Copy the comment to the clipboard. Start with the line `:robot: says...`, then a blank line, then the file:
   `{ printf ':robot: says...\n\n'; cat {run directory}/submit.md; } | pbcopy`
2. Open the comment form: `gh pr comment <number> --web`.
3. Tell me the comment is on the clipboard, ready to paste.

If the sandbox blocks either command, give me this line to run myself:

```
! { printf ':robot: says...\n\n'; cat {run directory}/submit.md; } | pbcopy && gh pr comment <number> --web
```

> [!CAUTION]
> NEVER post the comment. Do not run `gh pr comment` without `--web`. Do not call the GitHub API to create a comment or a PR review. I review and submit it myself.
