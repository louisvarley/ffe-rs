# Mining machines (F242-F252, F317)

Source: `jjffe-master/ffe.asm` 19190-20000 (`FUNC_000242`..`FUNC_000252`),
25495 (`FUNC_000317`), 82644 (`FUNC_001122`..`FUNC_001124`); strings
`DATA_005053` (0xa200..0xa21d) and the yield table `DATA_005055/56/57`,
`DATA_005058` in `ffedat.asm`. Port: `ffe_core::mining`, `Flight::mining`,
`main.rs::mining_action`/`mining_rows`, `Navigator::{landed_planet,
planet_offset, machine_position}`, `Screen::draw_mining_list`.

- **Deploy (F246).** `d` while landed (the source's `DATA_008870 == 0x34`)
  with a machine aboard drops it beside the ship and frees its 30 t. The
  survey reads the planet's `+0xe0` temperature (over 0x1d9 too hot, under
  0x32 too cold), its model (0x7a sulphur, 0x7f water ice, 0x7d/0x7e/0x80/0x81
  liquid water), a water-ice coin below 0x107 K, and otherwise F922's roll of
  the system's starport tech level (0: 10, 1-2: 9, 3-13: 8, 14-26: 6, else 4)
  times 3 (size word `+0xa6` >= 0x1d) or 2, squared `>> 5` and capped at 26,
  into the 27-row yield table. The two messages are 0xa20c and 0xa211 with the
  surface in degrees C (`+0xe0 - 0x111`).
- **Dig (F243 event 0x10).** Each game day moves a tonne from the reserve to
  the hopper while the hopper holds fewer than ten.
- **Load (F247).** `d` while landed within 1 km of a machine (the port's
  stand-in for the source's contact flag `DATA_008860` bit 0x10) loads the
  hopper into the bay (0xa20f, or 0xa210 when the bay fills). An empty hopper
  with 30 t free takes the machine back aboard (0xa20d), else the hold-full
  message.
- **List (F317).** Status page section 6 lists the machines: find, system,
  planet and hopper, or "In Cargo Bay".

Port choices: a deployed machine's position is kept in its planet's own frame
(the source keeps the object's position and re-creates the object on system
entry). The machine is drawn through the effect path with its model. The
Vaccine machine's model 0x6f is used for code 0x12; its Thargoid mission use
(collecting vaccine from a bunker) is not ported. Mining at a starport is
allowed and is not recorded as F236's "Illegal Mining".
