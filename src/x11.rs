// X11 backend for ahkd.
// Copyright (C) 2021 by Alexander Bethel.

// This file is part of ahkd.

// ahkd is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// ahkd is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public
// License for more details.

// You should have received a copy of the GNU General Public License
// along with ahkd. If not, see <https://www.gnu.org/licenses/>.

use crate::keyseq::{Key, Keysym, ModField};
use crate::AhkdError;
use std::collections::HashMap;
use std::error::Error;
use x11_keysymdef::lookup_by_keysym;
use x11rb::connection::Connection;
use x11rb::errors::ConnectionError;
use x11rb::protocol::{
    xproto::{
        GetKeyboardMappingReply, GetKeyboardMappingRequest, GrabKeyRequest, GrabKeyboardRequest,
        GrabMode, GrabStatus, ModMask, UngrabKeyRequest, UngrabKeyboardRequest, Window,
    },
    Event,
};
use x11rb::rust_connection::RustConnection;
use x11rb::CURRENT_TIME;

/// A structure for sending and receiving X11 events.
pub struct X11Conn {
    /// The display we're connected to.
    display: RustConnection,

    /// The root window of that display.
    root_window: Window,

    /// The keyboard mapping.
    keymap: KeyMap,
}

/// A converter between keycodes and keysyms.
struct KeyMap {
    /// The mapping from keysyms to keycodes.
    ks_to_kc: HashMap<u32, u8>,

    /// The mapping from keycodes to keysyms.
    kc_to_ks: HashMap<u8, u32>,
}

impl X11Conn {
    /// Connects to the X11 display.
    pub fn new(display_name: Option<&str>) -> Result<Self, Box<dyn Error>> {
        let display = RustConnection::connect(display_name)?.0;

        let setup = display.setup();
        // TODO: what if there's more than one screen?
        let root_window = setup.roots[0].root;
        let min_keycode = setup.min_keycode;
        let max_keycode = setup.max_keycode;
        let keymap_pkt = GetKeyboardMappingRequest {
            first_keycode: min_keycode,
            count: max_keycode - min_keycode,
        }
        .send(&display)?
        .reply()?;

        let keymap = KeyMap::new(min_keycode, keymap_pkt);

        Ok(Self {
            display,
            root_window,
            keymap,
        })
    }

    /// Listens for the given set of keys, and returns the first key
    /// pressed.
    pub fn next_key(&self, keys: &[Key]) -> Result<Key, Box<dyn Error>> {
        self.grab_keys(&keys)?;
        let k = self.get_key()?;
        self.ungrab_keys(&keys)?;

        Ok(k)
    }

    /// Listens for any keypress on the entire keyboard, and returns
    /// the first key pressed.
    pub fn next_key_kbd(&self) -> Result<Key, Box<dyn Error>> {
        self.grab_kbd()?;
        let k = self.get_key()?;
        self.ungrab_kbd()?;

        Ok(k)
    }

    /// Globally grabs the given set of keys from the keybaord.
    fn grab_keys(&self, keys: &[Key]) -> Result<(), Box<dyn Error>> {
        for key in keys {
            // TODO: why am I making a Keysym here?
            let keycode = self.keymap.keysym_to_keycode(Keysym(key.main_key.0));

            GrabKeyRequest {
                owner_events: false,
                grab_window: self.root_window,
                modifiers: (&key.modifiers).into(),
                key: keycode,
                pointer_mode: GrabMode::ASYNC,
                keyboard_mode: GrabMode::ASYNC,
            }
            .send(&self.display)?
            .check()?;
        }

        Ok(())
    }

    /// Globally grabs the entire keyboard.
    fn grab_kbd(&self) -> Result<(), Box<dyn Error>> {
        let reply = GrabKeyboardRequest {
            owner_events: false,
            grab_window: self.root_window,
            time: CURRENT_TIME,
            pointer_mode: GrabMode::ASYNC,
            keyboard_mode: GrabMode::ASYNC,
        }
        .send(&self.display)?
        .reply()?;

        if reply.status != GrabStatus::SUCCESS {
            Err(Box::new(AhkdError::KeyboardGrabError))
        } else {
            Ok(())
        }
    }

    /// Ungrabs the given set of keys.
    fn ungrab_keys(&self, keys: &[Key]) -> Result<(), Box<dyn Error>> {
        for key in keys {
            UngrabKeyRequest {
                key: self.keymap.keysym_to_keycode(Keysym(key.main_key.0)),
                grab_window: self.root_window,
                modifiers: (&key.modifiers).into(),
            }
            .send(&self.display)?
            .check()?;
        }

        Ok(())
    }

    /// Ungrabs the keyboard.
    fn ungrab_kbd(&self) -> Result<(), Box<dyn Error>> {
        UngrabKeyboardRequest { time: CURRENT_TIME }
            .send(&self.display)?
            .check()?;
        Ok(())
    }

    /// Waits for and returns a keypress event from the X11 server.
    /// For this function to ever return successfully, it is necessary
    /// to have informed the X11 server in advance that we wish to
    /// receive some set of keypresses, using either grab_keys() or
    /// grab_kbd().
    fn get_key(&self) -> Result<Key, ConnectionError> {
        loop {
            if let Some(key) = self.event_to_key(self.display.wait_for_event()?) {
                return Ok(key);
            }
        }
    }

    /// Determines what keypress the event corresponds to, if any.
    fn event_to_key(&self, ev: Event) -> Option<Key> {
        if let Event::KeyPress(e) = ev {
            let keycode = e.detail;
            let modifiers = e.state.into();
            let keysym = self.keymap.keycode_to_keysym(keycode);

            // We received the keysym from the X11 server, so it must
            // be a valid keysym number, so we can `unwrap` here.
            if lookup_by_keysym(keysym.0).unwrap().unicode != 0 as char {
                return Some(Key {
                    modifiers,
                    main_key: keysym,
                });
            }
        }
        None
    }
}

impl KeyMap {
    /// Sets up the mappings between keysyms and keycodes.
    pub fn new(min_keycode: u8, packet: GetKeyboardMappingReply) -> Self {
        Self {
            ks_to_kc: packet
                .keysyms
                .iter()
                .enumerate()
                .map(|(keycode, keysym)| {
                    (
                        *keysym,
                        (keycode / packet.keysyms_per_keycode as usize) as u8 + min_keycode,
                    )
                })
                .collect(),
            kc_to_ks: packet
                .keysyms
                .iter()
                .step_by(packet.keysyms_per_keycode as _)
                .enumerate()
                .map(|(keycode, keysym)| (keycode as u8 + min_keycode, *keysym))
                .collect(),
        }
    }

    /// Gets the lowest keycode corresponding to a keysym.
    fn keysym_to_keycode(&self, keysym: Keysym) -> u8 {
        // TODO: deal with missing keysyms.
        *self.ks_to_kc.get(&keysym.0).unwrap_or_else(|| todo!())
    }

    /// Gets the first keysym corresponding to a keycode.
    fn keycode_to_keysym(&self, keycode: u8) -> Keysym {
        // All valid keycodes must have at least one associated
        // keysym, so we can `unwrap` here.
        Keysym(*self.kc_to_ks.get(&keycode).unwrap())
    }
}

impl From<&ModField> for u16 {
    fn from(mods: &ModField) -> Self {
        // I wonder if we can use some bitfield crate or something to
        // make this a little more elegant?
        let mut n = 0;

        let mut mask_if = |flag, mask| {
            if flag {
                n |= mask;
            }
        };

        mask_if(mods.mod_shift, ModMask::SHIFT);
        mask_if(mods.mod_control, ModMask::CONTROL);
        mask_if(mods.mod1, ModMask::M1);
        mask_if(mods.mod2, ModMask::M2);
        mask_if(mods.mod3, ModMask::M3);
        mask_if(mods.mod4, ModMask::M4);
        mask_if(mods.mod5, ModMask::M5);

        n
    }
}

impl From<u16> for ModField {
    fn from(n: u16) -> Self {
        Self {
            mod_shift: n & u16::from(ModMask::SHIFT) != 0,
            mod_control: n & u16::from(ModMask::CONTROL) != 0,
            mod1: n & u16::from(ModMask::M1) != 0,
            mod2: n & u16::from(ModMask::M2) != 0,
            mod3: n & u16::from(ModMask::M3) != 0,
            mod4: n & u16::from(ModMask::M4) != 0,
            mod5: n & u16::from(ModMask::M5) != 0,
        }
    }
}
