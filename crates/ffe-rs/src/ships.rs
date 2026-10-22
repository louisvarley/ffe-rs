//! The flyable craft: original model table slots 14..=70, the ship range of the
//! original's model name table (`DATA_004682`, cache v11). Slots named
//! "Unknown", or holding text rather than a name, are left out, as are craft the
//! viewer cannot draw on their own.
use crate::model_viewer::ModelViewer;
use ffe_content::VerifiedCache;

/// First and last model table slots of the original's ship range.
pub const FIRST_SHIP: u32 = 14;
pub const LAST_SHIP: u32 = 70;
/// The Eagle long range fighter (slot 20), used when a save names no ship.
pub const DEFAULT_SHIP: u32 = 20;

#[derive(Debug, Clone, Default)]
pub struct ShipList {
    ships: Vec<(u32, String)>,
}

impl ShipList {
    /// Ships with an original name (cache v11) and a model the viewer can draw.
    /// Empty without the name table.
    pub fn from_cache(cache: &VerifiedCache, models: &ModelViewer) -> Self {
        let Some(table) = cache.asset("base:text/model-names") else {
            return Self::default();
        };
        let names: Vec<String> = table
            .split(|b| *b == 0)
            .map(|name| name.iter().map(|b| char::from(*b)).collect())
            .collect();
        let ships = (FIRST_SHIP..=LAST_SHIP)
            .filter_map(|index| {
                let name = names.get(index as usize)?.trim();
                let readable =
                    name.chars().next().is_some_and(char::is_alphabetic) && name != "Unknown";
                let drawable = models.find(index).and_then(|m| m.radius()).is_some();
                (readable && drawable).then(|| (index, name.to_owned()))
            })
            .collect();
        Self { ships }
    }

    pub fn from_ships(ships: Vec<(u32, String)>) -> Self {
        Self { ships }
    }

    pub fn is_empty(&self) -> bool {
        self.ships.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ships.len()
    }

    pub fn name(&self, index: u32) -> Option<&str> {
        self.ships
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, name)| name.as_str())
    }

    /// The ship `step` places after `current` in table order, wrapping; the first
    /// ship when `current` is not in the list.
    pub fn step(&self, current: u32, step: i32) -> Option<u32> {
        let count = self.ships.len() as i32;
        if count == 0 {
            return None;
        }
        let position = self.ships.iter().position(|(i, _)| *i == current);
        let next = match position {
            Some(position) => (position as i32 + step).rem_euclid(count),
            None => 0,
        };
        Some(self.ships[next as usize].0)
    }
}
