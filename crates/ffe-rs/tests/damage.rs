//! Player damage, the repair service and F938/F929 (dashboard `gameplay-3`).
use ffe_core::equipment::Loadout;
use ffe_core::laser::Damage;
use ffe_rs::damage::{Damage as DamageState, SystemEvent, THRUSTER_NAMES};
use ffe_rs::flight::Flight;
use ffe_rs::ships::ShipStats;
use ffe_rs::station::{self, RepairAction};

fn stats() -> ShipStats {
    ShipStats {
        forward_thrust: 0x100,
        reverse_thrust: -0x80,
        gun_mounts: 1,
        fuel_scoop_mounting: false,
        mass: 25,
        price_mass: 38,
        cargo: 20,
        shield_strength: 1,
        name_code: 0,
        missiles: 2,
        drive: -2,
        rating: 0,
        eye: [0; 2],
        front_gun: [0; 2],
        rear_gun: [0; 2],
        top_turret: [0; 2],
        bottom_turret: [0; 2],
    }
}

fn flight() -> Flight {
    let mut flight = Flight {
        hull: [100, 100],
        damage: DamageState {
            thrusters: Some([0x80, -0x80, 0x80, -0x80, 0x100, -0x80]),
            ..DamageState::default()
        },
        ..Flight::default()
    };
    flight.combat.drive = 2;
    flight
}

#[test]
fn f950_thresholds_break_the_drive_or_one_named_thruster() {
    let mut flight = flight();
    // 0x100 (< 0x888): F947 destroys the drive and empties the energy.
    let (damage, event) = flight.take_damage_roll(10, 25, 0x100, 0);
    assert_eq!(damage, Damage::Hull);
    assert_eq!(event, Some(SystemEvent::DriveDamaged));
    assert_eq!(flight.combat.drive, 0);
    assert_eq!(flight.loadout.drive, 0);
    assert_eq!(flight.combat.energy, 0);
    assert_eq!(event.unwrap().message(), "ATTENTION: Drive Damaged");

    // 0x1000 (0x888..0x1c71): F948 reduces the drive to class 1.
    let mut flight = self::flight();
    let (_, event) = flight.take_damage_roll(10, 25, 0x1000, 0);
    assert_eq!(event, Some(SystemEvent::DriveDamaged));
    assert_eq!(flight.combat.drive, 1);
    assert_eq!(flight.loadout.drive, 1);

    // 0x4000 (0x1c71..0x5555): F949 zeroes the drawn thruster word.
    let mut flight = self::flight();
    let (_, event) = flight.take_damage_roll(10, 25, 0x4000, 4);
    assert_eq!(event, Some(SystemEvent::ThrusterDamaged(4)));
    assert_eq!(flight.damage.thrusters.unwrap()[4], 0);
    assert_eq!(
        event.unwrap().message(),
        format!("ATTENTION: {} Thruster Damaged", THRUSTER_NAMES[4])
    );

    // 0x6000 (>= 0x5555): F945/F933 damages a fitted general item.
    let mut flight = self::flight();
    flight.loadout = Loadout::default();
    flight.loadout.general = 1 << 20;
    let (_, event) = flight.take_damage_roll(10, 25, 0x6000, 0);
    assert_eq!(event, Some(SystemEvent::EquipmentDamaged(20)));
    assert_eq!(flight.loadout.general & (1 << 20), 0);
    assert_ne!(flight.damage.equipment & (1 << 20), 0);
}

#[test]
fn f953_calls_the_subsystem_roll_only_on_a_hull_hit() {
    let mut flight = flight();
    flight.shields = [40, 40];
    // The shield absorbs it: no subsystem damage.
    let (damage, event) = flight.take_damage_roll(30, 25, 0x100, 0);
    assert_eq!(damage, Damage::Shields);
    assert_eq!(event, None);
    assert_eq!(flight.combat.drive, 2);

    // Hull damage with a big fraction of the remaining hull.
    flight.shields = [0, 0];
    let (damage, event) = flight.take_damage_roll(90, 25, 0x4000, 4);
    assert_eq!(damage, Damage::Hull);
    assert_eq!(event, Some(SystemEvent::ThrusterDamaged(4)));
    assert_eq!(flight.hull, [10, 100]);
}

#[test]
fn f444_rows_show_hull_thruster_and_item_repairs_and_charge_the_table() {
    let mut flight = flight();
    flight.credits = 100_000;
    flight.hull = [90, 100];
    flight.damage.equipment = 1 << 20;
    flight.damage.thrusters.as_mut().unwrap()[4] = 0;
    let rows = station::repairs_for(
        25,
        true,
        flight.hull,
        flight.damage.equipment,
        flight.damage.damaged_thrusters(),
        "1-Jan-3250",
        "1-Jan-3250",
    );
    // Three services, the two hull rows, one thruster and one item.
    assert_eq!(rows.len(), 7);
    let find = |action: RepairAction| rows.iter().find(|row| row.action == action).unwrap();
    assert_eq!(find(RepairAction::HullQuarter).cost, 10);
    assert_eq!(
        find(RepairAction::HullQuarter).title,
        "Repair 250kg of hull (2.50t total damage)"
    );
    assert_eq!(find(RepairAction::HullAll).cost, 100);
    assert_eq!(find(RepairAction::Thruster(4)).cost, 100);
    assert_eq!(
        find(RepairAction::Thruster(4)).title,
        "Repair Main thruster"
    );
    // DATA_005303's Fuel Scoop row: 2.5 x its 350 credit price.
    assert_eq!(find(RepairAction::Item(20)).cost, 875);
    assert_eq!(find(RepairAction::Item(20)).title, "Repair Fuel Scoop");

    // F448: the entire hull costs 10 x 100 tenths = 1000 credits (10000 cash).
    let entire = find(RepairAction::HullAll).clone();
    let before = flight.credits;
    station::apply_repair(&mut flight, Some(&stats()), &entire, 0).unwrap();
    assert_eq!(flight.hull[0], 100);
    assert_eq!(flight.credits, before - 1000);

    // F446: a repaired item is fitted again.
    let item = find(RepairAction::Item(20)).clone();
    station::apply_repair(&mut flight, Some(&stats()), &item, 0).unwrap();
    assert_eq!(flight.damage.equipment & (1 << 20), 0);
    assert_ne!(flight.loadout.general & (1 << 20), 0);

    // F449: the thruster word comes back from the statistics.
    let thruster = find(RepairAction::Thruster(4)).clone();
    station::apply_repair(&mut flight, Some(&stats()), &thruster, 0).unwrap();
    assert_eq!(flight.damage.thrusters.unwrap()[4], 0x100);
}

#[test]
fn a_repair_charges_ten_times_the_credit_cost_and_refuses_without_cash() {
    let mut flight = self::flight();
    flight.hull = [99, 100];
    let rows = station::repairs_for(
        25,
        true,
        flight.hull,
        0,
        [false; 6],
        "1-Jan-3250",
        "1-Jan-3250",
    );
    let quarter = rows
        .iter()
        .find(|row| row.action == RepairAction::HullQuarter)
        .unwrap()
        .clone();
    // The row is $10, so the cash unit (tenths) loses 100.
    flight.credits = 99;
    let error = station::apply_repair(&mut flight, Some(&stats()), &quarter, 0).unwrap_err();
    assert!(error.starts_with("You cannot afford"));
    assert_eq!(flight.hull, [99, 100]);
    assert_eq!(flight.credits, 99);

    flight.credits = 100;
    station::apply_repair(&mut flight, Some(&stats()), &quarter, 0).unwrap();
    assert_eq!(flight.hull, [100, 100]);
    assert_eq!(flight.credits, 0);
}

#[test]
fn f450_to_f452_reset_the_service_counters_and_dates() {
    let mut flight = self::flight();
    flight.credits = 100_000;
    flight.damage.service_counters = [3, 4];
    flight.service_days = [7, 8];
    let rows = station::repairs(25, true, "1-Jan-3250", "1-Jan-3250");
    let deluxe = rows
        .iter()
        .find(|row| row.action == RepairAction::Deluxe)
        .unwrap()
        .clone();
    station::apply_repair(&mut flight, Some(&stats()), &deluxe, 42).unwrap();
    assert_eq!(flight.service_days, [42, 42]);
    assert_eq!(flight.damage.service_counters, [500, 500]);
    assert_eq!(flight.credits, 100_000 - 25 * 80 * 10);

    flight.damage.service_counters = [1, 2];
    flight.service_days = [0, 0];
    station::apply_repair(
        &mut flight,
        Some(&stats()),
        rows.iter()
            .find(|row| row.action == RepairAction::DriveService)
            .unwrap(),
        50,
    )
    .unwrap();
    assert_eq!(flight.service_days, [50, 0]);
    assert_eq!(flight.damage.service_counters[0], 500);
    assert_eq!(flight.damage.service_counters[1], 2);
}

#[test]
fn f938_recharges_shields_and_the_hull_auto_repair_adds_tics() {
    // 0x80 max, no booster: 0x80 * 8 / 32 / 2 = 16.
    let mut flight = self::flight();
    flight.shields = [0, 0x80];
    flight.recharge(8, 100);
    assert_eq!(flight.shields[0], 16);
    // The Energy Booster doubles it.
    flight.shields = [0, 0x80];
    flight.loadout.general |= ffe_core::equipment::ENERGY_BOOSTER_BIT;
    flight.recharge(8, 100);
    assert_eq!(flight.shields[0], 32);
    // The Hull Auto-Repair System adds one point per tic, capped at the full
    // hull.
    flight.hull = [90, 100];
    flight.loadout.general |= ffe_core::equipment::HULL_AUTO_REPAIR_BIT;
    flight.recharge(5, 100);
    assert_eq!(flight.hull[0], 95);
    flight.recharge(500, 100);
    assert_eq!(flight.hull[0], 100);
}

#[test]
fn f929_burns_one_tonne_for_0x20000000_energy_and_warns_when_empty() {
    let mut flight = self::flight();
    flight.loadout.general |= ffe_core::equipment::AUTO_REFUELLER_BIT;
    flight.hold = vec![0; ffe_core::market::NAMES];
    flight.hold[ffe_core::hyperspace::HYDROGEN] = 2;
    flight.combat.energy = 0;
    assert!(flight.refuel_energy());
    assert_eq!(flight.combat.energy, 0x2000_0000);
    assert_eq!(flight.hold[ffe_core::hyperspace::HYDROGEN], 1);
    assert!(!flight.damage.fuel_empty);

    // With fuel gone the pool stays empty and F929's failure latches.
    flight.combat.energy = 0;
    flight.hold[ffe_core::hyperspace::HYDROGEN] = 0;
    assert!(!flight.refuel_energy());
    assert!(flight.damage.fuel_empty);
    assert!(flight.damage.engine_failed);
    assert_eq!(flight.limits(Some(&stats())).axes, [[0, 0], [0, 0], [0, 0]]);
}

#[test]
fn f938_charges_thrust_energy_and_refuels_a_charge_that_empties_the_pool() {
    let mut flight = self::flight();
    flight.loadout.general |= ffe_core::equipment::AUTO_REFUELLER_BIT;
    flight.hold = vec![0; ffe_core::market::NAMES];
    flight.hold[ffe_core::hyperspace::HYDROGEN] = 1;
    // mass 25 * sum 2000 / 512 = 97, * 1000 seconds = 97000.
    flight.combat.energy = 100_000;
    assert!(!flight.spend_thrust_energy(1000, 25, 2000));
    assert_eq!(flight.combat.energy, 100_000 - 97_000);

    // The next frame cannot pay: F929 burns the tonne and refills.
    assert!(!flight.spend_thrust_energy(1000, 25, 2000));
    assert_eq!(flight.combat.energy, 0x2000_0000);
    assert_eq!(flight.hold[ffe_core::hyperspace::HYDROGEN], 0);
}

#[test]
fn the_fuel_scoop_adds_one_tonne_up_to_the_cargo_capacity() {
    let mut flight = self::flight();
    flight.hold = vec![0; ffe_core::market::NAMES];
    assert_eq!(flight.scoop_hydrogen(2), Some(1));
    assert_eq!(flight.scoop_hydrogen(2), Some(2));
    assert_eq!(flight.scoop_hydrogen(2), None);
    assert_eq!(flight.hold[ffe_core::hyperspace::HYDROGEN], 2);
    // A fitted scoop only harvests near a star at speed.
    assert!(ffe_rs::damage::scoop_ready(0, 0x2328, true));
    assert!(!ffe_rs::damage::scoop_ready(
        ffe_rs::damage::SCOOP_DISTANCE + 1,
        0x3000,
        true
    ));
}

#[test]
fn the_daily_servicing_roll_can_break_equipment_or_the_drive() {
    let mut flight = self::flight();
    flight.loadout = Loadout::default();
    flight.loadout.general = 1 << 20;
    flight.last_service_day = Some(0);
    flight.damage.service_counters = [0, 0];
    let events = flight.service_day(100);
    assert!(!events.is_empty());
    assert_eq!(flight.combat.drive, 0);
    assert_eq!(flight.loadout.drive, 0);
    assert!(events
        .iter()
        .any(|event| *event == SystemEvent::DriveFailure
            || *event == SystemEvent::EquipmentFailure(20)));
}

/// The gameplay slice's scripted sequence: a hit destroys the Main thruster,
/// the ship docks, the repair row is paid for, and the flight model's z limit
/// comes back.
#[test]
fn damage_dock_repair_then_fly_restores_the_thruster_limit() {
    use ffe_rs::flight::Mode;
    use ffe_rs::original_flight::OriginalFlight;

    let stats = stats();
    let mut flight = self::flight();
    flight.mode = Mode::Flight;
    flight.credits = 10_000;
    let (_, event) = flight.take_damage_roll(10, 25, 0x4000, 4);
    assert_eq!(event, Some(SystemEvent::ThrusterDamaged(4)));
    assert_eq!(flight.limits(Some(&stats)).axes[2], [0, -0x80]);
    let model = OriginalFlight::from_flight(&flight, Some(&stats));
    assert_eq!(model.player().limits.axes[2][0], 0, "Main is dead");

    // Dock at the starport; F444 lists the destroyed thruster.
    flight.mode = Mode::Docked;
    let rows = station::repairs_for(
        25,
        true,
        flight.hull,
        flight.damage.equipment,
        flight.damage.damaged_thrusters(),
        "1-Jan-3250",
        "1-Jan-3250",
    );
    let row = rows
        .iter()
        .find(|row| row.action == RepairAction::Thruster(4))
        .unwrap()
        .clone();
    station::apply_repair(&mut flight, Some(&stats), &row, 0).unwrap();
    assert_eq!(flight.credits, 10_000 - 100 * 10);

    // Fly again: the restored word reaches F598/F600.
    flight.mode = Mode::Flight;
    assert_eq!(flight.limits(Some(&stats)).axes[2], [0x100, -0x80]);
    let model = OriginalFlight::from_flight(&flight, Some(&stats));
    assert_eq!(model.player().limits.axes[2][0], 0x100);
}

#[test]
fn first_frame_recharge_does_not_destroy_a_new_ship() {
    let mut flight = Flight::default();
    // main runs F938 before its HUD-side ensure_damage call.
    flight.recharge(994, 100);
    flight.ensure_damage(100);
    assert_eq!(flight.hull, [100, 100]);
    assert!(!flight.destroyed());
}

#[test]
fn recharge_does_not_revive_a_destroyed_ship_or_erase_damage() {
    let mut flight = flight();
    flight.hull = [40, 100];
    flight.recharge(994, 100);
    assert_eq!(flight.hull, [40, 100]);
    flight.hull = [0, 100];
    flight.recharge(994, 100);
    assert!(flight.destroyed());
}

#[test]
fn f938_runtime_events_do_not_spend_physics_tics_as_seconds() {
    // F152/F1007 deliver whole seconds; F938's Saker upward load is
    // (28 * 3270) >> 9 = 178 per second. Physics still uses ~49700 tics/s.
    for (fps, acceleration) in [(50, 1.0), (64, 1.0), (50, 10.0), (64, 10000.0)] {
        let mut flight = self::flight();
        flight.combat = ffe_rs::combat::Combat::new_game();
        let start = flight.combat.energy;
        let mut seconds = 0;
        for _ in 0..fps * 60 {
            let elapsed = 1.0 / f64::from(fps);
            let event = flight.combat.tick(
                ffe_rs::combat::frame_tics(elapsed, acceleration),
                elapsed * acceleration,
            );
            seconds += event;
            if event != 0 {
                assert!(!flight.spend_thrust_energy(event, 28, 3270));
            }
        }
        assert!((f64::from(seconds) - 60.0 * acceleration).abs() <= 1.0);
        assert_eq!(flight.combat.energy, start - 178 * seconds);
        assert!(!flight.damage.fuel_empty);
        assert!(!flight.damage.engine_failed);
    }
}

#[test]
fn f938_preserves_imul_low_word_and_signed_shift() {
    let mut flight = self::flight();
    flight.combat.energy = u32::MAX;
    // 32767 * (3 * abs(-32768)) wraps to -1073840128; SAR 9
    // gives -2097344, then IMUL 2 gives cost 0xffbffe80.
    assert!(!flight.spend_thrust_energy(2, 32767, 98304));
    assert_eq!(flight.combat.energy, u32::MAX - 0xffbffe80);
}

#[test]
fn f929_military_internal_refuelling_preserves_cargo_mass_as_waste() {
    let mut flight = self::flight();
    flight.loadout.general |= ffe_core::equipment::AUTO_REFUELLER_BIT;
    flight.combat.drive = 10;
    flight.hold = vec![0; ffe_core::market::NAMES];
    flight.hold[ffe_core::hyperspace::MILITARY_FUEL] = 2;
    flight.combat.energy = 0;
    assert!(flight.refuel_energy());
    assert_eq!(flight.hold[ffe_core::hyperspace::MILITARY_FUEL], 1);
    assert_eq!(flight.hold[ffe_core::hyperspace::RADIOACTIVES], 1);
    assert_eq!(flight.hold.iter().sum::<u16>(), 2);
    flight.combat.energy = 0;
    assert!(flight.refuel_energy());
    assert_eq!(flight.hold[ffe_core::hyperspace::RADIOACTIVES], 2);
    flight.combat.energy = 0;
    assert!(!flight.refuel_energy());
    assert_eq!(flight.hold[ffe_core::hyperspace::RADIOACTIVES], 2);
}
