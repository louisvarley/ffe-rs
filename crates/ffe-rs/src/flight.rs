use serde::{Deserialize, Serialize};

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
    /// model is running (it rolls, which `yaw` and `pitch` cannot say). Not saved:
    /// a loaded game falls back to `yaw` and `pitch`.
    #[serde(skip)]
    pub axes: Option<[[f64; 3]; 3]>,
    /// Flying in a system (the navigator's bodies, not the prototype's station at the
    /// origin): the prototype's boundary, docking and safety reset do not apply.
    #[serde(skip)]
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
}

fn default_ship() -> u32 {
    crate::ships::DEFAULT_SHIP
}

#[derive(Debug, Default, Clone, Copy)]
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
        if !self
            .position
            .iter()
            .chain(&self.velocity)
            .chain([&self.yaw, &self.pitch])
            .all(|number| number.is_finite())
            || self
                .position
                .iter()
                .any(|number| number.abs() > 1_000_000.0)
            // The prototype step caps itself at 250; the original flight model
            // (key O) reaches a few thousand flight units per second.
            || self.velocity.iter().any(|number| number.abs() > 5000.0)
            || self.speed() > 5000.0000001
            || !(0.0..std::f64::consts::TAU).contains(&self.yaw)
            || self.pitch.abs() > 1.4
            || self.cargo > CARGO_CAPACITY
            || self.stock > 1000
            || self.credits > 1_000_000_000
            || !(crate::ships::FIRST_SHIP..=crate::ships::LAST_SHIP).contains(&self.ship)
            || !(self.thrust.is_finite() && self.thrust.abs() <= 1.0)
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
        {
            return Err("save contains out-of-range prototype state".to_owned());
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
    /// of hull (or of generators) resets them.
    pub fn ensure_damage(&mut self, hull_full: u16) {
        if hull_full != 0 && self.hull[1] != hull_full {
            self.hull = [hull_full, hull_full];
        }
        let shield_max = self.loadout.shield_max();
        if shield_max != self.shields[1] {
            self.shields = [shield_max, shield_max];
        }
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

    /// Whether the player's hull has been used up (F942 destroys the object).
    pub fn destroyed(&self) -> bool {
        self.hull[1] != 0 && self.hull[0] == 0
    }
}

fn length(vector: [f64; 3]) -> f64 {
    vector
        .iter()
        .map(|number| number * number)
        .sum::<f64>()
        .sqrt()
}
