# The galaxy map (F840-F863, F1684, F1699-F1701, F1769)

Read from `jjffe-master/ffe.asm`. The map is a 3D scene drawn by the ordinary model machinery: the
camera is a matrix built from the GUI's rotation accumulators, and the scene is a few model runs.

## The scene (F840)

- **Camera**: `F1675` builds the matrix from the accumulators (`F1445`), swapped into the
  object's matrix layout; the zoom is `DATA_008804+0x242` (added to the objects' z), the centre of the view
  `+0x23a/+0x23e/+0x250..+0x258`.
- **Grid** (model `0xa6`): drawn when bit 0 of the map flags (`+0x24e`) is set.
- **Stars** (`F841`, model `0xab`): the 3 by 3 sectors around the centre, each `F841(buffer, object, x, y, draw)`:
  it builds the sector's six-byte vertex records, the object's star fields (`+0xa0` the sector code, `+0xa4` the star
  count, `+0xa6 + 2 i` the star's type bytes) and, with `draw`, runs the model through `F1684`. A hand-made sector then
  also draws its own model (`DATA_008776`).
- **Range sphere** (model `0xaa`, `F862`) and the **selection marker** (model `0xa9`).

## Light points (opcode 30 `0x801e`, F1769) — ported, cache v23

Model `0xab` is one command, `0x801e`. `F1769` loops over the count in global word 4 (`+0xa4`); for star `i` of type
`t` (the low nibble of byte `+0xa6 + 2 i`):

1. if global word 1 has bit 5, a line from vertex `2i` to `2i + 1` (the stalk to the grid) in material `DATA_007828[t]`;
2. a point at vertex `2i` in material `DATA_007820[t]`, its size the packed angle `DATA_007821[t] >> 7`;
3. if the depth of **vertex slot 0** (`[DATA_009200] + 0xc`, not the star's own) is at most `DATA_007822[t]`, a label
   (`F1764`): the string selector `0x8400` (a system name, `sector::system_name`) and the sector code with the star's
   number `i` in bits 26..31. `F1764` queues the label only inside x 2..=317, y 4..=149.

Slot 0 is vertex 0, resolved when the first star's point is drawn, so the label test compares every star against the
first star's depth: the labels of a sector appear together as that star comes within each type's range.

`ffe_core::star_points` holds the tables (cache v23, `base:world/star-points`, 224 bytes: the four tables of 14 dwords
in the order `7820`, `7821`, `7822`, `7828`; RVAs `0x104038`, `0x104070`, `0x1040a8`, `0x10410c` in floppy 1.02);
`Vertices::with_stars` attaches them and the object's stars, `execute_live` runs `F1769`, and
`ModelViewer::execute_stars` runs model `0xab` over a sector's records.

## Verification

- `--map-stars` in `headless.c`: the real `F841` builds the records and star fields of 49 hand-made and 191 generated
  sectors, then the real `F1772` runs model `0xab` with the point, line and label calls recorded (the point probe
  resolves its vertex like `F1699`, so slot 0 is what the original leaves). `verify-map-stars.py` and
  `map_stars_reference.rs`: all 240 cases match (1,589 stars, 831 stalks, 214 queued labels and their depth keys); trace
  SHA-256 `59e7dec3e998d06d7d8011653c47e65ab0eda0df4ad824c3ecf08ab753cb8a9c`.
- `verify-assets.py`: the imported tables equal JJFFE's `DATA_007820`, `7821`, `7822` and `7828`.
- Synthetic tests: `crates/ffe-core/tests/star_points.rs`.

## Not yet

The point's pixels (`F1699`, `F1700`, `F1701`: the dot or shaded disc), the scene assembly of `F840`, the grid, sphere
and marker, the selection logic, the console text and the screen in the playable.
