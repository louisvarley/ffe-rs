//! F236's offence table and the port's `Legal` state: the unlawful-discharge
//! message and fine (`DATA_005039`, `DATA_005040`), the wanted bit
//! (`DATA_008646`), the per-starport ledger (`DATA_009095`) and F783's clear.

use ffe_core::npc::{
    offence_message, Legal, OFFENCES, POLICE_FINE, SMUGGLING, TRADING_ILLEGAL, UNLAWFUL_DISCHARGE,
};

/// Both illegal-trade offences (`DATA_005039[5]` and `[7]`): trading is
/// `F542`'s fine-free record plus F543's 500.0 credit charge, smuggling is
/// `F233`'s 2500 printed / 25000 ledger message with no wanted bit.
#[test]
fn the_illegal_goods_offences_record_their_own_words() {
    let mut legal = Legal::default();
    let trading = legal.record(TRADING_ILLEGAL, Some(2)).expect("recorded");
    assert!(trading.message.is_empty(), "flags 0 shows no F236 message");
    assert_eq!((trading.fine, trading.charged), (0, 0));
    assert!(!legal.wanted, "flags 0 leaves the wanted bit clear");
    assert_eq!(legal.offences, 1);
    legal.charge(POLICE_FINE, Some(2));
    assert_eq!(legal.fines[2], 5_000);
    assert_eq!(POLICE_FINE, 0x1388);

    let smuggling = legal.record(SMUGGLING, Some(1)).expect("recorded");
    assert_eq!(smuggling.fine, 2_500);
    assert_eq!(smuggling.charged, 25_000);
    assert_eq!(
        smuggling.message,
        "Smuggling illegal goods is not permitted here. Pay a $2500 fine."
    );
    assert!(!legal.wanted, "offence 7's flags are 2, so no wanted bit");
    assert_eq!(legal.fines[1], 25_000);
    assert_eq!(legal.offences, 2);
}

#[test]
fn the_offence_table_is_data_005039() {
    let names = [
        "Failure to obey Traffic Control",
        "Launching without clearance",
        "Evading payment of fine",
        "Unlawful discharge of weapon",
        "Illegal Mining",
        "Trading in illegal goods",
        "Attempted bribery",
        "Smuggling illegal goods",
        "Unlawful dumping",
        "Permit Violation",
        "Piracy and murder",
    ];
    for (index, name) in names.iter().enumerate() {
        assert_eq!(OFFENCES[index].name, *name, "offence {index}");
    }
    // The `+0/+4/+8/+0xc` words of the record used by F931/F941
    // (`F237(ship, 3)`): points 20, printed fine 600, witness base 8,
    // wanted/message/starport flags 7.
    let discharge = OFFENCES[UNLAWFUL_DISCHARGE];
    assert_eq!(discharge.points, 20);
    assert_eq!(discharge.fine, 600);
    assert_eq!(discharge.witness, 8);
    assert_eq!(discharge.flags, 0b111);
}

#[test]
fn unlawful_discharge_message_is_the_source_string() {
    assert_eq!(
        offence_message(UNLAWFUL_DISCHARGE, false),
        "Unlawful discharge of weapon is not permitted here. Pay a $600 fine."
    );
    // DATA_005041: a second offence at a starport that already owes a fine.
    assert_eq!(
        offence_message(UNLAWFUL_DISCHARGE, true),
        "Unlawful discharge of weapon can't be tolerated. Pay an extra $600 fine."
    );
}

#[test]
fn the_ledger_is_ten_times_the_printed_fine_and_cash_is_not_touched_here() {
    let mut legal = Legal::default();
    let report = legal
        .player_fired(Some(2))
        .expect("a near starport records the offence");
    assert_eq!(report.fine, 600);
    assert_eq!(report.charged, 6_000);
    assert_eq!(report.message, offence_message(UNLAWFUL_DISCHARGE, false));
    // F236 adds B*10 to DATA_009095[starport]; it does not touch DATA_008888
    // (credits), so the fine is a debt until the fines office collects it.
    assert_eq!(legal.fines, [0, 0, 6_000, 0, 0, 0]);
    assert_eq!(legal.offences, 1);
    assert!(legal.wanted);
    assert!(legal.illegal_fire);
    assert!(!legal.alert);
}

#[test]
fn the_second_offence_at_a_starport_uses_the_extra_message() {
    let mut legal = Legal::default();
    legal.fines[0] = 500;
    let report = legal.player_fired(Some(0)).expect("recorded");
    assert!(report.additional);
    assert_eq!(
        report.message,
        "Unlawful discharge of weapon can't be tolerated. Pay an extra $600 fine."
    );
    assert_eq!(legal.fines[0], 6_500);
}

#[test]
fn the_illegal_fire_flag_spends_the_system_report_even_without_a_starport() {
    let mut legal = Legal::default();
    // F931 sets DATA_008860 bit 2 before F237; F236 returns without a fine
    // when F234 finds no starport. The flag stays set.
    assert!(legal.player_fired(None).is_none());
    assert!(legal.illegal_fire);
    assert!(!legal.wanted);
    assert_eq!(legal.fines, [0; 6]);
    // Later shots in the same system never reach F237 again.
    assert!(legal.player_fired(Some(0)).is_none());
    assert_eq!(legal.fines, [0; 6]);
}

#[test]
fn f783_clears_the_alert_but_not_the_illegal_fire_flag() {
    let mut legal = Legal::default();
    legal.player_fired(Some(0));
    legal.alert = true;
    assert!(legal.clear_alert());
    assert!(!legal.wanted);
    assert!(!legal.alert);
    assert!(legal.illegal_fire);
    // F783 on a clean player is a no-op.
    assert!(!legal.clear_alert());
}

#[test]
fn fines_are_per_starport_and_f655_reads_the_ledger() {
    let mut legal = Legal::default();
    legal.fines[5] = 12_000;
    assert_eq!(legal.owed(Some(5)), 12_000);
    assert_eq!(legal.owed(None), 0);
    // The ledger saturates rather than wrapping.
    let mut full = Legal::default();
    full.fines[0] = u32::MAX - 10;
    full.player_fired(Some(0));
    assert_eq!(full.fines[0], u32::MAX);
}
