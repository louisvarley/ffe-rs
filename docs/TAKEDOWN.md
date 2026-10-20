# Takedown and rights-holder contact

This project is unofficial and is not affiliated with Frontier Developments, David Braben, John Jordan, the FFED3D authors or the Anisotropic mod authors.

If you are a rights-holder (or an authorised agent) and believe that material in this repository infringes your rights, please contact the maintainer through the repository's issue tracker or the contact address published in the repository profile. A private report is preferred for sensitive claims.

Please include:

- your name and organisation;
- the right you own and the work concerned;
- the exact repository path(s) or release artefact(s) at issue;
- the URL/commit/tag where the material appears;
- a statement that you are authorised to act for the rights-holder;
- what you want done (remove, disable, attribute, replace, licence);
- a contact address for follow-up.

## What will happen

1. **Acknowledge** the report as soon as practical.
2. **Assess** the specific material and its role. The project distinguishes:
   - the player-supplied game files (`game/`, `legacy/Elite3/`, `data.bin`) — never committed; if one is found tracked, it is removed immediately;
   - the unlicensed reverse-engineering reference (`legacy/ffed3d*`) — never committed; if found tracked, it is removed;
   - the upscale pack (`upscaled.bin`) — derived from original art; if requested, it is removed from the current tree and, where practical, from history/LFS, and the release stops shipping it;
   - the project's original Rust code/docs — the project owner decides the licence/response; the AI does not choose a licence.
3. **Act promptly**: remove or disable the material, or replace it with a compliant alternative, and document the change in `docs/PORT_LOG.md` if appropriate.
4. **Notify** the reporter of the action and, if relevant, how to obtain a compliant build.

## Good-faith policy

- No game data is distributed.
- The project requires the player's own copy.
- The reference source is used only locally for study and traces; it is git-ignored.
- Golden tests contain hashes/numeric dumps, not frames or assets.
- The upscale pack is opt-in, marked `derived-from-original`, and tied to the player's source version.
- The project will not fight a reasonable request over unlicensed reference material or original art. Preservation of the player's ability to use their own copy is the priority.

## If you are a player

Do not upload game files, `data.bin`, original art, screenshots/frames or save files to issues, pull requests or releases. If you need help with a version, provide hashes and file names/sizes, not the files themselves.

## If you are a contributor

Do not copy code, tables, text, art, models, music or scripts from `legacy/` or `game/` into the repository. Port behaviour, write original Rust, and use traces/hashes. If you are unsure, ask before committing. See `docs/LEGAL.md` and the forbidden-file check.
