//! Ship-name and statistics cross-checks.
//!
//! A model's display name is the statistics block's `+0xe` ship-name ffcode
//! expanded through `DATA_004682` (F437/F439/F282 -> F1345), not the model
//! table slot. `+0xc` is the shield word. The local test compares all 466
//! model-table slots and the shipyard's 58 generated hulls against the cache:
//!
//! ```text
//! FFE_WORLD_CACHE=<data.bin> cargo test -p ffe-rs --test ship_names -- --ignored
//! ```

use ffe_rs::model_viewer::ModelViewer;
use ffe_rs::ships::{model_name, name_at, npc_spec, ShipList, ShipStats};

fn word(block: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([block[at], block[at + 1]])
}

fn cache() -> ffe_content::VerifiedCache {
    ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
        .unwrap()
}

#[test]
fn ship_stats_split_the_shield_and_name_code_words() {
    let mut block = [0u8; 48];
    block[0xc..0xe].copy_from_slice(&150i16.to_le_bytes());
    block[0xe..0x10].copy_from_slice(&0x4028u16.to_le_bytes());
    block[0xa..0xc].copy_from_slice(&71u16.to_le_bytes());
    let stats = ShipStats::from_block(&block).unwrap();
    assert_eq!(stats.shield_strength, 150, "the +0xc shield word");
    assert_eq!(stats.name_code, 0x4028, "the +0xe name ffcode");
    assert_eq!(stats.price_mass, 71, "F437's +0xa cost word");
    // F1345 masks the code to the table index, so the shipyard's 0x4028 is
    // "Spar attack fighter" (index 40), not the slot-43 "Turner Class".
    let mut table = Vec::new();
    for index in 0..=45u16 {
        let text = match index {
            40 => "Spar attack fighter",
            43 => "Turner Class",
            _ => "Mine",
        };
        table.extend_from_slice(text.as_bytes());
        table.push(0);
    }
    assert_eq!(
        name_at(&table, 0x4028).as_deref(),
        Some("Spar attack fighter")
    );
    assert_eq!(name_at(&table, 0x402b).as_deref(), Some("Turner Class"));
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn every_model_name_expands_from_its_statistics_code() {
    let cache = cache();
    let table = cache
        .asset("base:text/model-names")
        .expect("cache v11 model names");
    let names: Vec<&[u8]> = table.split(|byte| *byte == 0).collect();
    let records = cache.asset("base:model/records").unwrap();
    let stats_table = cache.asset("base:ship/stats").unwrap();

    let mut with_stats = 0;
    let mut named = 0;
    let mut ship_codes = 0;
    for model in 0..466u32 {
        let Some(info) = ffe_core::objects::ModelInfo::read(records, stats_table, model) else {
            continue;
        };
        let Some(block) = info.stats else {
            continue;
        };
        with_stats += 1;
        let stats = ShipStats::from_block(&block).expect("a non-zero statistics block");
        if (0x4000..0x8000).contains(&stats.name_code) {
            ship_codes += 1;
        }
        let index = usize::from(stats.name_code & 0x3fff);
        let Some(entry) = names.get(index) else {
            continue;
        };
        let expected = String::from_utf8_lossy(entry).trim().to_owned();
        if expected.is_empty() {
            continue;
        }
        named += 1;
        assert_eq!(
            model_name(&cache, model).as_deref(),
            Some(expected.as_str()),
            "model {model:#04x}: the +0xe code must name it, not the slot"
        );
    }
    // Floppy 1.02: 84 slots have a statistics block. All 84 resolve a name
    // through +0xe; 73 of them are 0x40xx ship codes, the rest 0 ("Mine").
    assert_eq!(with_stats, 84);
    assert_eq!(named, 84);
    assert_eq!(ship_codes, 73);
}

#[test]
#[ignore = "Local-only: needs the content cache (FFE_WORLD_CACHE)"]
fn the_shipyard_and_ship_list_use_the_same_expanded_names() {
    let cache = cache();
    let ships = ShipList::from_cache(&cache, &ModelViewer::new(&cache).unwrap());
    let shipyard = ffe_core::shipyard::MODELS;
    assert_eq!(shipyard.len(), 58, "the 58 drawable shipyard hulls");

    for hull in shipyard {
        // The generated table trims the one trailing space in DATA_004682
        // ("Harrier ").
        assert_eq!(
            model_name(&cache, hull.model).as_deref(),
            Some(hull.name.trim()),
            "model {:#04x} does not expand its +0xe code",
            hull.model
        );
        // ShipList leaves "Unknown" hulls out of Page Up/Down; the shipyard
        // still lists them.
        if hull.name != "Unknown" {
            assert_eq!(
                ships.name(hull.model),
                Some(hull.name.trim()),
                "model {:#04x} differs between the ship list and F437's row",
                hull.model
            );
        }
        let info = ffe_core::objects::ModelInfo::read(
            cache.asset("base:model/records").unwrap(),
            cache.asset("base:ship/stats").unwrap(),
            hull.model,
        )
        .unwrap()
        .stats
        .unwrap();
        assert_eq!(
            word(&info, 0xc),
            hull.shield,
            "model {:#04x}: the shipyard shield is +0xc",
            hull.model
        );
    }

    // The names the old slot-indexed lookup got wrong, with their real shield
    // words from +0xc.
    for (model, name, shield, old_slot_name) in [
        (0x1f, "Saker III fighter", 85, "Viper Defence MkII"),
        (0x17, "Eagle long range fighter", 52, "Sidewinder"),
        (0x1e, "Viper Defence Craft", 130, "Merlin attack fighter"),
        (0x22, "Viper Defence MkII", 140, "Cobra Mk III"),
        (0x26, "Cobra Mk III", 150, "Lanner"),
        (0x2b, "Spar attack fighter", 170, "Turner Class"),
        (0x2e, "Turner Class", 310, "Unknown"),
        (0x46, "Lynx Bulk Carrier", 800, "Long-range Cruiser"),
        (0x47, "Long-range Cruiser", 100, "<outside the old range>"),
    ] {
        assert_eq!(ships.name(model), Some(name), "model {model:#04x}");
        assert_ne!(ships.name(model), Some(old_slot_name), "model {model:#04x}");
        let spec = npc_spec(&cache, model).unwrap();
        assert_eq!(spec.name, name, "model {model:#04x}");
        assert_eq!(spec.shield_strength, shield, "model {model:#04x} +0xc");
    }
}
