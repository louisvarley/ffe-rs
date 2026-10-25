//! The playable missile field: launch, tracking, the F953 damage hand-off and
//! the F1120 pylon words (guide: `docs/notes/combat.md`).
//!
//! [`ffe_core::missile`] holds the original's tables and the missile object.
//! This module owns the live list, turns the cache's 48-byte model statistics
//! into a [`Spec`], and applies a hit through `NpcShip::hit` (F953's
//! shields→hull→death path) so the missile and the laser share one damage
//! implementation.
use ffe_core::laser::Damage;
use ffe_core::missile::{self, Event, Missile, Spec, State, Target};
use ffe_core::npc::NpcShip;

use crate::ships::ShipStats;

/// The live missiles of the current system.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MissileField {
    pub missiles: Vec<Missile>,
    /// The next F941 object id. The core `Missile` has no object slots, so the
    /// field hands each launch an id; `DATA_008877` refers to it.
    next_id: u32,
}

/// A missile F718's ECM destroyed, for F719's explosion and the message log.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EcmLoss {
    pub id: u32,
    /// The missile model (`+0x82`), to explain which ECM defeated it.
    pub model: u32,
    /// The system-frame position (millimetres) F719 puts the model-0x9b object
    /// at.
    pub position: [i64; 3],
    /// The missile's axes, for the explosion object's random matrix.
    pub axes: [[f64; 3]; 3],
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
        if let Some(missile) = self.missiles.last_mut() {
            missile.id = self.next_id;
        }
        self.next_id = self.next_id.wrapping_add(1);
    }

    pub fn clear(&mut self) {
        self.missiles.clear();
    }

    /// The live missile with F941's object id (`+0x86`).
    pub fn get(&self, id: u32) -> Option<&Missile> {
        self.missiles
            .iter()
            .find(|missile| missile.id == id && missile.alive())
    }

    /// F718's ECM branch: destroy every live missile the source's model table
    /// defeats. The frame's `DATA_008835` bits are global, so there is no range
    /// or owner test. Returns the destroyed missiles for F719's explosion.
    pub fn ecm(&mut self, naval: bool) -> Vec<EcmLoss> {
        let mut losses = Vec::new();
        for missile in &mut self.missiles {
            if missile.alive() && missile::ecm_destroys(missile.spec.model, naval) {
                missile.state = State::Spent;
                losses.push(EcmLoss {
                    id: missile.id,
                    model: missile.spec.model,
                    position: missile.position,
                    axes: missile.axes,
                });
            }
        }
        self.missiles.retain(Missile::alive);
        losses
    }

    /// F1057: pull a missile away from `from` to the chaff object `to`. Only
    /// models 6..=11 are affected, and the original rolls
    /// `F1530() & 0x1000` for each; `allow` is that per-missile gate.
    pub fn retarget(
        &mut self,
        from: u32,
        to: u32,
        mut allow: impl FnMut(&Missile) -> bool,
    ) -> usize {
        let mut count = 0;
        for missile in &mut self.missiles {
            if missile.alive()
                && missile.target == Some(from)
                && missile::retargetable(missile.spec.model)
                && allow(missile)
            {
                missile.target = Some(to);
                count += 1;
            }
        }
        count
    }

    /// Tick every missile over `seconds` of world time, then apply F953 to any
    /// ship a missile reached. F758's explosion on timeout does no damage.
    pub fn tick(&mut self, seconds: f64, ships: &mut [NpcShip]) -> Vec<MissileEvent> {
        if self.missiles.is_empty() {
            return Vec::new();
        }
        let targets: Vec<Target> = ships
            .iter()
            .filter(|ship| ship.visible())
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
            station: None,
            bay: None,
            pad: None,
            dock_timer: 0.0,
            dock_retry: 0.0,
            dock_wait: 0.0,
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

    fn spec_for(model: u32) -> Spec {
        Spec::from_words(model, 32700, -6540, 2100)
    }

    #[test]
    fn launches_get_f941_object_ids_and_can_be_found() {
        let mut field = MissileField::default();
        field.launch(spec_for(6), 0x82, 0, None, [0; 3], axes(), 1, 5200);
        field.launch(spec_for(7), 0x83, 0, None, [0; 3], axes(), 2, 5200);
        assert_eq!(field.missiles[0].id, 0);
        assert_eq!(field.missiles[1].id, 1);
        assert!(field.get(0).is_some());
        assert_eq!(field.get(1).unwrap().spec.model, 7);
        assert!(field.get(2).is_none());
    }

    #[test]
    fn ecm_destroys_f718s_models_and_only_f719s_explosions_remain() {
        let mut field = MissileField::default();
        for model in [4, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
            field.launch(spec_for(model), 0x82, 0, None, [0; 3], axes(), 1, 5200);
        }
        // The standard system defeats models 5 and 6; its DATA_008835 bit 2.
        let losses = field.ecm(false);
        assert_eq!(
            losses.iter().map(|loss| loss.model).collect::<Vec<_>>(),
            vec![5, 6]
        );
        assert!(losses.iter().all(|loss| loss.id < 10));
        // F719's object data is the missile's position and axes (the odd-pylon
        // separation offset from F941).
        assert_eq!(losses[0].position, [0x2000, -5200, 0]);
        assert_eq!(losses[0].axes, axes());
        // Naval E.C.M. adds 7, 9 and 10.
        let losses = field.ecm(true);
        assert_eq!(
            losses.iter().map(|loss| loss.model).collect::<Vec<_>>(),
            vec![7, 9, 10]
        );
        // 4, 8, 11, 12 and 13 are immune to both.
        assert_eq!(
            field
                .missiles
                .iter()
                .map(|missile| missile.spec.model)
                .collect::<Vec<_>>(),
            vec![4, 8, 11, 12, 13]
        );
        assert!(field.ecm(true).is_empty());
    }

    #[test]
    fn f1057_retargets_only_the_models_it_names_and_only_from_the_chosen_target() {
        let mut field = MissileField::default();
        field.launch(spec_for(6), 0x82, 0, Some(1), [0; 3], axes(), 1, 5200);
        field.launch(spec_for(5), 0x81, 0, Some(1), [0; 3], axes(), 1, 5200);
        field.launch(spec_for(7), 0x83, 0, Some(1), [0; 3], axes(), 1, 5200);
        field.launch(spec_for(8), 0x84, 0, Some(2), [0; 3], axes(), 1, 5200);
        // F1530's 0x1000 gate is per missile; this one accepts.
        assert_eq!(field.retarget(1, 99, |_| true), 2);
        assert_eq!(field.missiles[0].target, Some(99));
        assert_eq!(field.missiles[1].target, Some(1), "model 5 is not named");
        assert_eq!(field.missiles[2].target, Some(99));
        assert_eq!(field.missiles[3].target, Some(2), "another target");
        // A failed roll leaves them all alone.
        assert_eq!(field.retarget(99, 0, |_| false), 0);
        assert_eq!(field.missiles[0].target, Some(99));
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
