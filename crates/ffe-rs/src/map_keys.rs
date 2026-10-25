//! The galaxy map's own function keys (`F840`, `ffe.asm:57197-57478`).
//!
//! The original's map page takes F5-F12 for itself: F5 re-shows the F834
//! controls row, F6 selects the row, F7 details, F8 picture, F9/F10 details
//! with the next/previous image and F11/F12 zoom. The port's global keys use
//! some of the same functions (F5 save/load, F6-F10 views, F11 screenshot,
//! F12 model viewer), so ownership is explicit: only the original galaxy map
//! page owns F5-F12. The port's Tab system map, the `n` places list and every
//! other screen keep the global keys.
//!
//! The port-only map keys (arrows, Page Up/Down, `+`/`-`, `C`, `H`, `Enter`,
//! drag) stay alongside the original bindings; see `docs/notes/galaxy-map.md`.

use crate::console::MapConsole;
use crate::navigation::View;

/// What an owned function key asks the map to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapAction {
    /// F5: F834 re-shows the map page and its controls row.
    Page,
    /// F6: select/reset the controls row (0x23).
    Select,
    /// F7: the details page (0x22).
    Details,
    /// F8: picture mode (0x21).
    Picture,
    /// F9: details with the next image (0x22 + 0x20).
    Next,
    /// F10: details with the previous image (0x22 + 0x1f).
    Previous,
    /// F11: zoom in (the original's `sub` of the zoom word).
    ZoomIn,
    /// F12: zoom out (the original's `add` of the zoom word).
    ZoomOut,
}

/// Whether the original's map page owns F5-F12. Only [`View::Galaxy`] is the
/// F834/F840 page; `View::System` (the port's Tab map) and `View::Places` (the
/// navigation computer) are not. `page_available` is false when the cache has
/// no sector tables, so the list fallback keeps the global keys too.
/// Whether `view` is the original F834/F840 map page. Only [`View::Galaxy`] is;
/// `View::System` (the port's Tab map) and `View::Places` (the navigation
/// computer) are not.
pub fn owns(view: View) -> bool {
    view == View::Galaxy
}

/// Whether the original's map page owns F5-F12. `page_available` is false when
/// the cache has no sector tables, so the list fallback keeps the global keys
/// too.
pub fn owns_keys(view: View, page_available: bool) -> bool {
    page_available && owns(view)
}

/// The map-local action of a function-key number (`5`..=`12`), or `None` for
/// every other key.
pub fn action(function: u8) -> Option<MapAction> {
    Some(match function {
        5 => MapAction::Page,
        6 => MapAction::Select,
        7 => MapAction::Details,
        8 => MapAction::Picture,
        9 => MapAction::Next,
        10 => MapAction::Previous,
        11 => MapAction::ZoomIn,
        12 => MapAction::ZoomOut,
        _ => return None,
    })
}

/// The map action of a console click while the map page is showing. F969
/// installs the flight console's hot areas (their action codes are the function
/// keys F1-F12) and then adds `DATA_007241`'s ignored action 0x80, so a click on
/// the map row is the same function key the keyboard sends: F5 at 129,163
/// (0xf5), F6 front (0xf6), F7 rear (0xf7), F8 turret (0xf8), F9 external
/// (0xf9), F10 missile (0xfa), F11 combat (0xfb) and F12 capsule (0xfc).
/// Nothing outside that range is a map action.
pub fn click(click: crate::console::Click) -> Option<MapAction> {
    use crate::console::Click;
    Some(match click {
        Click::Screen(5) => MapAction::Page,
        Click::Front => MapAction::Select,
        Click::Rear => MapAction::Details,
        Click::Turret => MapAction::Picture,
        Click::External => MapAction::Next,
        Click::Missile => MapAction::Previous,
        Click::Combat => MapAction::ZoomIn,
        Click::Capsule => MapAction::ZoomOut,
        _ => return None,
    })
}

/// The console page a function key selects (`None` for the scene-only zoom
/// keys). F5 returns to the controls page, which is the state `F834` leaves.
pub fn page(action: MapAction) -> Option<MapConsole> {
    Some(match action {
        MapAction::Page => MapConsole::default(),
        MapAction::Select => MapConsole {
            select: true,
            ..MapConsole::default()
        },
        MapAction::Details => MapConsole {
            details: true,
            ..MapConsole::default()
        },
        MapAction::Picture => MapConsole {
            picture: true,
            ..MapConsole::default()
        },
        MapAction::Next => MapConsole {
            details: true,
            next: true,
            ..MapConsole::default()
        },
        MapAction::Previous => MapConsole {
            details: true,
            previous: true,
            ..MapConsole::default()
        },
        MapAction::ZoomIn | MapAction::ZoomOut => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f5_to_f12_are_the_original_map_keys() {
        let expected = [
            MapAction::Page,
            MapAction::Select,
            MapAction::Details,
            MapAction::Picture,
            MapAction::Next,
            MapAction::Previous,
            MapAction::ZoomIn,
            MapAction::ZoomOut,
        ];
        for (offset, expected) in expected.into_iter().enumerate() {
            assert_eq!(action(5 + offset as u8), Some(expected));
        }
        // Nothing outside F5-F12 is map-local.
        for function in (0..=4).chain(13..=24) {
            assert_eq!(action(function), None, "F{function}");
        }
    }

    #[test]
    fn only_the_galaxy_page_owns_the_keys() {
        assert!(owns(View::Galaxy));
        for view in [View::Closed, View::System, View::Places] {
            assert!(!owns(view), "{view:?} must keep the global keys");
        }
        assert!(owns_keys(View::Galaxy, true));
        assert!(!owns_keys(View::Galaxy, false));
        for view in [View::Closed, View::System, View::Places] {
            assert!(!owns_keys(view, true));
        }
    }

    #[test]
    fn the_map_rows_hot_areas_are_the_same_function_keys() {
        use crate::console::Click;
        for (c, expected) in [
            (Click::Screen(5), MapAction::Page),
            (Click::Front, MapAction::Select),
            (Click::Rear, MapAction::Details),
            (Click::Turret, MapAction::Picture),
            (Click::External, MapAction::Next),
            (Click::Missile, MapAction::Previous),
            (Click::Combat, MapAction::ZoomIn),
            (Click::Capsule, MapAction::ZoomOut),
        ] {
            assert_eq!(click(c), Some(expected));
        }
        for c in [
            Click::Screen(2),
            Click::Screen(3),
            Click::Screen(4),
            Click::CycleView,
            Click::Navigation,
            Click::Hyperspace,
            Click::Target,
            Click::Labels,
            Click::Autopilot,
            Click::AutopilotOff,
            Click::Pause,
            Click::Acceleration(0),
        ] {
            assert_eq!(click(c), None, "{c:?} is not a map action");
        }
    }

    #[test]
    fn the_console_pages_match_f840s_images() {
        assert_eq!(page(MapAction::Page), Some(MapConsole::default()));
        assert_eq!(
            page(MapAction::Select),
            Some(MapConsole {
                select: true,
                ..MapConsole::default()
            })
        );
        assert_eq!(
            page(MapAction::Details),
            Some(MapConsole {
                details: true,
                ..MapConsole::default()
            })
        );
        assert_eq!(
            page(MapAction::Picture),
            Some(MapConsole {
                picture: true,
                ..MapConsole::default()
            })
        );
        assert_eq!(
            page(MapAction::Next),
            Some(MapConsole {
                details: true,
                next: true,
                ..MapConsole::default()
            })
        );
        assert_eq!(
            page(MapAction::Previous),
            Some(MapConsole {
                details: true,
                previous: true,
                ..MapConsole::default()
            })
        );
        assert_eq!(page(MapAction::ZoomIn), None);
        assert_eq!(page(MapAction::ZoomOut), None);
    }
}
