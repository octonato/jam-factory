# Glossary

The terms of Jam Board and of the workflow around it. An alias in parentheses names the same thing.

## Workflow

**Main checkout**: A repository clone listed in `repos` in `~/.jam-factory/config.json`. Jam Board lists it with its linked worktrees.

**Worktree**: A linked git worktree of a main checkout, in a sibling folder. It holds one feature branch.

**Session**: One run of the factory on one branch. Jam Board shows its stages and files.

**Session type**: What a session does. Its `flow` in `session.json` sets it:

- **Build**: `/jam:take` takes one change through spec, plan, implement, verify and triage.
- **Vibe**: `/jam:just-vibe` runs a quick task with no spec, plan or review.
- **Review**: `/jam:review` reviews a coworker's PR and writes a PR comment.
- **Walk**: `/jam:walk-through` explains a code path or a change, one step at a time.

**Live session**: A session that is not archived. A branch holds at most one live build or vibe session and one live review session.

**Archived session**: A session with `archived: true`. Jam Board hides it. Archive a session to start a new one of its type on the same branch.

## Jam Board

**Jam Board**: The desktop app that shows sessions, worktrees, PRs and GitHub pages. `jam-factory-server` serves its pages.

**Header tabs**: GitHub, Sessions and Terminals, at the top of every page.

**Home page** (main page): The page of the GitHub header tab. It shows your PRs on github.com.

**Sessions page**: The page that lists the live sessions.

**Terminals page**: The page that lists the terminal tabs of every worktree.

**PRs page**: The page of one main checkout. It lists its open PRs and its worktrees without a PR.

**Worktree page**: The page of one worktree. It shows the worktree card, the bar and the selected bar item.

**Worktree card**: The top of the worktree page. It shows the worktree, or its PR as the PRs page shows it, and the worktree's actions.

**Bar**: The row under the worktree card. It holds one item per live session, the GitHub item and the Terminal item.

**GitHub item**: The bar item that shows the worktree's PR, its issue or its repo page in a GitHub view.

**Terminal item**: The bar item that shows the worktree's terminal tabs.

**Sidebar**: The left column of the worktree page while a session item is selected. It holds the session strip and the session timeline.

**Session strip**: The facts of the selected session, as pills, and its archive button.

**Session timeline**: The nodes of the selected session, top to bottom, with a label before each round.

**Node**: One point on the session timeline, such as `spec`, `review` or `triage`. Its state shows whether it is done, running, waiting for you, blocked or next. Click it to open its file in the viewer.

**Viewer**: The pane right of the sidebar. It shows the file of the selected node.

**GitHub view** (GitHub page): A GitHub page shown inside Jam Board, on the home page or on a GitHub item.

**Go menu** (navigation menu): The app menu that holds the board-wide keys. They work on every page, even while a GitHub view has the focus:

- **Branches** (⇧⌘B), **Worktrees** (⇧⌘W) and **Terminals** (⇧⌘T) open the jump pop-up on that tab.
- **GitHub** (⇧⌘G) opens the GitHub pop-up.
- **Previous Bar Item** (⌥⌘[) and **Next Bar Item** (⌥⌘]) select the bar item left or right of the current one, and wrap around at the ends.
- **Open in New Window** (⌘N) opens the shown worktree page in its own window.
- **Jam Board Window** (⌘0) opens the main Jam Board window from a worktree's own window.

**Jump pop-up**: The pop-up of ⇧⌘B, ⇧⌘W and ⇧⌘T. Its Branches, Worktrees and Terminals tabs list the pages to jump to.

**GitHub pop-up**: The pop-up of ⇧⌘G over a GitHub view. It checks out a PR, starts work on an issue or loads a repo page.
