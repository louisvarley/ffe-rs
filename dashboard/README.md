# Burndown dashboard

Open `index.html` in a browser (no server or network needed). It reads
`burndown.js` and draws the remaining-work chart, the burn rate and the work-item
table.

## Rule: update it whenever the build moves

Every time a build or commit changes the milestone's progress, update
`dashboard/burndown.js` in the same commit:

1. Set the **status** of every finished item to `done` (and `in-progress` when
   work has started), and give it the version/commit label.
2. **Append** one history point with today's date, the remaining count
   (`total - done`, where in-progress counts as not done) and a one-line note of
   what landed. Never rewrite older points; if a correction is unavoidable, say
   so in the note.
3. Keep `total` implied by the task list: add a task when scope is added (e.g.
   "a planet in the flight scenario"), and note that in the history point.

The page shows a warning if the last history point disagrees with the task list,
so a missed update is visible rather than silent.

The chart is a burndown of the **whole port to completion**: every milestone
from the current one to release hardening is a milestone section in the data,
and the chart sums the items that are not done. Completed milestones stay in the
data with all items `done` (they keep the history honest); add a new milestone
section when scope is planned, and say so in the history point.
