// Burndown data for the current milestone. Update this file whenever a build or
// commit changes the milestone's progress: set the new task statuses and APPEND
// one history point (dated, with the remaining count and a short note).
//
// Remaining counts every task that is not "done" (in-progress counts as not
// done). History is the only source for the chart's time axis: never rewrite old
// points, except to correct a mistake (say so in the note).
window.FFE_BURNDOWN = {
  project: "ffe-rs",
  milestone: "v0.48 — Every model fully drawn",
  unit: "work items",
  updated: "2026-10-23",
  tasks: [
    { id: "labels", title: "Labels and the full 219-entry string table", status: "done", version: "v0.48a" },
    { id: "text", title: "3D text: glyph programs and F1747 placement", status: "done", version: "v0.48b" },
    { id: "surface-resolve", title: "F1874 surface origin and axis resolution", status: "done", version: "v0.48c" },
    { id: "surface-helpers", title: "Surface helpers F1875 / F1824 / F1825", status: "done", version: "v0.48d" },
    { id: "surface-entry-map", title: "F1823 entry map (replace the legacy hash arena)", status: "in-progress", version: "v0.48e" },
    { id: "surface-dispatch", title: "F1833 flat/textured emitter dispatch", status: "in-progress", version: "v0.48e" },
    { id: "surface-emit", title: "F1834 / F1835 quad emission (with F1868-F1873)", status: "todo" },
    { id: "surface-oracle", title: "Surface quad oracle at a planet-scale distance", status: "todo" },
    { id: "lines", title: "F1616 line rasteriser (replaces the stand-in)", status: "todo" },
    { id: "curves", title: "F1811 curve rasteriser (replaces the stand-in cubic)", status: "todo" },
    { id: "pixels", title: "Pixel-level whole-frame oracle (F1795-F1797 spans)", status: "todo" },
    { id: "planet", title: "A planet in the flight scenario", status: "todo" }
  ],
  history: [
    {
      date: "2026-10-22",
      remaining: 12,
      note: "Milestone v0.48 planned in docs/STATUS.md."
    },
    {
      date: "2026-10-23",
      remaining: 8,
      note: "v0.48a-e landed: labels, 3D text, surface resolution (verified), three surface helpers, entry-map/dispatch research."
    }
  ]
};
