# Legal notice and project stance

This is not legal advice. It records the project's understanding and intended conduct so that contributors and users can make informed decisions. If you are a rights-holder, see `docs/TAKEDOWN.md`.

## 1. What the game is

**Frontier: First Encounters** (also known as **Elite 3**) is a commercial computer game originally published in 1995. The game, its code, artwork, text, music, sound effects and documentation are the copyright of **Frontier Developments** and **David Braben** (and their licensors). “Elite”, “Frontier” and “First Encounters” are associated trademarks. The game is still commercially available in some forms; the copyright has not expired.

The copy in `game/` (and `legacy/Elite3/`) is a player-supplied installation of the DOS floppy release, English 1.02. It is **not** part of this repository and must never be committed.
The additional root-level release/patch folders, including the CD image, are
also local-only player inputs, not distributable project files.

## 2. What FFED3D / JJFFE is

The source in `legacy/ffed3d-master/` is a community reverse-engineering project:

- It is based on **JJFFE** (John Jordan's reverse-engineered Windows executable), version 2.8a6.
- It includes the **Anisotropic mod** (version 0.9c) and further **FFED3D** changes.
- Its splash screen states: “This is a reverse engineered version of a commercial game. It is illegal to own it even if you own the original.”
- The copy supplied for this project contains **no licence statement**. There is no grant to copy, modify, redistribute or sublicense it. It descends from Frontier Developments' code and contains their data/tables.

The separately supplied `jjffe-master/` identifies as JJFFE **2.8a7** and is now
the selected gameplay reference. The FFED3D ancestry above must not be applied
to that separate tree. Its use as a reference likewise does not establish
redistribution rights. The owner's approval of local work is not permission
from third-party rights-holders.

Therefore:

- Supplied reference trees remain unchanged; the owner permits local study
  builds, patches and instrumentation in separate copies. They are not treated
  as licensed open-source dependencies.
- Its source, binaries, models, textures, shaders, scripts and data are **not committed** to this repository and are **not redistributed**.
- The planned Rust port is a source-informed, AI-assisted reimplementation.
  It must not vendor or ship the reference code/data; local reference builds
  are for validation only. Studying recovered source and writing new Rust does
  not by itself establish clean-room status or a redistribution grant.
- If a rights-holder or the FFED3D authors object to this use, the project will stop using the reference and comply promptly (see `docs/TAKEDOWN.md`).

## 3. What this repository contains

Allowed:

- Original Rust source, documentation, tests, schemas and tooling.
- Golden **hashes and numeric dumps** derived from running the reference engine against the player's files (`tests/golden/`).
- A placeholder `LICENSE` for the project owner to fill in. No licence has been chosen by the AI.

Never allowed:

- `game/` or any copy of the original game files.
- `legacy/Elite 3/`, `legacy/Elite3/`, `legacy/ffed3d/`, `legacy/ffed3d-master/` (no licence).
- `data.bin` (contains converted original content).
- `upscaled.bin` (derived art; local only pending explicit rights review).
- `jjffe-master/`, `ffed3d-master/`, and root `Frontier-First-Encounters*`
  release/patch directories.
- `to-be-upscaled/` (un-upscaled original art).
- `upscaled/` working files (unpacked upscaled art).
- `extracted/`, frame dumps, screenshots or captures derived from game assets.
- Any FFED3D/JJFFE source, binary, model, texture, shader, script or resource.
- Any save file or commander file from a player.

The CI check (`ci/check_forbidden.py`) fails if a forbidden path is tracked.

The experimental playable example uses `minifb` 0.28 (MIT) plus the workspace's
existing serde/PNG/cache stack. V0.2 removes the earlier `font8x8` placeholder
dependency and imports the player's original font bytes into their private cache.
No original font or console pixels are embedded in source/binaries/test fixtures;
the renderer reads them from the supplied executable via the importer.
V0.3 likewise imports two station vertex record blocks only into the private
cache. Public tests use synthetic programs; executed-reference tests retain
numeric metadata/hashes only. No original station vertices, static meshes or
captured primitive streams are compiled into the native executable.
V0.4 imports two root drawing-command streams into the same private cache.
Public command tests use synthetic records; the separate local reference probe
redirects four emission calls only in an ignored generated assembly copy.
Neither that derived source/binary nor original command bytes are distributed.
V0.5 imports station normal bytes only into the private cache. Public regression
tests use synthetic normal/vertex records; local executed-reference comparisons
retain hashes/numeric results, not original normal tables or captured art.
V0.6 imports the original sine table into the same private cache, never Rust
source or public test fixtures. Synthetic ramp tables test the integer helpers;
local differential traces remain disposable and are not distributed.
V0.7 dispatcher tests use newly authored synthetic command programs, not copied
station streams. The local command probe reuses the same four emission-call
hooks; generated reference sources/binaries and numeric traces stay private.
Actual station streams are read only from the player's cache to verify explicit
unsupported-operation rejection, never stored in public fixtures.
V0.9 depth/order comparisons use synthetic points, command operands and marker
callbacks. The local unmodified original allocator/group dispatcher/sorter is
executed without adding assembly patches; numeric traces are disposable.
No game primitive storage, original mesh records or captured art are added to
the public regression fixtures.
Dependency licences/notices, including transitive/platform dependencies, must
accompany any future binary distribution; Cargo.lock records the resolved set.
This does not resolve the separate source-informed-reimplementation or game-art
rights above. Local original-art captures remain private and untracked.

## 4. The player's copy

`ffe-rs` requires the player to own and supply their own copy of First Encounters. The conversion tool reads that copy locally and produces `data.bin` on the player's machine. Neither the tool nor the repository distributes the game. The finished product must not include a copy of the game, and the release ZIP must not include `data.bin` or `game/` content.

The tool must detect unsupported versions and refuse to convert them rather than guessing or downloading game data.

## 5. The upscale pack

`upscaled.bin` is:

- generated by the player (or a community pack) from the player's own art;
- **derived from original art**; its manifest marks each asset `derived-from-original` or `original-work`;
- not a replacement for the game and not usable without `data.bin`/the player's copy;
- local-only unless appropriate distribution rights are established. It is
  ignored by git and rejected by CI. The historical LFS attribute does not
  override that policy.

Do not upload un-upscaled original art, `data.bin`, `game/`, or screenshots/frames to this repository or to issue trackers.
Do not upload derived packs merely because AI generated/upscaled them or because
the engine also requires the original game. "Rights-holder tolerance" and a
takedown policy are not a substitute for permission. Source/art license selection
remains a release prerequisite, not a new technical approval request.

## 6. AI assistance

Parts of this project are written with AI assistance. The AI is used to study the reference, propose designs, write Rust, tests and documentation. A human is responsible for reviewing, testing and licensing the result. AI assistance does not change the copyright status of the original game or the reference source.

## 7. No warranty / no licence chosen

The project is provided for preservation, research and personal use. It comes with no warranty. **No open-source licence has been selected.** The `LICENSE` file is a placeholder; until the project owner fills it in, do not assume any permission to redistribute the Rust code. The absence of a licence is intentional and must be resolved by the owner, not by the AI.

## 8. Trademarks and attribution

“Frontier: First Encounters”, “Elite”, “Frontier” and related marks belong to their owners. This project is unofficial and is not endorsed by Frontier Developments, David Braben, John Jordan, the FFED3D authors or the Anisotropic mod authors. Attribution to the original creators and the reverse-engineering community is given in `README.md` and `docs/INVENTORY.md`; it does not imply endorsement.

## 9. Reporting concerns

If you believe this repository contains material that infringes your rights, follow `docs/TAKEDOWN.md`. The maintainers will respond promptly, remove or disable the material, and preserve the player's local data where possible.
