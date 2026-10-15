# Factory triage

This procedure takes the findings of one verify round and asks me what to do with each one. The caller gives the job folder and the round `N`. The formats are in `state.md`.

## 1. Read the round

Read `round-N/report.md`. Take every entry under **Findings** and every entry under **Open questions**. Leave out the **Dropped** entries.

## 2. Filter

Read `decisions.md` and `deferred.md`. Compare each finding with every entry in both files.

A finding matches an entry when both describe the same problem in the same code element: the same function, class, field or config key. The line numbers can differ, because the code moves between rounds. The wording can differ too. A matching location alone is not a match.

- **Clear match with a `decisions.md` entry**: record `F{n}: auto-dismissed: matches D{k}` in `triage.md`. Don't show it to me.
- **Clear match with a `deferred.md` entry**: record `F{n}: auto-deferred: matches E{k}` in `triage.md`. Don't show it to me.
- **Uncertain match**: show it to me in step 3, marked `possibly dismissed as D{k}` or `possibly deferred as E{k}`.
- **No match**: show it to me in step 3.

Never drop an uncertain match.

## 3. Present

If no finding is left, tell me so and go to step 5.

Read the code for each finding left. Then show me each one:

- its ID, severity and location as `path:line`
- the problem and the suggested fix, as the report gives them
- the possible match, if any
- your assessment in one or two sentences: fix it, dismiss it or defer it, and why

Then ask me with **AskUserQuestion**. Ask one question per finding, up to four per call, worst first. Offer `Fix`, `Dismiss` and `Defer`. Put your assessment first and mark it `(Recommended)`. I may answer in chat instead.

## 4. Record

For each finding, by my answer:

- **Fix**: add an `open` task to `tasks.md`. Its title is the finding's title, and its source is `round N F{n}`. Its text holds the problem, the location, the suggested fix and any note I added.
- **Dismiss**: add an entry to `decisions.md`. The reason is my note. If I gave none, use your assessment and say so: `per assessment: {reason}`.
- **Defer**: add an entry to `deferred.md` with `issue: none`.

Record each one in `round-N/triage.md`, as `state.md` shows.

## 5. Pick the outcome

Ask me with **AskUserQuestion** what comes next:

- `Next round`: implement the fix tasks, then run another verify round.
- `Last round`: implement the fix tasks, then finish without another verify round.
- `Finalize`: finish now.

Recommend `Finalize` when no fix task came out of this round. Otherwise recommend `Next round`.

If I pick `Finalize` and this round added fix tasks, warn me that they will stay open. Offer `Last round` instead. Ask again.

Write the outcome as the last line of `round-N/triage.md`.
