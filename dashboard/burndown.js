// Whole-project milestone burndown. Update this file whenever a build or commit
// changes progress: set item statuses, add items when scope is added, and APPEND
// one dated history point with the new remaining count.
//
// Remaining counts every task that is not "done" (in-progress counts as not
// done). History is the only source for the chart's time axis: never rewrite old
// points, except to correct a mistake (say so in the note).
//
// The backlog was first enumerated at milestone level on 2026-10-23; the
// 2026-10-22 point reflects the v0.48 plan only (the rest of the backlog existed
// but was not itemised yet).
window.FFE_BURNDOWN = {
  project: "ffe-rs",
  unit: "work items",
  updated: "2026-10-23",
  milestones: [
    {
      id: "v0.48",
      title: "v0.48 — Every model fully drawn",
      status: "in-progress",
      tasks: [
        { id: "labels", title: "Labels and the full 219-entry string table", status: "done", version: "v0.48a" },
        { id: "text", title: "3D text: glyph programs and F1747 placement", status: "done", version: "v0.48b" },
        { id: "surface-resolve", title: "F1874 surface origin and axis resolution", status: "done", version: "v0.48c" },
        { id: "surface-helpers", title: "Surface helpers F1875 / F1824 / F1825", status: "done", version: "v0.48d" },
        { id: "surface-entry-map", title: "F1823 entry map (replace the legacy hash arena)", status: "in-progress", version: "v0.48e" },
        { id: "surface-dispatch", title: "F1833 flat/textured emitter dispatch", status: "in-progress", version: "v0.48e" },
        { id: "surface-emit", title: "F1834 / F1835 quad emission (with F1868-F1873)", status: "todo" },
        { id: "surface-oracle", title: "Surface quad oracle at a planet-scale distance", status: "todo" },
        { id: "lines", title: "F1616 line rasteriser (replaces the stand-in)", status: "done", version: "v0.48f" },
        { id: "curves", title: "F1811 curve rasteriser (replaces the stand-in cubic)", status: "done", version: "v0.48g" },
        { id: "pixels", title: "Pixel-level whole-frame oracle (F1795-F1797 spans)", status: "in-progress", version: "v0.48i" },
        { id: "planet", title: "A planet in the flight scenario", status: "todo" }
      ]
    },
    {
      id: "flight",
      title: "Faithful flight and gameplay (phase 3/4)",
      status: "planned",
      tasks: [
        { id: "thrust", title: "Original thrust vector and ship limits (F598/F600)", status: "todo" },
        { id: "physics", title: "Original flight integration decoupled from the frame", status: "todo" },
        { id: "controls", title: "Original input processing at 50 Hz (incl. remapping)", status: "todo" },
        { id: "docking", title: "Original docking rules and station position/orientation", status: "todo" },
        { id: "economy", title: "Market and economy tables", status: "todo" },
        { id: "combat", title: "Combat, AI and missiles", status: "todo" },
        { id: "navigation", title: "Navigation, hyperspace and time acceleration", status: "todo" },
        { id: "missions", title: "Missions and journal", status: "todo" },
        { id: "saves", title: "Legacy commander import and native save format", status: "todo" },
        { id: "audio", title: "HMP -> MIDI synthesis and the 23 effects", status: "todo" }
      ]
    },
    {
      id: "content",
      title: "Complete content import (phase 2)",
      status: "planned",
      tasks: [
        { id: "tables", title: "Gameplay tables (ships, equipment, economy)", status: "todo" },
        { id: "strings", title: "Message strings, journal and remaining fonts", status: "todo" },
        { id: "music", title: "Music data (HMP) cached losslessly", status: "todo" },
        { id: "cd", title: "Optional CD media inventory (videos/stills)", status: "todo" }
      ]
    },
    {
      id: "packs",
      title: "Graphics-pack slice (phase 6)",
      status: "planned",
      tasks: [
        { id: "upscale", title: "Upscale export and pack commands", status: "todo" },
        { id: "pack-validate", title: "Pack registry, validation and fallback", status: "todo" },
        { id: "pack-switch", title: "Live Classic/Enhanced switching", status: "todo" }
      ]
    },
    {
      id: "enhanced",
      title: "Enhanced renderer (phase 7)",
      status: "planned",
      tasks: [
        { id: "native", title: "Native resolution, filtering and MSAA", status: "todo" },
        { id: "bloom", title: "Modest bloom", status: "todo" },
        { id: "atmosphere", title: "Planet atmosphere and shadows", status: "todo" },
        { id: "hdr", title: "HDR output and temporal AA (deferred)", status: "todo" }
      ]
    },
    {
      id: "release",
      title: "Release hardening (phase 8)",
      status: "planned",
      tasks: [
        { id: "persistence", title: "Config and input-remap persistence", status: "todo" },
        { id: "migration", title: "Save/config migration", status: "todo" },
        { id: "platforms", title: "Linux/macOS builds", status: "todo" },
        { id: "packaging", title: "Packaging and distribution audit", status: "todo" },
        { id: "licence", title: "Choose the Rust code licence", status: "todo" }
      ]
    }
  ],
  history: [
    {
      date: "2026-10-22",
      remaining: 38,
      note: "Whole-port scope at milestone level: v0.48 planned with 12 items; the later phases were already planned in CLAUDE.MD."
    },
    {
      date: "2026-10-23",
      remaining: 34,
      note: "v0.48a-d landed (labels, 3D text, surface resolution verified, three surface helpers). v0.48a-d landed (labels, 3D text, surface resolution verified, three surface helpers); the whole-port backlog is itemised here for the first time."
    },
    {
      date: "2026-10-23",
      remaining: 33,
      note: "v0.48f: F1616 line rasteriser ported and verified (4096 cases, 0 mismatched)."
    },
    {
      date: "2026-10-23",
      remaining: 32,
      note: "v0.48g: F1811/F1546 curve rasteriser ported and verified (4096 curves, 0 mismatched)."
    },
    {
      date: "2026-10-23",
      remaining: 32,
      note: "v0.48h: flat triangle/quad rasterisers (F1604, F1607/F1608) ported and verified (4096 faces); the pixel item is in progress."
    },
    {
      date: "2026-10-23",
      remaining: 32,
      note: "v0.48i: polygon spans (F1560/F1613) and F1714 off-view outlines ported and verified; every primitive now draws with original rasterisers. Whole-frame oracle still to do."
    }
  ]
};
