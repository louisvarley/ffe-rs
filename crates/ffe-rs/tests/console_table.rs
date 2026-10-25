//! `DATA_005100` (`ffedat.asm:4601`): all 82 console records, the state engine
//! (`F261`-`F264`), the lamp routines (`F1116`), the view row (`F991`) and the
//! engines-off equipment page (`F1120`). No content cache is needed.

use ffe_rs::camera::ViewMode;
use ffe_rs::console::{
    buttons, Console, ConsoleState, ShipSystems, BAR, RIGHT_BUTTONS, SCREEN_BUTTONS,
};

#[test]
fn the_table_has_all_82_records_in_the_documented_order() {
    let table = buttons();
    assert_eq!(table.len(), 82);
    // The dump in docs/notes/dashboard.md §4.2, spot-checked at both ends and
    // through the pages this slice draws.
    assert_eq!(
        (table[0].x, table[0].y, table[0].image(), table[0].radio),
        (296, 178, Some(102), 1)
    );
    assert_eq!((table[0x0e].x, table[0x0e].y), (60, 172));
    assert_eq!(table[0x0e].cur, 1, "F1 starts lit");
    assert_eq!((table[0x25].x, table[0x25].y), (40, 192));
    assert_eq!(table[0x2b].bitmaps[0..2], [238, 247]);
    assert_eq!(table[0x37].bitmaps[0..2], [212, 213]);
    assert_eq!(table[0x3a].bitmaps[0], 268, "60x45 ship diagram");
    assert_eq!(
        table[0x3b].bitmaps,
        [282, 282, 346, 278, 279, 276, 277, 344, 343, 345]
    );
    assert_eq!((table[0x51].x, table[0x51].y), (17, 179));
}

#[test]
fn the_radio_groups_match_the_original() {
    let table = buttons();
    let count = |radio: u16| table.iter().filter(|b| b.radio == radio).count();
    assert_eq!(count(1), 36, "views/equipment/page slots");
    assert_eq!(count(2), 4, "screen buttons");
    assert_eq!(count(3), 7, "meter including the pause duplicate");
    assert_eq!(count(5), 2);
    assert_eq!(count(6), 2);
    assert_eq!(count(7), 2);
    assert_eq!(count(10), 2, "missile/combat pair");
    for radio in 13..=22 {
        assert_eq!(count(radio), 1, "equipment icon {radio:#x}");
    }
    for radio in 23..=37 {
        assert!(count(radio) >= 1, "lamp group {radio}");
    }
    // The visible byte is 0 in the data; F1's current image is already 1.
    assert!(table.iter().all(|button| !button.visible));
    assert_eq!(table[0x38].cur, 1);
}

#[test]
fn f262_sets_the_image_and_resets_the_radio_group() {
    let mut console = Console::new();
    console.set_image(0x15, 1);
    console.set_image(0x18, 1);
    assert_eq!(
        console.image(0x15),
        Some(159),
        "0x18's group-1 reset returns 0x15 to image 0"
    );
    assert_eq!(console.image(0x18), Some(0xb1));
    // Setting an image the record already has is a no-op.
    console.set_image(0x18, 1);
    assert_eq!(console.image(0x18), Some(0xb1));
    // Image 0 does not reset the radio group.
    console.set_image(0x18, 0);
    assert_eq!(console.image(0x18), Some(162));
}

#[test]
fn the_meter_and_screen_buttons_are_the_table_records() {
    for (index, (x, y, grey, lit)) in BAR.into_iter().enumerate() {
        let record = buttons()[0x26 + index];
        assert_eq!((record.x, record.y), (x, y));
        assert_eq!(record.bitmaps[0..2], [grey, lit]);
    }
    for (x, y, off, on, key) in SCREEN_BUTTONS {
        let record = buttons()[0x0b + usize::from(4 - key)];
        assert_eq!((record.x, record.y), (x, y));
        assert_eq!(record.bitmaps[0..2], [off, on]);
    }
    for (x, y, off, on, click) in RIGHT_BUTTONS {
        let id = match click {
            ffe_rs::console::Click::Hyperspace => 0x0f,
            ffe_rs::console::Click::Labels => 0x10,
            ffe_rs::console::Click::Navigation => 0x11,
            _ => unreachable!(),
        };
        let record = buttons()[id];
        assert_eq!((record.x, record.y), (x, y));
        assert_eq!(record.bitmaps[0..2], [off, on]);
    }
}

#[test]
fn f1116_lights_the_original_lamps() {
    let mut console = Console::new();
    let systems = ShipSystems {
        // Autopilot at speed 0x2e with f0 clear: 0x47; e8 < 0x28: 0x48 image 1.
        e8: 0x2e,
        ..ShipSystems::default()
    };
    console.apply_flight(ViewMode::Front, &ConsoleState::default(), &systems);
    assert!(console.is_visible(0x37) && console.is_visible(0x38));
    assert!(console.is_visible(0x39));
    assert!(console.is_visible(0x47), "the autopilot lamp");
    assert!(!console.is_visible(0x45));
    assert!(console.is_visible(0x46));
    assert_eq!(console.image(0x38), Some(215));
    assert_eq!(console.image(0x46), Some(266));
    assert_eq!(
        console.image(0x48),
        Some(264),
        "e8 >= 0x28 returns 0x48 to image 0"
    );
    // e9 bit 2: both 0x38 and 0x46 take image 0 (label/flag state).
    let systems = ShipSystems {
        e9: 2,
        ..ShipSystems::default()
    };
    console.apply_flight(ViewMode::Front, &ConsoleState::default(), &systems);
    assert_eq!(console.image(0x38), Some(214));
    assert_eq!(console.image(0x46), Some(265));
    // e8 == 2 also lights 0x39.
    let systems = ShipSystems {
        e8: 2,
        ..ShipSystems::default()
    };
    console.apply_flight(ViewMode::Front, &ConsoleState::default(), &systems);
    assert_eq!(console.image(0x39), Some(217));
}

#[test]
fn f991_shows_the_view_row_and_only_hides_the_turret_on_two_mount_ships() {
    let console_state = ConsoleState::default();
    let mut console = Console::new();
    console.apply_flight(
        ViewMode::Rear,
        &console_state,
        &ShipSystems {
            mounts: 2,
            ..ShipSystems::default()
        },
    );
    for id in [0x15usize, 0x17, 0x18, 0x19] {
        assert!(console.is_visible(id), "{id:#x} is always shown");
    }
    assert!(!console.is_visible(0x16), "the turret needs > 2 mounts");
    assert_eq!(console.image(0x17), Some(0xb0), "rear lit");
    assert_eq!(console.image(0x18), Some(162));
    // Four mounts: the turret button appears and lights in a turret view.
    let mut console = Console::new();
    console.apply_flight(
        ViewMode::TopTurret,
        &console_state,
        &ShipSystems {
            mounts: 4,
            ..ShipSystems::default()
        },
    );
    assert!(console.is_visible(0x16));
    assert_eq!(console.image(0x16), Some(0xaf));
    // F10's missile view: with the Missile Viewer and player+0xef, 0x14 is
    // shown and lit, and the other view buttons are off (radio group 1).
    let mut console = Console::new();
    console.apply_flight(
        ViewMode::Missile,
        &console_state,
        &ShipSystems {
            mounts: 2,
            general: 1 << 9,
            ef: 1,
            ..ShipSystems::default()
        },
    );
    assert!(console.is_visible(0x14));
    assert_eq!(console.image(0x14), Some(0xad), "missile view lit");
    assert_eq!(console.image(0x18), Some(162), "front off");
}

#[test]
fn f1120_draws_the_equipment_page_and_hides_the_lamps() {
    // Bits 2, 19, 23, 24 and 26 of the general-equipment dword: military
    // cameras, ECM, Naval ECM, hyperspace cloud analyser and energy bomb.
    let systems = ShipSystems {
        slots: [0, 1, 2, 0, 0, 0, 0, 0, 0, 3],
        general: (1 << 2) | (1 << 19) | (1 << 23) | (1 << 24) | (1 << 26),
        p511e: 1,
        e8: 0x34,
        data_247: false,
        ..ShipSystems::default()
    };
    let mut console = Console::new();
    console.apply_flight(
        ViewMode::Front,
        &ConsoleState {
            status_page: true,
            ..ConsoleState::default()
        },
        &systems,
    );
    assert!(console.is_visible(0x3a), "the ship diagram");
    assert!(console.is_visible(0x4f));
    assert!(console.is_visible(0x4a) && console.is_visible(0x4b));
    assert!(console.is_visible(0x4c) && console.is_visible(0x4d));
    assert!(console.is_visible(0x4e));
    assert_eq!(console.image(0x4a), Some(272), "Naval ECM is image 1");
    for id in [0x45usize, 0x46, 0x47, 0x48, 0x49] {
        assert!(!console.is_visible(id), "{id:#x} is hidden on the page");
    }
    // Slots 1, 2 and 9: icons 0x3c, 0x3d and 0x44 with the low nibble images.
    assert_eq!(console.image(0x3c), Some(282));
    assert_eq!(console.image(0x3d), Some(346));
    assert!(console.is_visible(0x44));
    assert!(!console.is_visible(0x3b));
    // F1120 lights 0x19.
    assert_eq!(console.image(0x19), Some(178));
    // The F1 page restores the lamps and hides the page.
    console.apply_flight(ViewMode::Front, &ConsoleState::default(), &systems);
    assert!(!console.is_visible(0x3a));
    assert!(console.is_visible(0x45) || console.is_visible(0x46));
    assert_eq!(console.image(0x19), Some(163));
}

#[test]
fn f991_shows_the_equipment_icons_its_dword_bits_select() {
    let console_state = ConsoleState::default();
    // Bits 5 (Navigation computer), 6 (Combat Computer), 27 (Escape Capsule)
    // and 9 (Missile Viewer) light 0x11-0x14; 0x14 also needs player+0xef.
    let mut systems = ShipSystems {
        general: (1 << 5) | (1 << 6) | (1 << 9) | (1 << 27),
        ..ShipSystems::default()
    };
    let mut console = Console::new();
    console.apply_flight(ViewMode::Front, &console_state, &systems);
    for id in [0x11usize, 0x12, 0x13] {
        assert!(console.is_visible(id), "{id:#x} follows its bit");
    }
    assert!(!console.is_visible(0x14), "0x14 also needs player+0xef");
    systems.ef = 1;
    console.apply_flight(ViewMode::Front, &console_state, &systems);
    assert!(console.is_visible(0x14));
    // Bit 3 is the other half of F991's 0x12 test (the Stowmaster fighter).
    let systems = ShipSystems {
        general: 1 << 3,
        ..ShipSystems::default()
    };
    console.apply_flight(ViewMode::Front, &console_state, &systems);
    assert!(console.is_visible(0x12));
    // No equipment: all four stay hidden.
    console.apply_flight(ViewMode::Front, &console_state, &ShipSystems::default());
    for id in [0x11usize, 0x12, 0x13, 0x14] {
        assert!(!console.is_visible(id));
    }
}

#[test]
fn f1120_draws_the_new_games_two_missile_icons() {
    let systems = ShipSystems::default().with_loadout(&ffe_core::equipment::Loadout::new_game());
    // F998 type 0: scanner, autopilot, atmospheric shielding and the
    // navigation computer (bit 5).
    assert_eq!(systems.general, 0x4024_0020);
    let mut console = Console::new();
    console.apply_flight(
        ViewMode::Front,
        &ConsoleState {
            status_page: true,
            ..ConsoleState::default()
        },
        &systems,
    );
    // F998 fits two 0x82 (KL760) missiles: slots 0 and 1, image 2, records
    // 0x3b and 0x3c at (14,165) and (33,164).
    assert_eq!(buttons()[0x3b].bitmaps[2], 346, "image 2 is bitmap 346");
    assert!(console.is_visible(0x3b) && console.is_visible(0x3c));
    assert_eq!(console.image(0x3b), Some(346));
    assert_eq!(console.image(0x3c), Some(346));
    for id in 0x3d..=0x44 {
        assert!(!console.is_visible(id), "{id:#x} has no missile");
    }
}
