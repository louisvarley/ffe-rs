# Legal status, unlawful fire and fines

The owner's anchor: **"Unlawful discharge of a weapon is not permitted here. Pay a
4500 Fine."** This note maps that message to DOS FFE and records what the port
implements. **The 4500 is not in the source**: `DATA_005039`'s unlawful-discharge
record is `600`, and the executed original prints **"Unlawful discharge of weapon
is not permitted here. Pay a $600 fine."** while adding **6000 CR** to the
starport's fine ledger. See "The 4500 discrepancy" below.

## 1. Source map

| What | Where (`jjffe-master/ffe.asm`, `ffedat.asm`) |
|---|---|
| Player laser fire, illegal-fire gate | `F931` 69550; player branch 69592-69601: `[ship+0x86] == DATA_008857` (the ship is in the current system), `test DATA_008860,0x4`, then `DATA_008860 |= 4` and `F237(ship, 3)` |
| Player missile fire | `F941` 70458-70475: the same test and `F237(ship,3)`, then `DATA_008860 |= 4`; `DATA_008889++` |
| Offence dispatcher | `F237` 18924-18935: `F236(offence, ship, DATA_008996)` |
| Offence record and fines | `F236` 18761-18920: the `DATA_005039` record, the starport witness, `DATA_009095[] += fine*10`, the `DATA_008908[]` points, the `DATA_009093[]` offence bitmask, the message (`0x9e02+offence`), `DATA_008646 |= 1` |
| Starport witness / law | `F234` 18661-18713 and `F235` 18717-18757: the nearest active object with `+0x87 == 1` and `+0x14c & 0x10` (a **starport**, not an ordinary station), by `+0x88` (exponential distance); a random roll against `+8` (the record's `witness` byte) plus `DATA_009010` (system danger); no starport returns 0 and `F236` leaves the offence unrecorded. There is **no anarchy/law gate** in DOS FFE: danger only scales the witness roll. FFED3D's commented-out `DATA_CurrentDanger >= 2` lawless check is not in the shipped code (and its `IsCloseToStarport` drops danger entirely) |
| Exponential distance | `F1680` 101918-101925: `+0x88 = max(0, msb(distance) - 0xd) - object[+0x7e]`; `0xa` is "maximum distance about 20km" (`ffed3d-master/ffe.asm:72033`) |
| FFED3D's deterministic form | `ffed3d-master/win/ffemisc.cpp:265-309` `IsCloseToStarport`: nearest starport `dist_cam < 0x13`, and unlawful discharge only once per system (`DATA_PlayerFlags & 4`) |
| Fine table | `DATA_005039` `ffedat.asm:4362-4382`: eleven 14-byte records `{u32 points, u32 fine, u32 witness, u8 flags, u8 pad}` |
| Messages | `DATA_005040` / `DATA_005041` `ffedat.asm:4384/4386`; offence names `DATA_005042`-`DATA_005052` 4388-4408 |
| Wanted bit | `F236` 18897-18905: `DATA_008646 |= 1` unless the offender's model is 0x45/0x4f |
| Police chase (alert) | `F238` 18939-19037: `DATA_008647` 1/0xff and F239's police spawn/decay; `DATA_008647` is not set by the offence itself |
| Clear when the chase ends | `F783` 51110-51123: an object with `+0x118 == 0xe` (police) is removed → `DATA_008647 = 0`, `DATA_008646 &= ~1` |
| Docking fine check | `F655` 44642-44653: `DATA_009095[DATA_008996] >= 0x2710` → fine refusal; `F366` 27854-27893 pays it (the fines office) |
| No HUD badge | the flight HUD never reads `DATA_008646/47`; F2's status screen has `LEGAL STATUS`/`Clean..Fugitive` (`DATA_005189`, `DATA_005201`-`005205`) but that screen is not ported |

### The table (`DATA_005039`)

| # | Offence | points `+0` | fine `+4` | witness `+8` | flags `+0xc` |
|---|---|---|---|---|---|
| 0 | Failure to obey Traffic Control | 5 | 1000 | 0 | 3 |
| 1 | Launching without clearance | 8 | 500 | 0 | 2 |
| 2 | Evading payment of fine | 15 | 2000 | 0 | 3 |
| 3 | **Unlawful discharge of weapon** | 20 | **600** | 8 | 7 |
| 4 | Illegal Mining | 20 | 200 | 16 | 7 |
| 5 | Trading in illegal goods | 20 | 0 | 8 | 0 |
| 6 | Attempted bribery | 25 | 0 | 0 | 0 |
| 7 | Smuggling illegal goods | 30 | 2500 | 0 | 2 |
| 8 | Unlawful dumping | 40 | 3000 | 22 | 7 |
| 9 | Permit Violation | 80 | 4000 | 99 | 7 |
| 10 | Piracy and murder | 300 | 10000 | 20 | 7 |

`F236` adds **ten times** the printed fine to the per-starport ledger
(`DATA_009095[system]`, 18823-18834) and shows the `+4` amount in the message
(18870-18888). Unlawful discharge therefore prints `$600` and owes `6000`.

### When the money is taken

F236 only records a debt. `F655` refuses docking when the ledger is `>= 10000`;
`F366` is the police/fines window that takes the money when credits are enough
(`sub [0x114], fine`). The executed original's cash stays `1000` when the
offence is recorded, so the port also leaves `Flight::credits` alone.

## 2. What the port implements

- `ffe_core::npc`: [`Offence`], [`OFFENCES`] (the decoded `DATA_005039`),
  [`offence_message`] (`DATA_005040/41`), [`OffenceReport`] and the extended
  [`Legal`] (`wanted`, `alert`, `illegal_fire`, `offences`, `fines[6]`).
  `Legal::player_fired(starport)` is F931/F941 + F236: sets `DATA_008860` bit 2
  once, records the fine/wanted state only when a starport ordinal is given, and
  returns the message. `Legal::clear_alert()` is F783; it does not clear the
  illegal-fire flag.
- `ffe_rs::navigation`:
  - `distance_byte` is F1680's `max(0, msb(distance) - 0xd)` (the model-scale
    term is omitted; that makes the test stricter for large objects).
  - `starport_witness`/`Navigator::starport_in_range` is F234/F235 in FFED3D's
    deterministic form: the nearest starport whose byte is `< 0x13`
    (`JURISDICTION_DISTANCE`), returned as the `DATA_008996` ordinal.
  - `Navigator::player_fired` reports the offence, provokes every police and
    military ship (F238's response) and sets `alert` (`DATA_008647`).
  - `tick_ships` clears `wanted`/`alert` and emits `ShipEvent::AlertCleared`
    when a police ship is removed (F783).
  - `fine_at` is wired into the player's F655 docking requests, so a ledger of
    10000 or more refuses clearance.
  - `jump` resets `illegal_fire` at the system boundary (see deviations).
- `ffe_rs::main`: the first laser shot or missile launch calls
  `Navigator::player_fired`; its message becomes the frame's message-log entry
  (and appends " Police are responding." when a police ship turns). The F783
  event logs "The police have called off the chase.".
- No HUD badge is added: the source has none in flight.

## 3. Evidence

- `crates/ffe-core/tests/legal.rs`: the table, the exact message, `6000` ledger
  vs `600` printed fine, cash not touched, the second-offence "extra" message,
  no-starport behaviour, F783 clear, per-starport ledger and saturation.
- `crates/ffe-rs/src/navigation.rs` unit tests: `distance_byte` boundary at
  `0x12/0x13`, and the starport witness accepting only a starport inside
  `0x13`.
- `crates/ffe-rs/tests/npc_ai.rs::firing_at_hope_is_fined_and_shows_f236s_message`
  (ignored/local cache): the new game's Hope pad is a witness, the message and
  6000 fine, `wanted`, police hostility, F783 clear and the frame
  `docs/evidence/legal/legal-hope.png`.
- `docs/evidence/legal/original-legal-probe.jsonl`: the executed JJFFE 2.8a7
  `--legal-probe` at the new game's Ross 154 pad. Frame 164/165:
  `illegal 0→4`, `wanted 2→3`, `fines [0,0,0,0,0,0]→[0,0,6000,0,0,0]`,
  `cash 1000`, and the message **"Unlawful discharge of weapon is not permitted
  here. Pay a $600 fine."** (`DATA_008996=2` is the source's starport index; the
  port uses the ordinal among its starports, 0 at Hope.)

## 4. The 4500 discrepancy

`4500` appears nowhere in `DATA_005039`, in `FIRSTENC.EXE` (all four supplied
DOS variants), or in FFED3D's table; the nearest values are 6000 (the charge) and
600 (the printed amount). The executed original prints `$600` and owes `6000`.
The owner's memory/screenshot may be from a different version or may have
combined the message with another fine. The port follows the source; if the
owner wants the 4500 variant, it is a deliberate table/format change, not a port
bug, and should be recorded as such.

## 5. Remaining / owner decisions

- **Police spawns** (F238/F239) are not ported: the response provokes F688's
  existing police and military traffic. Spawning extra police needs F927 slots
  and the F239 object template.
- **Fines office** (F366/F543/F552, the police contact) is not ported; the debt
  is visible in `Legal::fines` and the F655 refusal, but cannot be paid yet.
- **F2 legal-status screen** (`LEGAL STATUS`, `Clean`/`Offender`/`Criminal`/
  `Outlaw`/`Fugitive`) is not ported; no badge is drawn in flight.
- **Illegal-fire reset**: DOS only clears `DATA_008860` in F1000's reset
  (`ffe.asm:74307`); the port also clears it on a system entry, matching
  FFED3D's once-per-system check. Without this the first shot of a game would
  disable the offence forever. Decide whether to keep that deviation.
- **Witness model scale**: `distance_byte` omits F1680's `- object[+0x7e]`
  model-scale term. Including it would widen the region for large starports.
- **Fine scope**: the source's `DATA_009095` has six slots and is indexed by
  `DATA_008996`, so fines are effectively per starport number, not per body
  across the galaxy. The port keeps the six-slot array and the current
  system's starport ordinal.
