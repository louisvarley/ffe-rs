mod library;

use ffe_core::surface::{Fault, Hull, Registers, Scene, Shapes};
use ffe_core::terrain::Terrain;
use library::Library;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case: u32,
    planet: u32,
    ship: u32,
    detail: i32,
    seed: u32,
    outer: i64,
    inner: i64,
    ship_outer: i64,
    ship_inner: i64,
    complex: u8,
    parent: u8,
    rotating: u8,
    words: Vec<u16>,
    planet_matrix: [i32; 9],
    ship_matrix: [i32; 9],
    relpos: [i64; 3],
    fault: bool,
    result: Option<i32>,
    flags: Option<u8>,
    more: Option<u8>,
    object: Option<u8>,
    chunk: Option<u8>,
    planet_flag: Option<u8>,
    distance: Option<i64>,
    altitude: Option<i64>,
}

/// The original F576 (`--planet`) for a ship sphere against the collision stream
/// of the game's models standing in as the complex shape, and for the planets'
/// terrain records the full terrain query beneath it.
#[test]
#[ignore = "Local-only original world data; use tools/reference/verify-world.py"]
fn executed_reference_surface_hits() {
    let library = Library::load();
    let table = library.cache.asset("base:terrain/tables").unwrap().to_vec();
    let maps = library.cache.asset("base:terrain/maps").unwrap().to_vec();
    let input = std::fs::read_to_string(std::env::var_os("FFE_PLANET_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 64 << 20);
    let (mut cases, mut faults, mut hits, mut clear, mut terrain, mut skipped) = (0, 0, 0, 0, 0, 0);
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        if case.fault {
            faults += 1;
            continue;
        }
        let (Some(planet_shape), Some(ship_shape)) =
            (library.shape(case.planet), library.shape(case.ship))
        else {
            skipped += 1;
            continue;
        };
        let planet = Hull {
            index: 5,
            parent: 0,
            rotating: 0,
            complex: true,
            orientation: case.planet_matrix,
            outer: case.outer,
            inner: case.inner,
            seed: case.seed,
            words: case.words.clone(),
        };
        let ship = Hull {
            index: 7,
            parent: case.parent,
            rotating: case.rotating,
            complex: case.complex != 0,
            orientation: case.ship_matrix,
            outer: case.ship_outer,
            inner: case.ship_inner,
            seed: 0,
            words: Vec::new(),
        };
        let mut terrain_state = Terrain::new(&table, &maps);
        let mut scene = Scene {
            terrain: &mut terrain_state,
            shapes: &library,
            detail: case.detail,
        };
        let mut regs = Registers {
            distance: 0xffff_ffff,
            ..Default::default()
        };
        let context = format!(
            "case {} planet {} ship {}",
            case.case, case.planet, case.ship
        );
        let result = match scene.hit(
            &mut regs,
            &ship,
            &ship_shape,
            &planet,
            &planet_shape,
            case.relpos,
        ) {
            Ok(result) => result,
            Err(Fault::OutOfRange) => {
                // The animation words enabled an entry that names a surface the
                // model does not have; the original reads past its data.
                skipped += 1;
                continue;
            }
            Err(fault) => panic!("{fault:?}, {context}"),
        };
        assert_eq!(Some(result), case.result, "result, {context}");
        assert_eq!(
            (
                regs.flags,
                regs.more,
                regs.object,
                regs.chunk,
                regs.planet,
                regs.distance,
                regs.altitude
            ),
            (
                case.flags.unwrap(),
                case.more.unwrap(),
                case.object.unwrap(),
                case.chunk.unwrap(),
                case.planet_flag.unwrap(),
                case.distance.unwrap(),
                case.altitude.unwrap()
            ),
            "registers, {context}"
        );
        cases += 1;
        hits += usize::from(result == 0);
        clear += usize::from(result < 0);
        terrain += usize::from((113..=148).contains(&case.planet));
    }
    assert!(cases > 3000);
    eprintln!("compared {cases} shape tests ({hits} hits, {clear} clear, {terrain} against planets; {faults} faults and {skipped} without data skipped)");
}
