use minifb::Key;

#[derive(Default)]
pub struct HeldControls {
    held: Vec<Key>,
    blocked: Vec<Key>,
}

impl HeldControls {
    pub fn update(&mut self, down: &[Key], pressed: &[Key], enabled: bool) {
        self.held.retain(|key| down.contains(key));
        self.blocked.retain(|key| down.contains(key));
        if !enabled {
            self.disarm(down);
            return;
        }
        for key in pressed {
            if down.contains(key) && !self.blocked.contains(key) && !self.held.contains(key) {
                self.held.push(*key);
            }
        }
    }

    pub fn disarm(&mut self, down: &[Key]) {
        self.held.clear();
        self.blocked = down.to_vec();
    }

    pub fn contains(&self, key: &Key) -> bool {
        self.held.contains(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_control_requires_a_real_press_and_clears_on_release() {
        let mut input = HeldControls::default();
        input.update(&[Key::W], &[], true);
        assert!(!input.contains(&Key::W));
        input.update(&[Key::W], &[Key::W], true);
        assert!(input.contains(&Key::W));
        input.update(&[Key::W], &[], true);
        assert!(input.contains(&Key::W));
        input.update(&[], &[], true);
        assert!(!input.contains(&Key::W));
    }

    #[test]
    fn disabled_controls_stay_blocked_until_released_even_if_reported_as_pressed() {
        let mut input = HeldControls::default();
        input.update(&[Key::W], &[Key::W], true);
        input.update(&[Key::W, Key::Right], &[Key::Right], false);
        input.update(&[Key::W, Key::Right], &[Key::W, Key::Right], true);
        assert!(!input.contains(&Key::W));
        assert!(!input.contains(&Key::Right));
        input.update(&[Key::Right], &[], true);
        input.update(&[Key::W, Key::Right], &[Key::W], true);
        assert!(input.contains(&Key::W));
        assert!(!input.contains(&Key::Right));
    }

    #[test]
    fn mode_transition_disarms_all_current_keys_without_blocking_new_keys() {
        let mut input = HeldControls::default();
        input.update(&[Key::W], &[Key::W], true);
        input.disarm(&[Key::W]);
        input.update(&[Key::W, Key::S], &[Key::W, Key::S], true);
        assert!(!input.contains(&Key::W));
        assert!(input.contains(&Key::S));
        input.update(&[], &[Key::W], true);
        assert!(!input.contains(&Key::W));
        input.update(&[Key::W], &[Key::W], true);
        assert!(input.contains(&Key::W));
    }
}
