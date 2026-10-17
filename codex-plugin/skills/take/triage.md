# Factory triage

This procedure takes the findings of one verify round and decides what to do with each one. The caller gives the job folder, the round `N` and the mode. The formats are in `state.md`.

In `interactive` mode, I decide. In `default` and `autonomous` mode, you decide and ask me nothing.

## 1. Read the round

Read `round-N/report.md`. Take every entry under **Findings** and every entry under **Open questions**. Leave out the **Dropped** entries.

## 2. Filter

Read `decisions.md` and `deferred.md`. Compare each finding with every entry in both files.

A finding matches an entry when both describe the same problem in the same code element: the same function, class, field or config key. The line numbers can differ, because the code moves between rounds. The wording can differ too. A matching location alone is not a match.

- **Clear match with a `decisions.md` entry**: record `F{n}: auto-dismissed: matches D{k}` in `triage.md`. Don't show it to me.
- **Clear match with a `deferred.md` entry**: record `F{n}: auto-deferred: matches E{k}` in `triage.md`. Don't show it to me.
- **Uncertain match**: in `interactive` mode, show it to me in step 3. Mark it `possibly dismissed as D{k}` or `possibly deferred as E{k}`. In the other modes, keep it for step 3.
- **No match**: keep it for step 3.

Never drop an uncertain match.

## 3. Decide

If no finding is left, tell me so and go to step 5.

Read the code for each finding left. Then follow the part for the mode.

### `interactive`

Show me each finding:

- its ID, severity and location as `path:line`
- the problem and the suggested fix, as the report gives them
- the possible match, if any
- your assessment in one or two sentences: fix it, dismiss it or defer it, and why

Then ask me with **request_user_input**. Ask one question per finding, up to four per call, worst first. Offer `Fix`, `Dismiss` and `Defer`. Put your assessment first and mark it `(Recommended)`. I may answer in chat instead.

### `default` and `autonomous`

Check each finding against the code. A finding is valid when the code has the stated problem and the change under review causes it. First settle whether an uncertain match is a real match. If it is, handle it as step 2 does.

Decide each finding:

- **A valid `must-fix` or `should-fix`**: fix it.
- **A valid `nit`**: defer it.
- **An open question**: defer it.
- **An invalid finding**: dismiss it. The reason names the code that shows the finding is wrong.

Tell me each decision in one line: the ID, the severity, the decision and the reason.

## 4. Record

For each finding, by the decision:

- **Fix**: add an `open` task to `tasks.md`. Its title is the finding's title, and its source is `round N F{n}`. Its text holds the problem, the location, the suggested fix and any note I added.
- **Dismiss**: add an entry to `decisions.md`. In `interactive` mode, the reason is my note. If I gave none, use your assessment and say so: `per assessment: {reason}`. In the other modes, the reason is `per check: {reason}`.
- **Defer**: add an entry to `deferred.md` with `issue: none`.

Record each one in `round-N/triage.md`, as `state.md` shows.

## 5. Pick the outcome

The outcomes are:

- `Next round`: implement the fix tasks, then run another verify round.
- `Last round`: implement the fix tasks, then finish without another verify round.
- `Finalize`: finish now.

In `default` and `autonomous` mode, pick `Last round` when this round added fix tasks, and `Finalize` otherwise. Ask me nothing.

In `interactive` mode, ask me with **request_user_input**. Recommend `Finalize` when no fix task came out of this round. Otherwise recommend `Next round`. If I pick `Finalize` and this round added fix tasks, warn me that they will stay open. Offer `Last round` instead. Ask again.

Write the outcome as the last line of `round-N/triage.md`.
