//! Traffic control at a station, and the player's docking and launch stages: the
//! original's F649-F657 (the record kept at `+0xc9..+0xd8` of the station's
//! physics object), F867's clearance timeout, F865's door word, and the timed
//! stage machine that F1025, F1067/F1068 and F1069-F1088 run once a ship has
//! touched the pad (`docs/notes/docking.md`).
//!
//! Positions and the lift scenes the original draws during the stages are not
//! here: this module is the state and the timing, in the original's units
//! (game tics for stage timeouts, game seconds for the clearance timeout).

use crate::model_animation::door_step;

/// The station record's bytes: `+0xc9` mode, `+0xca` the ship being handled,
/// `+0xce` the highest bay, `+0xd0..` one byte a bay, `+0xd8` the timeout and
/// `+0x9e` the door word (0 is open: the door animation counts down to it when
/// the mode is negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Starport {
    pub mode: u8,
    pub current: u8,
    pub timeout: u16,
    pub bays: u16,
    pub pads: [u8; 8],
    pub door: u16,
}

/// The mode after a ship has asked for permission (F654): the door counts down.
pub const GRANTED: u8 = 0xff;
/// The mode while the door closes behind a docking ship (F652): up to 0xffff.
pub const DOCKING_DONE: u8 = 3;
/// Clearance lasts this many game seconds (F654, F652).
pub const CLEARANCE_SECONDS: u16 = 0x7080;

/// A bay handed out by F650.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bay {
    pub index: u16,
    /// A new bay was taken (F650's flag -1); false when the ship already had one.
    pub fresh: bool,
}

/// What F655 answers (the strings it would print are in the comments).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// 0x98b9: the player owes too much.
    Fined,
    /// 0x98b7: traffic control is handling another ship.
    Busy,
    /// 0x98b8: every bay is taken (the record is reset).
    Full,
    /// 0x98ba (new) or 0x98bc (already granted).
    Granted(Bay),
}

impl Starport {
    /// A station with `bays` bays numbered 0..=`bays` (F880 sets `+0xce` to 3, 5, 7 or 1)
    /// and its door closed.
    pub fn new(bays: u16) -> Self {
        Starport {
            bays,
            door: 0xffff,
            ..Starport::default()
        }
    }

    fn bay_range(&self) -> impl Iterator<Item = usize> {
        let top = (self.bays as i16).min(7);
        (0..=top.max(-1)).rev().map(|i| i as usize)
    }

    /// F649 is a lookup of the station record by object index, which the caller
    /// does with its own list. F654: ask permission for `ship` (docking or
    /// launching). Refused while another ship holds the station in a mode
    /// other than 0 or 1.
    pub fn request(&mut self, ship: u8) -> bool {
        if self.mode != 0 && self.mode != 1 && self.current != ship {
            return false;
        }
        self.mode = GRANTED;
        self.current = ship;
        self.timeout = CLEARANCE_SECONDS;
        true
    }

    /// F650 for an indoor station: the first free bay from the highest down, or
    /// the one the ship already has. `None` when all are taken. (For the player
    /// the original also raises query event 0x17, which only mission code can
    /// veto; it is not modelled.)
    pub fn allocate(&mut self, ship: u8) -> Option<Bay> {
        for index in self.bay_range() {
            if self.pads[index] == ship {
                return Some(Bay {
                    index: index as u16,
                    fresh: false,
                });
            }
            if self.pads[index] == 0 {
                self.pads[index] = ship;
                return Some(Bay {
                    index: index as u16,
                    fresh: true,
                });
            }
        }
        None
    }

    /// F651: the ship's bay is freed and the door closes (mode 1, nobody current).
    pub fn release(&mut self, ship: u8) {
        for index in self.bay_range() {
            if self.pads[index] == ship {
                self.pads[index] = 0;
                break;
            }
        }
        self.mode = 1;
        self.current = 0;
        self.timeout = 0;
    }

    /// F652: the ship has touched the pad. Only the ship that has clearance
    /// starts the docking (mode 3, a new timeout).
    pub fn touch(&mut self, ship: u8) -> bool {
        if ship != self.current {
            return false;
        }
        self.mode = DOCKING_DONE;
        self.timeout = CLEARANCE_SECONDS;
        true
    }

    /// F653: docking complete, the record is reset.
    pub fn reset(&mut self) {
        self.mode = 0;
        self.current = 0;
        self.timeout = 0;
    }

    /// F657: this ship is the one being handled.
    pub fn is_docking(&self, ship: u8) -> bool {
        self.current == ship
    }

    /// F655: the whole request, as the comms screen makes it. `owes` is the
    /// player's outstanding fines at or over the limit (0x2710) and only counts
    /// for the player.
    pub fn request_docking(&mut self, ship: u8, is_player: bool, owes: u32) -> Reply {
        if is_player && owes >= 0x2710 {
            return Reply::Fined;
        }
        if !self.request(ship) {
            return Reply::Busy;
        }
        match self.allocate(ship) {
            None => {
                self.reset();
                Reply::Full
            }
            Some(bay) => Reply::Granted(bay),
        }
    }

    /// F867's countdown of the clearance, `seconds` of game time. When it runs
    /// out the bay is released and the ship that held it is returned.
    pub fn elapse(&mut self, seconds: u32) -> Option<u8> {
        if self.timeout == 0 {
            return None;
        }
        let left = i32::from(self.timeout as i16) as u32;
        if left > seconds {
            self.timeout = self.timeout.wrapping_sub(seconds as u16);
            return None;
        }
        let ship = self.current;
        self.release(ship);
        Some(ship)
    }

    /// F865: one frame of the door, `frame_time` accelerated tics.
    pub fn move_door(&mut self, frame_time: u32) {
        let (word, mode) = door_step(self.door, self.mode as i8, frame_time);
        self.door = word;
        self.mode = mode as u8;
    }
}

/// The player's states during docking and launching (`DATA_008870`).
pub mod state {
    pub const FREE: u8 = 0x00;
    pub const AUTOPILOT: u8 = 0x02;
    pub const STARPORT_ALIGN: u8 = 0x08;
    pub const ALIGN: u8 = 0x0a;
    pub const OPEN_DOOR: u8 = 0x0c;
    pub const INTO_LIFT: u8 = 0x0e;
    pub const TURN: u8 = 0x10;
    pub const CLOSE_DOOR: u8 = 0x12;
    pub const WAIT_IN_LIFT: u8 = 0x14;
    pub const OPEN_INNER: u8 = 0x16;
    pub const INTO_BAY: u8 = 0x18;
    pub const LAUNCH_INTO_LIFT: u8 = 0x1a;
    pub const LAUNCH_CLOSE: u8 = 0x1c;
    pub const LAUNCH_WAIT: u8 = 0x1e;
    pub const LAUNCH_OPEN: u8 = 0x20;
    pub const LAUNCH_OUT: u8 = 0x22;
    pub const TAKEOFF: u8 = 0x24;
    pub const AFTER_TAKEOFF: u8 = 0x26;
    pub const DOCKED_THARGOID: u8 = 0x28;
    pub const DOCKED: u8 = 0x2a;
    pub const LAUNCHING_STATION: u8 = 0x36;
    pub const LAUNCHING_STARPORT: u8 = 0x38;

    /// What the stage is, for a status line.
    pub fn describe(state: u8) -> &'static str {
        match state {
            ALIGN | STARPORT_ALIGN => "aligning with the lift",
            OPEN_DOOR => "opening the lift door",
            INTO_LIFT => "moving into the lift",
            TURN => "turning round in the lift",
            CLOSE_DOOR => "closing the lift door",
            WAIT_IN_LIFT => "waiting in the lift",
            OPEN_INNER => "opening the inner door",
            INTO_BAY => "moving into the bay",
            LAUNCH_INTO_LIFT => "moving into the lift",
            LAUNCH_CLOSE => "closing the lift door",
            LAUNCH_WAIT => "waiting in the lift",
            LAUNCH_OPEN => "opening the lift door",
            LAUNCH_OUT => "moving into the docking area",
            LAUNCHING_STATION | LAUNCHING_STARPORT => "leaving the station",
            _ => "",
        }
    }
}

/// The tics a landing touch gives the first stage (F1019, F1024: `0x70000`).
pub const TOUCH_TIMEOUT: i32 = 0x7_0000;
/// The default stage timeout after a stage ends (F1067: `0x1ffff`).
pub const STAGE_TIMEOUT: i32 = 0x1_ffff;
/// The launch request's first timeout (F363's caller: `0x17530`).
pub const LAUNCH_TIMEOUT: i32 = 0x1_7530;

/// The docking and launching stage machine: `DATA_008870` and `DATA_008884`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stages {
    pub state: u8,
    pub timeout: i32,
}

impl Stages {
    /// The ship has touched the pad of a station (or of a closed starport).
    pub fn touched(starport: bool) -> Self {
        Stages {
            state: if starport {
                state::STARPORT_ALIGN
            } else {
                state::ALIGN
            },
            timeout: TOUCH_TIMEOUT,
        }
    }

    /// Launch permission has been given (F654 passed in the services screen).
    pub fn launch_requested() -> Self {
        Stages {
            state: state::LAUNCH_INTO_LIFT,
            timeout: LAUNCH_TIMEOUT,
        }
    }

    /// Whether the stage is one of the docking or launching sequences.
    pub fn running(&self) -> bool {
        (state::ALIGN..=state::INTO_BAY).contains(&self.state)
            || (state::LAUNCH_INTO_LIFT..=state::LAUNCH_OUT).contains(&self.state)
            || self.state == state::STARPORT_ALIGN
            || self.state == state::DOCKED_THARGOID
            || self.state == state::LAUNCHING_STATION
            || self.state == state::LAUNCHING_STARPORT
    }

    /// F1068: count the timeout down; when it passes zero take `next` and `then`.
    fn stage(&mut self, dt: i32, next: u8, then: i32) {
        self.timeout = self.timeout.wrapping_sub(dt);
        if self.timeout < 0 {
            self.state = next;
            self.timeout = then;
        }
    }

    /// One frame of `dt` game tics (`DATA_009155`). `thargoid` is a station of
    /// model 0x45 or 0x4f (which skip the lift); `door` is the station's door
    /// word, waited for by the launch's last state. Returns the state it left
    /// when it changed.
    pub fn step(&mut self, dt: i32, thargoid: bool, door: u16) -> Option<u8> {
        use state::*;
        let before = self.state;
        match self.state {
            ALIGN | STARPORT_ALIGN => {
                // F1025: leaves when less than a frame is left.
                if self.timeout < dt {
                    if thargoid {
                        self.state = DOCKED_THARGOID;
                    } else {
                        self.state = OPEN_DOOR;
                        self.timeout = STAGE_TIMEOUT;
                    }
                } else {
                    self.timeout -= dt;
                }
            }
            OPEN_DOOR => self.stage(dt, INTO_LIFT, 0x1_8000),
            INTO_LIFT => self.stage(dt, TURN, STAGE_TIMEOUT),
            TURN => self.stage(dt, CLOSE_DOOR, STAGE_TIMEOUT),
            CLOSE_DOOR => self.stage(dt, WAIT_IN_LIFT, STAGE_TIMEOUT),
            WAIT_IN_LIFT => self.stage(dt, OPEN_INNER, STAGE_TIMEOUT),
            OPEN_INNER => self.stage(dt, INTO_BAY, 0x1_7530),
            INTO_BAY => self.stage(dt, DOCKED_THARGOID, STAGE_TIMEOUT),
            // F1091: the docked state follows at once.
            DOCKED_THARGOID => self.state = DOCKED,
            LAUNCH_INTO_LIFT => {
                if thargoid {
                    self.state = LAUNCH_OUT;
                    self.timeout = -1;
                    self.leave(dt);
                } else {
                    self.stage(dt, LAUNCH_CLOSE, STAGE_TIMEOUT);
                }
            }
            LAUNCH_CLOSE => self.stage(dt, LAUNCH_WAIT, STAGE_TIMEOUT),
            LAUNCH_WAIT => self.stage(dt, LAUNCH_OPEN, STAGE_TIMEOUT),
            LAUNCH_OPEN => self.stage(dt, LAUNCH_OUT, 0x1_8000),
            LAUNCH_OUT => self.leave(dt),
            // F1089/F1090: out of the door once it is open.
            LAUNCHING_STATION if door == 0 => self.state = AFTER_TAKEOFF,
            LAUNCHING_STARPORT if door == 0 => self.state = TAKEOFF,
            _ => {}
        }
        (self.state != before).then_some(before)
    }

    /// F1088: the last launch stage; at the end the ship is put out and the
    /// station's door starts to open.
    fn leave(&mut self, dt: i32) {
        self.timeout = self.timeout.wrapping_sub(dt);
        if self.timeout <= 0 {
            self.state = state::LAUNCHING_STATION;
        }
    }
}

/// The launching ship has been put out of the station: F1088 sets the station's
/// mode to 0xff and its door word to 0xffff, so the door counts down to open.
pub fn begin_exit(starport: &mut Starport) {
    starport.mode = GRANTED;
    starport.door = 0xffff;
}

/// The bay and mode when the ship docks (F1025 leaves the touch for the lift
/// stages with F653): the clearance record is reset.
pub fn finish_touch(starport: &mut Starport) {
    starport.reset();
}
