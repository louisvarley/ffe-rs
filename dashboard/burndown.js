// Whole-project burndown: from the first commit to a finished like-for-like
// replacement port. Update this file whenever a build or commit changes progress:
// set item statuses (give finished items a version label), add items when scope
// is found, and APPEND one dated history point with the new remaining count.
//
// Remaining counts every task that is not "done" (in-progress counts as not
// done). History is the only source for the chart's time axis: never rewrite old
// points, except to correct a mistake (say so in the note).
//
// 2026-10-23 restatement: the earlier two points counted only the work still to
// do (38 items). The backlog was widened to the whole project, finished work
// included, and the history rebuilt from the dates each finished item landed.
// Future items are planning-grade estimates and will be split or added as the
// work is understood; a rise in the remaining count means scope was found, not
// that work was lost.
window.FFE_BURNDOWN = {
  project: "ffe-rs",
  unit: "work items",
  updated: "2026-10-23",
  milestones: [
    {
      id: "foundation",
      title: "Foundation, evidence and tooling (phases 0-2)",
      status: "done",
      tasks: [
        { id: "foundation-1", title: "Inventory, rendering study, design proposal and repo hygiene", status: "done", version: "phase 0" },
        { id: "foundation-2", title: "Local-input protection and forbidden-file CI check", status: "done", version: "phase 0" },
        { id: "foundation-3", title: "ffe-tools: inspect, export and versioned cache (floppy 1.02 and CD 1.01 profiles)", status: "done", version: "v0.0" },
        { id: "foundation-4", title: "Core primitives: RNG, timer and clock verified against the original", status: "done", version: "v0.0" },
        { id: "foundation-5", title: "Reference harness: JJFFE adapter, Windows and Linux builds, trace tooling", status: "done", version: "v0.0-v0.1" },
        { id: "foundation-6", title: "Original bitmap font and console image decoding", status: "done", version: "v0.2" },
        { id: "foundation-7", title: "verify-all runner and CI", status: "done", version: "v0.38" }
      ]
    },
    {
      id: "model-engine",
      title: "Model engine, verified against the original (v0.3-v0.43)",
      status: "done",
      tasks: [
        { id: "model-engine-1", title: "Vertex interpreter and normals", status: "done", version: "v0.3-v0.5" },
        { id: "model-engine-2", title: "Command decoding and helpers", status: "done", version: "v0.4" },
        { id: "model-engine-3", title: "Sine table, transforms, rotation operands and visibility", status: "done", version: "v0.6" },
        { id: "model-engine-4", title: "Bounded executor and live vertex slots with projection", status: "done", version: "v0.7-v0.11" },
        { id: "model-engine-5", title: "Depth keys, depth commands and render ordering", status: "done", version: "v0.8-v0.12" },
        { id: "model-engine-6", title: "Point, line, text, child-call and colour operands", status: "done", version: "v0.13-v0.15" },
        { id: "model-engine-7", title: "Base transforms: real stations run to completion", status: "done", version: "v0.16-v0.17" },
        { id: "model-engine-8", title: "Face draw queue rule and whole-table model census", status: "done", version: "v0.18-v0.20" },
        { id: "model-engine-9", title: "Complex polygons, jittered vertices and thruster operands", status: "done", version: "v0.21-v0.23" },
        { id: "model-engine-10", title: "Coloured child calls, surface operands and nested depth blocks", status: "done", version: "v0.24-v0.26" },
        { id: "model-engine-11", title: "Opcodes 17/24/25 and trailing declaration data", status: "done", version: "v0.27-v0.28" },
        { id: "model-engine-12", title: "Curve, offset-curve and vertex-defined-basis vertex programs", status: "done", version: "v0.29-v0.33" },
        { id: "model-engine-13", title: "Frame restore, curves, labels, light points and sound requests", status: "done", version: "v0.30-v0.36" },
        { id: "model-engine-14", title: "Complex polygon segments and outlines", status: "done", version: "v0.38" },
        { id: "model-engine-15", title: "Textured faces verified", status: "done", version: "v0.38" },
        { id: "model-engine-16", title: "Child models with external vertices verified", status: "done", version: "v0.38" },
        { id: "model-engine-17", title: "Screen-space midpoints, zero normals and the return record (census 449 of 466)", status: "done", version: "v0.42-v0.43" }
      ]
    },
    {
      id: "visible",
      title: "Visible slice: models, ships and views (v0.37-v0.47)",
      status: "done",
      tasks: [
        { id: "visible-1", title: "F6 model viewer over every decodable model", status: "done", version: "v0.37" },
        { id: "visible-2", title: "The real station model in the flight view", status: "done", version: "v0.39" },
        { id: "visible-3", title: "Original lighting tables and F1326 colour packing", status: "done", version: "v0.40" },
        { id: "visible-4", title: "Child models drawn in the viewer and in flight", status: "done", version: "v0.41" },
        { id: "visible-5", title: "Real ships by name (slots 14-70), Page Up/Down", status: "done", version: "v0.44" },
        { id: "visible-6", title: "Original views: F1 cycle, F6/F7/F9, console icons, external camera", status: "done", version: "v0.44" },
        { id: "visible-7", title: "Thruster glows (F1738/F1704/F1805)", status: "done", version: "v0.45" },
        { id: "visible-8", title: "Animation inputs, station spin and rotating parts", status: "done", version: "v0.45" },
        { id: "visible-9", title: "Ship statistics, cockpit eyes and turret views", status: "done", version: "v0.46" },
        { id: "visible-10", title: "Clickable console view icons and flickering plumes", status: "done", version: "v0.47" }
      ]
    },
    {
      id: "v0.48",
      title: "v0.48 - Every model fully drawn",
      status: "in-progress",
      tasks: [
        { id: "v0.48-1", title: "Labels and the full 219-entry string table", status: "done", version: "v0.48a" },
        { id: "v0.48-2", title: "3D text: glyph programs and F1747 placement", status: "done", version: "v0.48b" },
        { id: "v0.48-3", title: "F1874 surface origin and axis resolution", status: "done", version: "v0.48c" },
        { id: "v0.48-4", title: "Surface helpers F1875 / F1824 / F1825", status: "done", version: "v0.48d" },
        { id: "v0.48-5", title: "F1823 entry map (replace the legacy hash arena)", status: "in-progress", version: "v0.48e" },
        { id: "v0.48-6", title: "F1833 flat/textured emitter dispatch", status: "in-progress", version: "v0.48e" },
        { id: "v0.48-7", title: "F1834 / F1835 quad emission (with F1868-F1873)", status: "todo" },
        { id: "v0.48-8", title: "Surface quad oracle at a planet-scale distance", status: "todo" },
        { id: "v0.48-9", title: "F1616 line rasteriser", status: "done", version: "v0.48f" },
        { id: "v0.48-10", title: "F1811 curve rasteriser", status: "done", version: "v0.48g" },
        { id: "v0.48-11", title: "F1604 / F1607 flat face rasterisers", status: "done", version: "v0.48h" },
        { id: "v0.48-12", title: "Polygon spans (F1560/F1613) and F1714 off-view outlines", status: "done", version: "v0.48i" },
        { id: "v0.48-13", title: "Fix the verify-assets and child-model oracles broken since v0.48b", status: "done", version: "v0.48j" },
        { id: "v0.48-14", title: "Pixel-level whole-frame oracle (framebuffer mode, station and three ships)", status: "todo" },
        { id: "v0.48-15", title: "Upper runtime palette", status: "todo" },
        { id: "v0.48-16", title: "Sprites, particles and circles (F1609-F1612)", status: "todo" },
        { id: "v0.48-17", title: "Remaining 17 of 466 model slots and the CD 1.01 census", status: "todo" },
        { id: "v0.48-18", title: "A planet in the flight scenario", status: "todo" }
      ]
    },
    {
      id: "flight",
      title: "Faithful flight core (phase 3)",
      status: "planned",
      tasks: [
        { id: "flight-1", title: "Object system and agent records (cstruct, gmstruct)", status: "todo" },
        { id: "flight-2", title: "Original thrust vector and ship limits (F598/F600)", status: "done", version: "v0.49d" },
        { id: "flight-3", title: "Physics: position and orientation update (cphys)", status: "done", version: "v0.49f" },
        { id: "flight-4", title: "Flight integration decoupled from the frame; fixed 50 Hz step", status: "in-progress", version: "v0.49h" },
        { id: "flight-5", title: "Original input processing, key accumulation and remapping", status: "in-progress", version: "v0.49g" },
        { id: "flight-6", title: "Time acceleration and its RNG consumption", status: "in-progress", version: "v0.49i" },
        { id: "flight-7", title: "Collision detection and response (ccoll)", status: "in-progress", version: "v0.49n" },
        { id: "flight-8", title: "Player events and state machine (cplayer, plstate)", status: "todo" },
        { id: "flight-9", title: "Entering and leaving the rotating station frame", status: "in-progress", version: "v0.49n" },
        { id: "flight-10", title: "Docking rules and station position/orientation", status: "todo" },
        { id: "flight-11", title: "Launch sequence", status: "todo" },
        { id: "flight-12", title: "Scripted flight scenarios traced against the reference", status: "todo" }
      ]
    },
    {
      id: "world",
      title: "The galaxy and its systems",
      status: "planned",
      tasks: [
        { id: "world-1", title: "Name generation (cname)", status: "todo" },
        { id: "world-2", title: "System generation: stars, planets and station placement (csysgen)", status: "in-progress", version: "v0.50a" },
        { id: "world-3", title: "System object types and orbits (csystem)", status: "todo" },
        { id: "world-4", title: "Galaxy generation and the RNG stream question (KNOWN_ISSUES F1476)", status: "todo" },
        { id: "world-5", title: "Planet surfaces and landing approach", status: "todo" },
        { id: "world-6", title: "Starports: services and events (cstarport)", status: "todo" },
        { id: "world-7", title: "Hyperspace jumps and arrival (chyper)", status: "todo" },
        { id: "world-8", title: "Navigation: system and galaxy maps", status: "todo" },
        { id: "world-9", title: "Lighting by the local star and day/night", status: "todo" },
        { id: "world-10", title: "Multiple systems replay against the reference", status: "todo" }
      ]
    },
    {
      id: "gameplay",
      title: "Gameplay systems (phase 4)",
      status: "planned",
      tasks: [
        { id: "gameplay-1", title: "Market and economy tables, pricing and cargo", status: "todo" },
        { id: "gameplay-2", title: "Equipment and ship purchase (shipyard, outfitting)", status: "todo" },
        { id: "gameplay-3", title: "Fuel, damage and repair", status: "todo" },
        { id: "gameplay-4", title: "Lasers and hit detection (claser)", status: "todo" },
        { id: "gameplay-5", title: "Missiles and countermeasures", status: "todo" },
        { id: "gameplay-6", title: "Agents: AI behaviour (cagent)", status: "todo" },
        { id: "gameplay-7", title: "Police, pirates and other encounters", status: "todo" },
        { id: "gameplay-8", title: "Combat scenario replay against the reference", status: "todo" },
        { id: "gameplay-9", title: "Bulletin board and missions (cbbs)", status: "todo" },
        { id: "gameplay-10", title: "Journal and mission state", status: "todo" },
        { id: "gameplay-11", title: "Shuttles and station traffic (shuttle)", status: "todo" },
        { id: "gameplay-12", title: "Communications and messages", status: "todo" },
        { id: "gameplay-13", title: "Player death and the escape capsule", status: "todo" },
        { id: "gameplay-14", title: "Rank, reputation and legal status", status: "todo" },
        { id: "gameplay-15", title: "Time-driven events over a long run", status: "todo" }
      ]
    },
    {
      id: "ui",
      title: "Interface and presentation (phase 5)",
      status: "planned",
      tasks: [
        { id: "ui-0", title: "Console buttons: time acceleration meter and screen buttons", status: "done", version: "v0.49i" },
        { id: "ui-1", title: "Cockpit instruments: scanner, gauges and status lights", status: "todo" },
        { id: "ui-2", title: "Crosshair, targeting and the HUD overlay", status: "todo" },
        { id: "ui-3", title: "Missile, combat and external camera views", status: "todo" },
        { id: "ui-4", title: "In-station screens: market, shipyard, bulletin board, status", status: "todo" },
        { id: "ui-5", title: "Menu and text rendering for every screen", status: "todo" },
        { id: "ui-6", title: "Galaxy and system map screens", status: "todo" },
        { id: "ui-7", title: "Intro and new-game flow", status: "todo" },
        { id: "ui-8", title: "Mouse and joystick input", status: "todo" },
        { id: "ui-9", title: "Options, key bindings screen", status: "todo" },
        { id: "ui-10", title: "Original palette effects and screen transitions", status: "todo" },
        { id: "ui-11", title: "Far and near transitions, clipping and planet approach fidelity", status: "todo" }
      ]
    },
    {
      id: "persist",
      title: "Saves and persistence",
      status: "planned",
      tasks: [
        { id: "persist-1", title: "Legacy commander import (read-only)", status: "todo" },
        { id: "persist-2", title: "Native versioned save format", status: "todo" },
        { id: "persist-3", title: "Save, load and quick-save in the game", status: "todo" },
        { id: "persist-4", title: "Imported saves replayed against the reference", status: "todo" },
        { id: "persist-5", title: "Config and input-remap persistence", status: "todo" }
      ]
    },
    {
      id: "audio",
      title: "Audio",
      status: "planned",
      tasks: [
        { id: "audio-1", title: "Sound effects: the 23 effects and sound requests", status: "todo" },
        { id: "audio-2", title: "HMP to MIDI parsing", status: "todo" },
        { id: "audio-3", title: "MIDI synthesis and music playback", status: "todo" },
        { id: "audio-4", title: "Positional volume and mixing", status: "todo" }
      ]
    },
    {
      id: "content",
      title: "Complete content import (phase 2)",
      status: "planned",
      tasks: [
        { id: "content-1", title: "Gameplay tables (ships, equipment, economy)", status: "todo" },
        { id: "content-2", title: "Message strings, journal and remaining fonts", status: "todo" },
        { id: "content-3", title: "Music data (HMP) cached losslessly", status: "todo" },
        { id: "content-4", title: "Optional CD media inventory (videos and stills)", status: "todo" },
        { id: "content-5", title: "Import verification on CD 1.01 as well as floppy 1.02", status: "todo" }
      ]
    },
    {
      id: "packs",
      title: "Graphics-pack slice (phase 6)",
      status: "planned",
      tasks: [
        { id: "packs-1", title: "Upscale export and pack commands", status: "todo" },
        { id: "packs-2", title: "Pack registry, validation and fallback", status: "todo" },
        { id: "packs-3", title: "Live Classic/Enhanced switching", status: "todo" },
        { id: "packs-4", title: "Pack changes never change gameplay traces", status: "todo" }
      ]
    },
    {
      id: "enhanced",
      title: "Enhanced renderer (phase 7)",
      status: "planned",
      tasks: [
        { id: "enhanced-1", title: "Native resolution, filtering and MSAA", status: "todo" },
        { id: "enhanced-2", title: "Modest bloom", status: "todo" },
        { id: "enhanced-3", title: "Planet atmosphere and shadows", status: "todo" },
        { id: "enhanced-4", title: "HDR output and temporal AA (deferred)", status: "todo" },
        { id: "enhanced-5", title: "Frame and VRAM budgets measured", status: "todo" }
      ]
    },
    {
      id: "release",
      title: "Release hardening and parity sign-off (phase 8)",
      status: "planned",
      tasks: [
        { id: "release-1", title: "Full playthrough checkpoints against the reference", status: "todo" },
        { id: "release-2", title: "Linux and macOS builds", status: "todo" },
        { id: "release-3", title: "Save and config migration", status: "todo" },
        { id: "release-4", title: "Packaging and distribution audit", status: "todo" },
        { id: "release-5", title: "Clean-machine player-data import", status: "todo" },
        { id: "release-6", title: "Choose the Rust code licence", status: "todo" },
        { id: "release-7", title: "Like-for-like parity sign-off by the owner", status: "todo" }
      ]
    }
  ],
  history: [
    {
      date: "2026-10-20",
      remaining: 128,
      note: "Phase 0 evidence, tooling plan and local-input protection."
    },
    {
      date: "2026-10-21",
      remaining: 110,
      note: "Tools, reference harness, verified model engine v0.3-v0.37 in one day."
    },
    {
      date: "2026-10-22",
      remaining: 96,
      note: "Textured and child models, real ships and views, glows, animation, ship stats."
    },
    {
      date: "2026-10-23",
      remaining: 88,
      note: "v0.48a-i: labels, 3D text, surface resolution, line, curve, flat and polygon rasterisers."
    },
    {
      date: "2026-10-23",
      remaining: 87,
      note: "v0.48j: the verify-assets and child-model oracles repaired; all 31 local oracles pass."
    },
    {
      date: "2026-10-23",
      remaining: 87,
      note: "v0.49a: thrust limits F598/F599/F600 ported and verified (4096 cases); the thrust item is in progress until the in-game thrust uses them. RNG isolation decision recorded (SIM-010)."
    },
    {
      date: "2026-10-23",
      remaining: 87,
      note: "v0.49b: F601/F602 velocity from thrust ported and verified (4096 cases); physics item in progress."
    },
    {
      date: "2026-10-23",
      remaining: 87,
      note: "v0.49c: orientation update F585/F584/F611/F1520 ported and verified (4096 cases); physics still in progress (positions, frames, gravity)."
    },
    {
      date: "2026-10-23",
      remaining: 87,
      note: "v0.49d: the player's free-flight orientation and thrust request (F1041, F607, F1043) ported and verified (4096 cases)."
    },
    {
      date: "2026-10-23",
      remaining: 87,
      note: "v0.49e: position from velocity (F1512/F1511) ported and verified (4096 cases)."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49f: a whole free-flight tick (F1041 + F601/F602 + F1512) verified end to end; the thrust and physics items are done for free space."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49g: set-speed and heading input (F1037/F1038/F1121) ported and verified (4096 cases); input item in progress (key accumulation and remapping remain)."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49h: the original flight model is playable (key O): verified physics driven by real time and the original keys; frames of reference, collisions and docking rules remain."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49i: the console's time acceleration meter and screen buttons work (scope +1 item, done); time acceleration scales the original flight model."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49j: gravity F603 and the ffp helpers ported and verified (4096 cases); frames of reference, auto-roll and collisions remain."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49k: frames of reference (F590/F604/F605/F606, F1677/F1678) ported and verified (4096 cases); station frames, auto-roll and collisions remain."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49l: auto-roll F609 and a whole tick in a world (gravity, frames, auto-roll) verified (4096 cases); the altitude/collision routines remain."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49m: collision distance F569/F570 (F575, F577) and the rotating-frame switch F572/F573 ported and verified (3,103 contact-free cases); contact, landing and complex shapes remain."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.49n: the player's whole tick with the collision scan verified (3,476 contact-free ticks); collision (ccoll) and rotating-frame items now in progress."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.50a: cache v15 with the galaxy and system tables; F868-F871 (system index, details, market, control) verified (5,835 lookups); system generation in progress."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.50b: system generation F872-F897 ported and verified byte for byte (4,156 systems; names aside), with the ffp library; object creation (F879/F880) and names remain."
    },
    {
      date: "2026-10-23",
      remaining: 85,
      note: "v0.50c: physics objects from system objects (F927, F1533, F880, orbit helpers) verified on 1,024 systems; cache v16; starport placement waits for the terrain query (F1876 distance mode)."
    }
  ]
};
