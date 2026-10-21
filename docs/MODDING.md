# Modding (design; implementation starts in Phase 8)

Status: **deferred design, not v1 scope.** The owner selected image-only graphics
packs first; no gameplay data mods, mesh packs or scripting are approved for v1.
The broader registry/mod sketches below are future research, not implementation
requirements. Follow [CLAUDE.MD](../CLAUDE.MD) for current scope.

---

## 1. Principles

- **All game content is data, never code.** Ships, equipment, goods, systems, planets, stations, missions and text are registry entries with stable IDs.
- **Stable namespaced IDs:** `base:cobra_mk3`, `mymod:my_ship`. Base content comes from `data.bin`; mods add/override/patch.
- **Deterministic load order.** Same mods + same versions + same order = same world.
- **No mods loaded = reference behaviour.** Golden tests run on base content only.
- **A mod cannot change unrelated procedural systems.** Adding/overriding one system, planet or station must not shift the RNG for any other system.
- **No scripting in v1.** Data-driven templates only; a `ScriptHost` extension point is reserved.
- **Precise validation.** File, line, field, expected type; generated JSON Schema; `validate-mod`; `new-mod`; dev-mode hot reload.
- **No redistribution of base assets.** `validate-mod` rejects files whose SHA-256 matches a base asset in `data.bin`.

---

## 2. Registry and IDs

```
Layer 0: base (data.bin)
Layer 1..n: mods in dependency order
```

Each entry has:

```rust
struct ContentEntry<T> {
    id: ContentId,          // namespaced, stable
    source: SourceRef,      // data.bin path + hash, or mod file + line
    data: T,
}
```

The registry is read-only during a simulation tick. Hot reload swaps a new registry between ticks (never mid-tick). The core receives a `&dyn ContentRegistry` and never mutates it.

ID kinds (initial set):

```
base:ship/<name>
base:model/<name-or-index>
base:texture/<name-or-index>
base:font/<name>
base:good/<name>
base:equipment/<name>
base:system/<name-or-coords>
base:planet/<name-or-coords>
base:station/<name-or-coords>
base:mission/<name>
base:text/<name>
base:audio/<name>
```

Mod IDs are `<mod-id>:<kind>/<name>`. Changing an ID is a breaking change and requires a migration note.

---

## 3. `mod.toml`

```toml
id = "mymod"
name = "My Mod"
version = "0.1.0"
authors = ["Name <email>"]
description = "Adds a ship and a system."
game_version = "ffe-floppy-1.02-en"
load_order = 100
dependencies = [{ id = "othermod", version = ">=0.1,<1" }]

# Optional explicit overrides (otherwise use override = true in the data file)
[[override]]
id = "base:ship/cobra_mk3"
fields = ["mass", "price"]
```

- `id` is unique, lowercase, `[a-z0-9_-]+`.
- `load_order` breaks ties after dependencies; lower loads first. The final order is deterministic: topological sort, then `load_order`, then ID.
- `dependencies` support a small semver range; cycles are an error with the cycle path.
- `game_version` must match the loaded `data.bin`; a mismatch is an error unless the mod declares `allow_any_version = true` and does not patch base IDs.
- `override` lists the IDs/fields the mod is allowed to patch; unlisted overrides are errors.

---

## 4. Data files

Same RON structs as `data.bin` (see `docs/FORMATS.md`). Examples are sketches; the Phase 8 examples will be tested end-to-end.

### 4.1 Add a ship

```ron
// mods/mymod/data/ships/my_ship.ron
(
  id: "mymod:ship/my_ship",
  name: "My Ship",
  shipdef: (
    forward_thrust: 320,
    rear_thrust: 160,
    lasers: 2,
    fuel_scoop: 0,
    mass: 18,
    capacity: 20,
    price: 45000,
    scale: 12,
    description: "mymod:text/ship/my_ship",
    crew: 1,
    missiles: 2,
    drive: 2,
    integral_drive: 0,
    elite_bonus: 0,
    mounts: (
      front: (x: 0, y: 0, z: 40),
      back: (x: 0, y: 0, z: -20),
      left: (x: -30, y: 0, z: 0),
      right: (x: 30, y: 0, z: 0),
    ),
  ),
  model: "mymod:model/my_ship",   // Classic model with a lossless command stream
  source_index: null,             // no base index
)
```

A ship must have a Classic model (`model.ffe.ron`) to be usable in Classic; a GLB-only model is Enhanced-only and is rejected for Classic unless `classic_fallback = "base:model/..."` is set.

### 4.2 Add a star system

```ron
// mods/mymod/data/systems/my_system.ron
(
  id: "mymod:system/my_system",
  name: "My System",
  sector: (x: 0x1a00, y: 0x1500),
  star_number: 0,
  seed: 0x12345678,             // explicit; does not consume another system's RNG
  primary: (type: "G", mass: 1.0, radius: 1.0, temperature: 5778),
  objects: [
    (kind: "planet", name: "My World", orbit: 1.0, model: "base:model/earth", ...),
    (kind: "starport", name: "My Port", parent: 1, latitude: 10, longitude: 20, ...),
  ],
)
```

- The system is inserted/overridden by ID or coordinates. Its seed is explicit and independent.
- The generator for every other system is keyed by `(sector, star)` and does not consume this mod's RNG. The test in §7 proves it.

### 4.3 Add a planet or station

Planets/stations are entries under `data/planets/` or `data/stations/` with a parent system ID and orbital parameters. They can be hand-authored (inserted at a specific orbit) or override a generated object by `(system, index/name)`. The placement code is the same as the base starport placement (lat/lon, radius, parent), so a mod cannot create an impossible orbit without validation.

### 4.4 Add a trade good

```ron
(
  id: "mymod:good/tea",
  name: "Tea",
  price: 120,
  price_var: 300,
  stock: 400,
  stock_var: 800,
  legality: "legal",            // legal | illegal | controlled
  tech_level: 4,
  effects: (agriculture: 0.5, tourism: 0.2, ...),
)
```

Adding a good must not renumber base goods or change their market RNG. New goods get a stable index appended by the registry; the simulation uses the ID, not the array position, for mod content.

### 4.5 Add a mission template

```ron
(
  id: "mymod:mission/photo",
  kind: "photo",
  title: "mymod:text/mission/photo/title",
  body: "mymod:text/mission/photo/body",
  requirements: (min_rank: 2, min_tech: 5, system: "any"),
  objectives: [
    (kind: "photograph", target: "base:planet/mars", distance: 1000),
  ],
  rewards: (cash: 5000, rank: 1),
  deadline_days: 30,
)
```

Templates are data; the mission engine interprets a fixed set of objective/action kinds. No arbitrary code.

---

## 5. Override and patch semantics

- **Add:** ID not present in base/earlier layers.
- **Override:** replace the whole entry; requires `override = true` (or `[[override]]`).
- **Patch:** change only listed fields; type-checked; nested fields use dotted paths.
- Arrays are replaced by default; `append`/`prepend` are explicit operations.
- Two mods patching the same field is a conflict unless the later one declares `supersedes` or `allow_conflict = true`; the report shows both files.
- Removing base content is not allowed in v1 (it would change the base game). A mod may hide it via a gameplay-affecting option, documented separately.

---

## 6. Assets and material maps

Mods may supply:

- Classic art: indexed PNG + palette sidecar, same format as `data.bin`.
- HD art: RGBA PNG, optionally normal/roughness/emissive maps.
- Models: `*.ffe.ron` (Classic) and/or `*.glb` (Enhanced), with the same ID scheme.
- Audio: WAV/MIDI (Classic) and optional Enhanced audio.

The graphics options apply to mod assets too: `hd_textures` uses the mod's HD asset if present and falls back to its Classic asset; `material_maps` uses its maps; Classic ignores HD entirely. `validate-mod` checks that a mod asset ID does not collide with a base ID unless it is an explicit override, and rejects a file hash identical to a base asset.

---

## 7. Procedural galaxy isolation test

Required test (Phase 8, and a smaller version in Phase 3):

1. Generate a fixed set of N systems with no mods; record each system's object list, seeds, positions, names and per-system RNG trace.
2. Load a mod that inserts/overrides one system, planet or station.
3. Regenerate the same N systems.
4. Assert every **other** system is byte-identical (including RNG call sequence). The changed system may differ.
5. Repeat with a mod that adds a good/ship/mission (should not touch system generation at all).

The key is per-system RNG streams derived from `(sector, star)`, not a single global stream. If a base routine must use a global stream, the mod system must not run before/inside it; document the boundary.

---

## 8. Validation and tooling

Planned commands:

```sh
ffe-tools validate-mod mods/mymod
ffe-tools new-mod mymod --template ship
ffe-tools schema --out schema/
ffe-tools build-data --game ./game --out data.bin
```

`validate-mod` checks:

- TOML/RON syntax and types, with file/line/column and field path;
- required fields, enum values, numeric ranges;
- ID syntax/uniqueness, layer conflicts, dependency cycles/versions;
- `game_version` compatibility;
- asset dimensions/format/palette, model command stream validity;
- base asset hash collisions;
- dangling references (model/texture/font/audio/mission IDs);
- `provenance` for HD art.

Errors look like:

```
mods/mymod/data/ships/my_ship.ron:42:7: field `shipdef.mass` expected u16, found string "18"
  help: use `mass: 18`
```

JSON Schema is generated from the Rust types into `schema/` for editors. `validate-mod` uses the Rust types directly (schema is an aid, not the authority). Dev mode watches mod files and swaps the registry between ticks; a failed reload keeps the previous registry and shows the error.

---

## 9. Scripting (not in v1)

No scripting in v1. Extension points reserved:

- `MissionObjective`/`MissionAction` enums (fixed kinds, data-driven).
- `EventHook` registry for data-defined reactions (e.g. on docking, on kill) with a small condition language.
- `ScriptHost` trait (not implemented) so a future scripting backend can be added without changing core call sites.

**Recommendation for later:** **Rhai** — small, embeddable, sandboxable, deterministic, no ambient I/O by default. Lua is possible (FFED3D already embeds Lua 5.1), but it is not deterministic/sandboxed by default and would make mods able to change simulation. Any scripting would be gameplay-affecting, opt-in, and never part of Enhanced graphics or the master button. The owner must approve the language and sandbox policy before implementation.

---

## 10. Phase 8 deliverables

- Registry layering, add/override/patch, conflict reporting.
- `mod.toml`/RON schemas, JSON Schema generation.
- `validate-mod`, `new-mod`, dev hot reload.
- Tested example mods (used only in tests): add a ship, a system, a planet, a station, a good, a mission template.
- `docs/MODDING.md` with worked, tested examples and exact commands.
- Galaxy isolation test.
- Base-asset hash rejection test.
- No mods loaded = reference behaviour; golden tests on base only.

Until Phase 8, this document is a design contract, not a claim that mods work.
