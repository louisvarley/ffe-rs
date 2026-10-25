//! The playable missile field: launch, tracking, the F953 damage hand-off and
//! the F1120 pylon words (guide: `docs/notes/combat.md`).
//!
//! [`ffe_core::missile`] holds the original's tables and the missile object.
//! This module owns the live list, turns the cache's 48-byte model statistics
//! into a [`Spec`], and applies a hit through `NpcShip::hit` (F953's
//! shields→hull→death path) so the missile and the laser share one damage
//! implementation.
use ffe_core::laser::Damage;
use ffe_core::missile::{self, Event, Missile, Spec, Target};
use ffe_core::npc::NpcShip;

use crate::ships::ShipStats;

/// The live missiles of the current system.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MissileField {
    pub missiles: Vec<Missile>,
}

/// What a tick did, for the message log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissileEvent {
    /// F720/F721/F953: the hit ship and the outcome.
    Hit {
        target: u32,
        name: String,
        damage: u32,
        outcome: Damage,
    },
    /// F758's timeout, F719's explosion without damage.
    Expired { target: Option<u32> },
}

impl MissileField {
    /// F941: a spec from a missile code, the model's statistics and the F1533
    /// collision radius. Models without statistics (4 and 5) fall back to a
    /// ballistic launch, as their words are zero in the cache.
    pub fn spec(code: u8, stats: Option<ShipStats>, radius: i64) -> Option<Spec> {
        let model = missile::model(code)?;
        Some(match stats {
            Some(stats) => {
                Spec::from_words(model, stats.forward_thrust, stats.reverse_thrust, radius)
            }
            None => Spec::ballistic(model, radius),
        })
    }

    /// F941/F1514: put a missile in flight. `pylon` is 1-based.
    #[allow(clippy::too_many_arguments)]
    pub fn launch(
        &mut self,
        spec: Spec,
        code: u8,
        owner: u32,
        target: Option<u32>,
        position: [i64; 3],
        axes: [[f64; 3]; 3],
        pylon: usize,
        launcher_radius: i64,
    ) {
        self.missiles.push(Missile::launch(
            spec,
            missile::ai_state(code),
            owner,
            target,
            position,
            axes,
            pylon,
            launcher_radius,
        ));
    }

    pub fn clear(&mut self) {
        self.missiles.clear();
    }

    /// Tick every missile over `seconds` of world time, then apply F953 to any
    /// ship a missile reached. F758's explosion on timeout does no damage.
    pub fn tick(&mut self, seconds: f64, ships: &mut [NpcShip]) -> Vec<MissileEvent> {
        if self.missiles.is_empty() {
            return Vec::new();
        }
        let targets: Vec<Target> = ships
            .iter()
            .filter(|ship| ship.alive())
            .map(|ship| Target {
                id: ship.id,
                position: ship.position,
                velocity: ship.velocity,
                radius: ship.radius,
            })
            .collect();
        let mut events = Vec::new();
        for missile in &mut self.missiles {
            match missile.tick(seconds, &targets) {
                Some(Event::Hit { target, damage }) => {
                    if let Some(ship) = ships.iter_mut().find(|ship| ship.id == target) {
                        let name = ship.name.clone();
                        let outcome = ship.hit(damage);
                        events.push(MissileEvent::Hit {
                            target,
                            name,
                            damage,
                            outcome,
                        });
                    }
                }
                Some(Event::Expired { target }) => {
                    events.push(MissileEvent::Expired { target });
                }
                None => {}
            }
        }
        self.missiles.retain(|missile| missile.alive());
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffe_core::missile::AI_MISSILE;
    use ffe_core::npc::{Role, State};

    fn stats() -> ShipStats {
        let mut block = [0u8; 48];
        let word = |block: &mut [u8; 48], offset: usize, value: i16| {
            block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        };
        word(&mut block, 0x00, 32700);
        word(&mut block, 0x02, -6540);
        word(&mut block, 0x06, 25);
        word(&mut block, 0x0e, 0x4014);
        word(&mut block, 0x12, 2);
        ShipStats::from_block(&block).unwrap()
    }

    fn ship(id: u32, position: [i64; 3]) -> NpcShip {
        NpcShip {
            id,
            model: 23,
            role: Role::Pirate,
            name: "Pirate 1".to_owned(),
            position,
            velocity: [0.0; 3],
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            radius: 10_000,
            damage: ffe_core::laser::DamageState {
                shields: 100,
                shields_max: 100,
                hull: 100,
                mass: 25,
            },
            shield_strength: 20,
            cargo: 8,
            state: State::Attack,
            anchor: [0; 3],
            orbit_radius: 0.0,
            phase: 0.0,
            max_speed: 0.0,
            accel: 0.0,
            turn_rate: 0.0,
            speed: 0.0,
            hostile: true,
            laser: None,
            guns: ffe_core::laser::State::default(),
            bounty: 0,
            leader: None,
            formation: [0; 3],
            pass: 0.0,
            flee: 0.0,
            flash: 0.0,
            death: 0.0,
        }
    }

    fn axes() -> [[f64; 3]; 3] {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    }

    #[test]
    fn the_player_code_maps_to_model_6_and_200_damage() {
        let spec = MissileField::spec(0x82, Some(stats()), 2100).unwrap();
        assert_eq!(spec.model, 6);
        assert_eq!(spec.damage, 200);
        assert_eq!(spec.radius, 2100);
        assert!(spec.forward > 19_000.0);
        assert!(spec.turn > 3.0);
        assert_eq!(
            MissileField::spec(0xc0, Some(stats()), 1200).map(|spec| spec.model),
            Some(4)
        );
        assert!(MissileField::spec(0x8a, None, 0).is_none());
    }

    #[test]
    fn a_launched_missile_hits_a_nearby_ship_through_f953_and_is_removed() {
        let mut field = MissileField::default();
        let spec = MissileField::spec(0x82, Some(stats()), 2100).unwrap();
        let mut ships = vec![ship(3, [0, 0, 120_000])];
        ships[0].damage.shields = 300;
        ships[0].damage.shields_max = 300;
        field.launch(spec, 0x82, 0, Some(3), [0; 3], axes(), 1, 5200);
        assert_eq!(field.missiles.len(), 1);
        assert_eq!(field.missiles[0].ai_state, AI_MISSILE);
        // The missile's separation position (8192, -5200, 0) is inside the
        // 131 m fuse of a target 120 m away at the first guidance update.
        let events = field.tick(ffe_core::missile::GUIDANCE_INTERVAL, &mut ships);
        assert_eq!(events.len(), 1);
        match &events[0] {
            MissileEvent::Hit {
                target,
                name,
                damage,
                outcome,
            } => {
                assert_eq!((*target, name.as_str(), *damage), (3, "Pirate 1", 200));
                assert_eq!(*outcome, Damage::Shields);
            }
            other => panic!("{other:?}"),
        }
        assert!(field.missiles.is_empty());
        assert_eq!(ships[0].damage.shields, 100);
        assert_eq!(ships[0].damage.hull, 100);
    }

    #[test]
    fn a_kill_goes_through_f942_and_a_timeout_does_not_damage() {
        let mut field = MissileField::default();
        let spec = MissileField::spec(0x82, Some(stats()), 2100).unwrap();
        let mut ships = vec![ship(3, [0, 0, 120_000])];
        ships[0].damage.shields = 0;
        ships[0].damage.hull = 100;
        field.launch(spec, 0x82, 0, Some(3), [0; 3], axes(), 1, 5200);
        let events = field.tick(ffe_core::missile::GUIDANCE_INTERVAL, &mut ships);
        assert!(matches!(
            events.as_slice(),
            [MissileEvent::Hit {
                outcome: Damage::Destroyed,
                ..
            }]
        ));
        assert!(!ships[0].alive());
        assert_eq!(ships[0].state, State::Dead);

        let mut field = MissileField::default();
        field.launch(spec, 0x82, 0, Some(99), [0; 3], axes(), 1, 5200);
        let mut far = vec![ship(3, [0, 0, 10_000_000])];
        let mut events = Vec::new();
        for _ in 0..61 {
            events.extend(field.tick(ffe_core::missile::GUIDANCE_INTERVAL, &mut far));
        }
        assert!(matches!(events.as_slice(), [MissileEvent::Expired { .. }]));
        assert!(field.missiles.is_empty());
        assert_eq!(far[0].damage.hull, 100);
    }
}
