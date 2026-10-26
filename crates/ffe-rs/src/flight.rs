use serde::{Deserialize, Serialize};

use ffe_core::npc::Random;

use crate::damage::{Damage, SystemEvent};

pub const STEP: f64 = 0.02;
pub const BUY_PRICE: u32 = 10;
pub const SELL_PRICE: u32 = 9;
pub const CARGO_CAPACITY: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Docked,
    Flight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flight {
    pub mode: Mode,
    /// DATA_008918, in original signed velocity units. Older saves infer it
    /// from motion once; new saves preserve a selected speed independently.
    #[serde(default)]
    pub set_speed: Option<i32>,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub credits: u32,
    pub cargo: u32,
    pub stock: u32,
    pub ticks: u64,
    pub docking_requested: bool,
    /// The player's craft: an original model table slot (see `ships`). Saves from
    /// before ship selection load with the Eagle long range fighter.
    #[serde(default = "default_ship")]
    pub ship: u32,
    /// The forward thrust last commanded (-1..1), for drawing engine glows. Saves
    /// from before it load with 0.
    #[serde(default)]
    pub thrust: f64,
    /// The ship's full orientation (right, up, forward) when the original flight
    /// model is running (it rolls, which `yaw` and `pitch` cannot say). Saves from
    /// before it load `None` and fall back to `yaw` and `pitch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<[[f64; 3]; 3]>,
    /// Flying in a system (the navigator's bodies, not the prototype's station at the
    /// origin): the prototype's boundary, docking and safety reset do not apply.
    /// Saves from before it load `false`.
    #[serde(default)]
    pub system_flight: bool,
    /// Tonnes of each commodity in the hold (the original's 38-word cargo array,
    /// `ffe_core::market`); empty is an empty hold. Saves from before the market
    /// load empty.
    #[serde(default)]
    pub hold: Vec<u16>,
    /// The laser installation and its heat, energy and refire words. Saves from
    /// before combat load no lasers.
    #[serde(default)]
    pub combat: crate::combat::Combat,
    /// The player's shield words (`ship+0xe0` current, `+0xe2` maximum). The
    /// maximum is `0x40` per fitted shield generator (`F413`); `[0, 0]` means
    /// no shield state, as on a new-game ship without a generator. `F1099` does
    /// not draw the line when the two words are equal (a pristine shield).
    #[serde(default)]
    pub shields: [u16; 2],
    /// The hull word (`ship+0xe4`, mass * 4) and its full value. `[0, 0]` means
    /// untracked until [`Flight::ensure_damage`] fills it from the ship
    /// statistics (the descriptor mass word times four, as F1533 does).
    #[serde(default)]
    pub hull: [u16; 2],
    /// The fitted equipment (`ship+0xc8`, `+0xd6..+0xdf`, `+0xe2`). Saves from
    /// before equipment load with nothing fitted; the new game sets F998's
    /// loadout in `main`.
    #[serde(default)]
    pub loadout: ffe_core::equipment::Loadout,
    /// The drive and equipment service dates (`DATA_008926`/`DATA_008925`) as
    /// game days, shown by F444's "last:" rows. Saves from before the repairs
    /// screen load with day 0, the new game's 1-Jan-3250.
    #[serde(default)]
    pub service_days: [u32; 2],
    /// The thruster words, damaged equipment bits and service counters
    /// (`ship+0xbc..+0xc6`, `+0xcc`, `DATA_008923`/`DATA_008924`). Saves from
    /// before the damage model load undamaged with fresh counters.
    #[serde(default)]
    pub damage: Damage,
    /// The last game day the servicing roll (F934) ran for; `None` starts at
    /// the first frame so a loaded save does not replay every past day.
    #[serde(skip)]
    pub last_service_day: Option<u32>,
    /// F1033's scoop cooldown, in accelerated seconds (`0x2468a` tics).
    #[serde(skip)]
    pub scoop_cooldown: f64,
    /// The original's shared random stream (`DATA_008818`/`DATA_008820`) for
    /// the damage and servicing rolls.
    #[serde(skip, default = "default_rng")]
    pub rng: Random,
    /// F1104's death sequence, when the hull has been used up and no escape
    /// craft launched. Saves from before the sequence load alive.
    #[serde(default)]
    pub death: Option<crate::death::Death>,
    /// The accepted BBS contracts, the journal and the comms pages (F290/
    /// F303/F306/F307). Saves from before the mission slice load empty.
    #[serde(default)]
    pub missions: ffe_core::missions::State,
}

fn default_rng() -> Random {
    Random::new(0x2545_f491, 0x9e37_79b9)
}

fn default_ship() -> u32 {
    crate::ships::DEFAULT_SHIP
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Controls {
    pub thrust: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub brake: bool,
}

impl Default for Flight {
    fn default() -> Self {
        Self {
            mode: Mode::Docked,
            set_speed: None,
            position: [0.0, 0.0, -60.0],
            velocity: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            credits: 1000,
            cargo: 0,
            stock: 50,
            ticks: 0,
            docking_requested: false,
            ship: default_ship(),
            thrust: 0.0,
            axes: None,
            system_flight: false,
            hold: Vec::new(),
            combat: crate::combat::Combat::default(),
            shields: [0; 2],
            hull: [0; 2],
            loadout: ffe_core::equipment::Loadout::default(),
            service_days: [0; 2],
            damage: Damage::default(),
            last_service_day: None,
            scoop_cooldown: 0.0,
            rng: default_rng(),
            death: None,
            missions: ffe_core::missions::State::default(),
        }
    }
}

impl Flight {
    pub fn approach() -> Self {
        Self {
            mode: Mode::Flight,
            position: [0.0, 0.0, -180.0],
            velocity: [0.0, 0.0, 5.0],
            ..Self::default()
        }
    }

    pub fn free_flight() -> Self {
        Self {
            mode: Mode::Flight,
            position: [0.0, 0.0, -1500.0],
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        // The prototype boundary keeps the ship near the origin and caps the
        // speed at the prototype's own limits. System flight carries the
        // original's 64-bit system coordinates (scaled by
        // `UNITS_PER_FLIGHT_UNIT`) and its signed 32-bit velocity words, which
        // are far larger; only finiteness and a generous corruption bound apply
        // there. The original flight model (key O) itself reaches a few
        // thousand flight units per second in the prototype.
        let (position_limit, velocity_limit) = if self.system_flight {
            (1.0e12, 1.0e8)
        } else {
            (1_000_000.0, 5000.0)
        };
        if !self
            .position
            .iter()
            .chain(&self.velocity)
            .chain([&self.yaw, &self.pitch])
            .all(|number| number.is_finite())
            || self
                .position
                .iter()
                .any(|number| number.abs() > position_limit)
            || self
                .velocity
                .iter()
                .any(|number| number.abs() > velocity_limit)
            || self.speed() > velocity_limit * 1.0000001
            || !(0.0..std::f64::consts::TAU).contains(&self.yaw)
            || self.pitch.abs() > 1.4
            || self.cargo > CARGO_CAPACITY
            || self.stock > 1000
            || self.credits > 1_000_000_000
            || !(crate::ships::FIRST_SHIP..=crate::ships::LAST_SHIP).contains(&self.ship)
            || !(self.thrust.is_finite() && self.thrust.abs() <= 1.0)
            || self.axes.is_some_and(|axes| {
                axes.iter().any(|row| {
                    row.iter()
                        .any(|value| !value.is_finite() || value.abs() > 1.5)
                        || !(0.9..=1.1).contains(&length(*row))
                })
            })
            || !(self.hold.is_empty() || self.hold.len() == ffe_core::market::NAMES)
            || self.hold.iter().map(|t| u32::from(*t)).sum::<u32>() > 100_000
            || self.shields[0] > self.shields[1]
            || self.hull[0] > self.hull[1]
            || !self.loadout.validate()
            || self.combat.drive > 0x0e
            || !(self.combat.cool_carry.is_finite() && (0.0..1.0).contains(&self.combat.cool_carry))
            || self
                .combat
                .lasers
                .iter()
                .any(|code| *code != 0 && ffe_core::laser::index(*code).is_none_or(|i| i > 10))
            || self
                .combat
                .missiles
                .iter()
                .any(|code| *code != 0 && ffe_core::missile::model(*code).is_none())
            || !self.missions.validate()
        {
            return Err("save contains out-of-range prototype state".to_owned());
        }
        // F1104's sequence ends the game; there is no save after death.
        if self.death.is_some() {
            return Err("save was made after the ship's destruction".to_owned());
        }
        if self.mode == Mode::Docked
            && (self.velocity != [0.0; 3]
                || self.position != [0.0, 0.0, -60.0]
                || self.docking_requested)
        {
            return Err("save has inconsistent docked state".to_owned());
        }
        Ok(())
    }

    pub fn forward(&self) -> [f64; 3] {
        if let Some(axes) = self.axes {
            return axes[2];
        }
        [
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ]
    }

    pub fn distance(&self) -> f64 {
        length(self.position)
    }
    pub fn speed(&self) -> f64 {
        length(self.velocity)
    }

    pub fn launch(&mut self) -> Result<&'static str, String> {
        if self.mode != Mode::Docked {
            return Err("Already in flight.".to_owned());
        }
        self.mode = Mode::Flight;
        self.position = [0.0, 0.0, -500.0];
        self.velocity = [0.0; 3];
        self.yaw = 0.0;
        self.pitch = 0.0;
        Ok("Launched. Station is ahead; D requests docking.")
    }

    pub fn request_docking(&mut self) -> Result<&'static str, String> {
        if self.mode != Mode::Flight {
            return Err("Already docked.".to_owned());
        }
        self.docking_requested = !self.docking_requested;
        Ok(if self.docking_requested {
            "Docking requested: range <80, speed <20, face station."
        } else {
            "Docking request cancelled."
        })
    }

    pub fn buy(&mut self) -> Result<&'static str, String> {
        self.require_docked()?;
        if self.cargo == CARGO_CAPACITY {
            return Err("Cargo hold full.".to_owned());
        }
        if self.stock == 0 {
            return Err("Station out of stock.".to_owned());
        }
        if self.credits < BUY_PRICE {
            return Err("Insufficient credits.".to_owned());
        }
        self.credits -= BUY_PRICE;
        self.cargo += 1;
        self.stock -= 1;
        Ok("Bought one unit of food.")
    }

    pub fn sell(&mut self) -> Result<&'static str, String> {
        self.require_docked()?;
        if self.cargo == 0 {
            return Err("No cargo to sell.".to_owned());
        }
        if self.stock >= 1000 {
            return Err("Station storage full.".to_owned());
        }
        let credits = self
            .credits
            .checked_add(SELL_PRICE)
            .filter(|value| *value <= 1_000_000_000)
            .ok_or("Credit limit reached.")?;
        self.credits = credits;
        self.cargo -= 1;
        self.stock += 1;
        Ok("Sold one unit of food.")
    }

    fn require_docked(&self) -> Result<(), String> {
        if self.mode != Mode::Docked {
            Err("Trading requires docking.".to_owned())
        } else {
            Ok(())
        }
    }

    pub fn step(&mut self, input: Controls) -> Result<Option<&'static str>, String> {
        if ![input.thrust, input.yaw, input.pitch]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1.0)
        {
            return Err("Invalid flight control input.".to_owned());
        }
        if self.mode == Mode::Docked {
            self.thrust = 0.0;
            return Ok(None);
        }
        self.thrust = input.thrust;
        self.yaw = (self.yaw + input.yaw * STEP).rem_euclid(std::f64::consts::TAU);
        self.pitch = (self.pitch + input.pitch * STEP).clamp(-1.4, 1.4);
        let forward = self.forward();
        for (axis, direction) in forward.into_iter().enumerate() {
            self.velocity[axis] += direction * input.thrust * 30.0 * STEP;
            if input.brake {
                self.velocity[axis] *= 0.94;
            }
        }
        let speed = self.speed();
        if speed > 250.0 {
            for velocity in &mut self.velocity {
                *velocity *= 250.0 / speed;
            }
        }
        for axis in 0..3 {
            self.position[axis] += self.velocity[axis] * STEP;
        }
        self.ticks = self.ticks.wrapping_add(1);
        Ok(self.finish_step())
    }

    /// What follows a step of either flight model: the prototype boundary,
    /// docking and the station safety reset. Returns the message when one fired.
    pub fn finish_step(&mut self) -> Option<&'static str> {
        if self.system_flight {
            return None;
        }
        let forward = self.forward();
        if self
            .position
            .iter()
            .any(|number| number.abs() > 1_000_000.0)
        {
            self.mode = Mode::Docked;
            self.position = [0.0, 0.0, -60.0];
            self.velocity = [0.0; 3];
            self.docking_requested = false;
            return Some("Prototype boundary reached: returned to station.");
        }
        let distance = self.distance();
        let facing = if distance > 0.0 {
            -forward
                .iter()
                .zip(self.position)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                / distance
        } else {
            1.0
        };
        if self.docking_requested && distance < 80.0 && self.speed() < 20.0 && facing > 0.95 {
            self.mode = Mode::Docked;
            self.position = [0.0, 0.0, -60.0];
            self.velocity = [0.0; 3];
            self.docking_requested = false;
            return Some("Docked. B buys food, N sells; F5 saves.");
        }
        if distance < 45.0 {
            self.position = [0.0, 0.0, -180.0];
            self.velocity = [0.0; 3];
            self.yaw = 0.0;
            self.pitch = 0.0;
            return Some("Unsafe approach: station safety reset. Slow down and request docking.");
        }
        None
    }

    /// Give the damage words the values object set-up gives a ship: F1533
    /// (`ffe.asm:90970`) stores the descriptor mass word times four in
    /// `+0xe4`, and the shield maximum (`+0xe2`) is `0x40` per fitted shield
    /// generator (F413 adds, F415 removes). A save from before the damage
    /// model loads with `[0, 0]` and takes the full hull here; an existing
    /// damaged ship keeps its words while its full value matches, and a change
    /// of hull (or of generators) resets them. A new hull also carries a fresh
    /// set of thruster limits and no damaged equipment.
    pub fn ensure_damage(&mut self, hull_full: u16) {
        if hull_full != 0 && self.hull[1] != hull_full {
            self.hull = [hull_full, hull_full];
            self.damage = Damage::default();
        }
        let shield_max = self.loadout.shield_max();
        if shield_max != self.shields[1] {
            self.shields = [shield_max, shield_max];
        }
    }

    /// Fill the six thruster limits from the ship's statistics when the save
    /// predates them (object set-up, the routine at 0x125c0).
    pub fn ensure_thrusters(&mut self, stats: &crate::ships::ShipStats) {
        self.damage.ensure_thrusters(stats);
    }

    /// The six limit words the flight model clamps against: F929's engine
    /// failure zeroes them all, otherwise the saved words (object set-up's
    /// values, less any destroyed thrusters).
    pub fn limits(&self, stats: Option<&crate::ships::ShipStats>) -> ffe_core::thrust::Limits {
        if self.damage.engine_failed {
            return ffe_core::thrust::Limits::default();
        }
        self.damage
            .limits()
            .or_else(|| stats.map(crate::ships::ShipStats::limits))
            .unwrap_or(crate::original_flight::FALLBACK_LIMITS)
    }

    /// The sum of the three thrust words, F938's engine load (`|x|+|y|+|z|`).
    pub fn thrust_sum(&self, thrust_words: [i16; 3]) -> i32 {
        thrust_words
            .iter()
            .fold(0i32, |sum, word| sum.wrapping_add(i32::from(*word).abs()))
    }

    /// F938 (`ffe.asm:70122`) for a whole-second event: shield recharge
    /// (`max * seconds / 32`, halved without the Energy Booster) and the Hull
    /// Auto-Repair System (`+0xc8` bit 31) adding `seconds` hull points up to the
    /// full hull.
    pub fn recharge(&mut self, seconds: u32, hull_full: u16) {
        if self.shields[0] != self.shields[1] {
            // F938: IMUL keeps the low dword, SAR is signed, the optional
            // halving is logical, and the clamp compares the wrapped sum.
            let mut added = (i32::from(self.shields[1]).wrapping_mul(seconds as i32) >> 5) as u32;
            if !self.loadout.energy_booster() {
                added >>= 1;
            }
            self.shields[0] =
                if u32::from(self.shields[0]).wrapping_add(added) > u32::from(self.shields[1]) {
                    self.shields[1]
                } else {
                    self.shields[0].wrapping_add(added as u16)
                };
        }
        if hull_full != 0 && self.hull[1] == 0 {
            // An uninitialised new ship is intact. F938 runs before the
            // HUD's ensure_damage on the first frame; setting only the maximum
            // would make that later call preserve a zero hull and trigger death.
            self.hull = [hull_full, hull_full];
        }
        if self.loadout.hull_auto_repair() && self.hull[0] != hull_full {
            let total = u32::from(self.hull[0]).wrapping_add(seconds) as i32;
            self.hull[0] = if i32::from(hull_full) < total {
                hull_full
            } else {
                self.hull[0].wrapping_add(seconds as u16)
            };
        }
    }

    /// F953's hull-damage tail (`ffe.asm:71289-71311`): a hit at least a
    /// quarter of the remaining hull, or one whose proportional random draw
    /// fires, calls F952's subsystem roll. The original's outer F952 test also
    /// compares `F922(cargo space)` with `DATA_008889`; that word is the
    /// player's combat score, which the port does not carry.
    pub fn take_damage_with_systems(
        &mut self,
        damage: u32,
        mass: u16,
    ) -> (ffe_core::laser::Damage, Option<SystemEvent>) {
        let result = self.take_damage(damage, mass);
        if result != ffe_core::laser::Damage::Hull {
            return (result, None);
        }
        let quarter = (u32::from(self.hull[0]) / 4).max(1);
        let proportional = damage
            .saturating_mul(0x8000)
            .checked_div(quarter)
            .unwrap_or(u32::MAX)
            .min(0xffff);
        if damage < quarter && self.rng.bounded(0x1_0000) >= proportional {
            return (result, None);
        }
        let roll = self.rng.bounded(0x1_0000) as u16;
        let thruster = self.rng.bounded(6) as u8;
        (result, self.apply_subsystem_roll(roll, thruster))
    }

    /// [`Flight::take_damage_with_systems`] with explicit F950 draws, for tests
    /// and for callers that keep their own random stream. The hull hit is
    /// applied; the subsystem roll always runs.
    pub fn take_damage_roll(
        &mut self,
        damage: u32,
        mass: u16,
        roll: u16,
        thruster: u8,
    ) -> (ffe_core::laser::Damage, Option<SystemEvent>) {
        let result = self.take_damage(damage, mass);
        if result != ffe_core::laser::Damage::Hull {
            return (result, None);
        }
        (result, self.apply_subsystem_roll(roll, thruster))
    }

    /// F950 for one draw (`F1530` 0..0xffff) and thruster index (`F922(6)`).
    pub fn apply_subsystem_roll(&mut self, roll: u16, thruster: u8) -> Option<SystemEvent> {
        let event = if roll >= 0x5555 {
            self.damage
                .break_equipment(&mut self.loadout.general, &mut self.rng)
                .map(SystemEvent::EquipmentDamaged)
        } else if roll < 0x888 {
            self.combat.drive = 0;
            self.loadout.drive = 0;
            self.combat.energy = 0;
            Some(SystemEvent::DriveDamaged)
        } else if roll < 0x1c71 {
            if self.combat.drive != 0 {
                self.combat.drive = 1;
                self.loadout.drive = 1;
            }
            Some(SystemEvent::DriveDamaged)
        } else if self.damage.break_thruster(usize::from(thruster)) {
            Some(SystemEvent::ThrusterDamaged(thruster))
        } else {
            None
        };
        if event.is_some() {
            self.damage.fuel_empty = false;
        }
        event
    }

    /// F950's roll: two thirds break a fitted general item (F945/F933), then
    /// the drive (F947/F948) and finally a thruster (F949) as the draw falls.
    pub fn subsystem_roll(&mut self) -> Option<SystemEvent> {
        let roll = self.rng.bounded(0x1_0000) as u16;
        let thruster = self.rng.bounded(6) as u8;
        self.apply_subsystem_roll(roll, thruster)
    }

    /// F929's fuel slot: hydrogen for drives below class 10, military fuel
    /// (`ffe_core::hyperspace::MILITARY_FUEL`) from class 10 up.
    pub fn fuel_slot(&self) -> usize {
        if self.combat.drive >= ffe_core::hyperspace::MILITARY_CLASS as u8 {
            ffe_core::hyperspace::MILITARY_FUEL
        } else {
            ffe_core::hyperspace::HYDROGEN
        }
    }

    /// Burn one tonne of fuel from the hold. Returns false when the slot is
    /// empty (F929's `+0x15a`/`+0x15c` test).
    pub fn burn_fuel(&mut self) -> bool {
        let slot = self.fuel_slot();
        let Some(tonnes) = self.hold.get_mut(slot).filter(|tonnes| **tonnes > 0) else {
            return false;
        };
        *tonnes -= 1;
        if slot == ffe_core::hyperspace::MILITARY_FUEL {
            // F929 exchanges one tonne for one tonne of waste; unlike hydrogen,
            // military refuelling does not free cargo space.
            self.hold.resize(
                self.hold.len().max(ffe_core::hyperspace::RADIOACTIVES + 1),
                0,
            );
            let waste = &mut self.hold[ffe_core::hyperspace::RADIOACTIVES];
            *waste = waste.wrapping_add(1);
        }
        true
    }

    /// F929's empty pool: with the Auto Refueller fitted and fuel in the hold,
    /// burn one tonne and add `0x20000000` energy. Otherwise latch the empty
    /// warning and F929's engine failure.
    pub fn refuel_energy(&mut self) -> bool {
        if !self.loadout.auto_refueller() || !self.burn_fuel() {
            self.damage.fuel_empty = true;
            self.damage.engine_failed = true;
            return false;
        }
        self.combat.energy = self
            .combat
            .energy
            .saturating_add(crate::damage::ENERGY_PER_TONNE);
        self.damage.fuel_empty = false;
        self.damage.engine_failed = false;
        true
    }

    /// After F930's shot (F929): an emptied pool is refilled from fuel.
    pub fn refuel_after_energy(&mut self) {
        if self.combat.energy == 0 {
            self.refuel_energy();
        }
    }

    /// F938's thrust energy (`mass * sum |thrust words| >> 9 * seconds`) through
    /// F929. Returns true when the engine failed (no energy and no fuel).
    pub fn spend_thrust_energy(&mut self, seconds: u32, mass: u16, thrust_sum: i32) -> bool {
        // F938 uses signed model mass and the low 32 bits of both IMULs.
        let cost = (i32::from(mass as i16).wrapping_mul(thrust_sum) >> 9)
            .wrapping_mul(seconds as i32) as u32;
        if cost == 0 && self.combat.energy > 0 {
            self.damage.engine_failed = false;
            return false;
        }
        if self.combat.energy > cost {
            self.combat.energy -= cost;
            self.damage.engine_failed = false;
            return false;
        }
        self.combat.energy = 0;
        if self.refuel_energy() {
            false
        } else {
            self.damage.engine_failed = true;
            true
        }
    }

    /// F933/F934's daily servicing: the counters fall by one a day; when a
    /// 0..255 draw beats the counter, one fitted item (F933) or the drive
    /// fails from lack of servicing.
    pub fn service_day(&mut self, day: u32) -> Vec<SystemEvent> {
        let last = self.last_service_day.unwrap_or(day);
        if day <= last {
            return Vec::new();
        }
        self.last_service_day = Some(day);
        let days = (day - last).min(4096);
        let mut events = Vec::new();
        for _ in 0..days {
            for index in 0..2 {
                let counter = &mut self.damage.service_counters[index];
                *counter = counter.saturating_sub(1);
                let counter = *counter;
                if self.rng.bounded(0x100) <= counter {
                    continue;
                }
                if index == 0 {
                    if let Some(bit) = self
                        .damage
                        .break_equipment(&mut self.loadout.general, &mut self.rng)
                    {
                        events.push(SystemEvent::EquipmentFailure(bit));
                    }
                } else if self.combat.drive != 0 {
                    self.combat.drive = 0;
                    self.loadout.drive = 0;
                    self.combat.energy = 0;
                    events.push(SystemEvent::DriveFailure);
                }
            }
        }
        events
    }

    /// F449/F450/F451/F452: reset a service counter to 500 days.
    pub fn reset_service(&mut self, index: usize, day: u32) {
        if let Some(counter) = self.damage.service_counters.get_mut(index) {
            *counter = crate::damage::SERVICE_DAYS;
        }
        if let Some(date) = self.service_days.get_mut(index) {
            *date = day;
        }
    }

    /// F446: repair one general-equipment bit (clear `+0xcc`, set `+0xc8`).
    pub fn repair_equipment(&mut self, bit: u8) -> bool {
        self.damage.repair_equipment(bit, &mut self.loadout.general)
    }

    /// F447/F448: repair `points` hull points, up to the full hull.
    pub fn repair_hull(&mut self, points: u16) {
        if self.hull[1] != 0 {
            self.hull[0] = self.hull[0].saturating_add(points).min(self.hull[1]);
        }
    }

    /// F1033: add one tonne of hydrogen to the hold, if there is space.
    /// Returns the new hydrogen count.
    pub fn scoop_hydrogen(&mut self, capacity: u32) -> Option<u16> {
        let used: u32 = self.hold.iter().map(|tonnes| u32::from(*tonnes)).sum();
        if used >= capacity {
            return None;
        }
        if self.hold.len() < ffe_core::market::NAMES {
            self.hold.resize(ffe_core::market::NAMES, 0);
        }
        let slot = &mut self.hold[ffe_core::hyperspace::HYDROGEN];
        *slot = slot.saturating_add(1);
        Some(*slot)
    }

    /// Whether the player's hull has been used up (F942 destroys the object).
    pub fn destroyed(&self) -> bool {
        self.hull[1] != 0 && self.hull[0] == 0
    }

    /// F953 (`ffe.asm:71190`) on the player: the shields absorb first (`+0xe0`
    /// against `+0xe2`), then the hull (`+0xe4`, mass * 4). `mass` is the ship
    /// statistics' mass word. The first hit records the full hull from it, so a
    /// save from before the damage model (hull `[0, 0]`) still shows F1099's
    /// hull line; a fitted shield generator fills the shield maximum when the
    /// frame has not run [`Flight::ensure_damage`] yet.
    pub fn take_damage(&mut self, damage: u32, mass: u16) -> ffe_core::laser::Damage {
        let full = mass.max(1).saturating_mul(4);
        if self.hull[1] == 0 {
            self.hull = [full, full];
        }
        if self.shields[1] == 0 {
            let shield_max = self.loadout.shield_max();
            if shield_max != 0 {
                self.shields = [shield_max, shield_max];
            }
        }
        self.apply_hit(damage)
    }

    /// The equipment slice's entry point: [`Flight::ensure_damage`] has already
    /// filled the words, so this applies one F953 hit to them. Kept for its
    /// callers and tests; it shares [`Flight::take_damage`]'s path.
    pub fn hit(&mut self, damage: u32) -> ffe_core::laser::Damage {
        self.apply_hit(damage)
    }

    fn apply_hit(&mut self, damage: u32) -> ffe_core::laser::Damage {
        let mut state = ffe_core::laser::DamageState {
            shields: self.shields[0].min(self.shields[1]),
            shields_max: self.shields[1],
            hull: self.hull[0].min(self.hull[1]),
            mass: self.hull[1] / 4,
        };
        let result = ffe_core::laser::apply_damage(&mut state, damage);
        self.shields[0] = state.shields;
        self.hull[0] = state.hull;
        result
    }
    /// F953's amount from F575's surface contact: the damage the bounce handed
    /// to F953 (`+0xc` in `DATA_008548`), applied to the shields and then the
    /// hull like a laser hit. `None` when the contact carried no damage (a soft
    /// landing, or a touch whose F953 amount was zero or negative: F953 raises a
    /// negative amount to `0x7fffffff`, but F575 only passes the bounce's
    /// non-negative product, and -1 comes from the missile branch of another
    /// body, not the player's).
    pub fn take_contact_damage(
        &mut self,
        contact: &ffe_core::collision::Contact,
        mass: u16,
    ) -> Option<ffe_core::laser::Damage> {
        match contact.damage {
            damage if damage > 0 => {
                Some(self.take_damage(damage.min(i64::from(u32::MAX)) as u32, mass))
            }
            _ => None,
        }
    }
}

fn length(vector: [f64; 3]) -> f64 {
    vector
        .iter()
        .map(|number| number * number)
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffe_core::collision::Contact;
    use ffe_core::laser::Damage;

    /// F575's bounce damage reaches F953 as `contact.damage`; the Saker's hull is
    /// the descriptor mass (28) times four.
    fn contact(damage: i64) -> Contact {
        Contact {
            flags: 0x83,
            distance: -1,
            object: 2,
            pad: 0,
            touch: Some(ffe_core::landing::Touch::Crash {
                damage: damage as i32,
            }),
            damage,
        }
    }

    fn saker() -> Flight {
        let mut state = Flight {
            mode: Mode::Flight,
            ..Flight::default()
        };
        state.ensure_damage(112);
        state
    }

    #[test]
    fn a_surface_crash_applies_f953_to_the_hull() {
        let mut state = saker();
        // The bounce's minimum for a 28 t hull is the mass word itself; a
        // sub-lethal hit takes it off the hull (the Saker has no generator).
        assert_eq!(
            state.take_contact_damage(&contact(28), 28),
            Some(Damage::Hull)
        );
        assert_eq!(state.hull, [84, 112]);
        assert!(!state.destroyed());
    }

    #[test]
    fn shields_absorb_the_crash_before_the_hull() {
        let mut state = saker();
        state.shields = [64, 64];
        assert_eq!(
            state.take_contact_damage(&contact(80), 28),
            Some(Damage::Hull)
        );
        assert_eq!(state.shields, [0, 64]);
        assert_eq!(state.hull, [96, 112]);
        // At or below the current shields nothing reaches the hull.
        let mut state = saker();
        state.shields = [64, 64];
        assert_eq!(
            state.take_contact_damage(&contact(64), 28),
            Some(Damage::Shields)
        );
        assert_eq!((state.shields, state.hull), ([0, 64], [112, 112]));
    }

    #[test]
    fn a_lethal_crash_uses_the_hull_up_and_destroys_the_ship() {
        // A hull-speed impact (about 2,200 velocity units) gives F953 more than
        // the Saker's 112 t of hull: F942 destroys the object and F1104 starts
        // the death hold.
        let mut state = saker();
        assert_eq!(
            state.take_contact_damage(&contact(112), 28),
            Some(Damage::Destroyed)
        );
        assert!(state.destroyed());
        assert_eq!(state.hull, [0, 112]);
        assert!(state.death.is_none(), "F1104 is the caller's step");
    }

    #[test]
    fn a_landing_or_an_empty_touch_deals_no_damage() {
        let mut state = saker();
        let mut landing = contact(0);
        landing.flags = 0x42;
        landing.touch = Some(ffe_core::landing::Touch::Landing);
        assert_eq!(state.take_contact_damage(&landing, 28), None);
        // The missile branch of another body passes F953 -1; the player's body
        // never takes that path, and F575's squeeze is not damage to this ship.
        assert_eq!(state.take_contact_damage(&contact(-1), 28), None);
        assert_eq!(state.hull, [112, 112]);
        assert!(!state.destroyed());
    }
}
