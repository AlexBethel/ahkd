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
use std::convert::TryInto;
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

    /// The number of the lowest valid keycode. This is used to
    /// measure the `keymap` off of.
    min_keycode: u8,

    // TODO: preprocess this somehow, it's nasty to keep this actual
    // reply structure lying around. Maybe turn it into a HashMap or
    // something.
    /// The mapping between keysyms and keycodes.
    keymap: GetKeyboardMappingReply,
}

impl X11Conn {
    /// Connects to the X11 display.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        // TODO: allow arbitrary X11 server names.
        let display = RustConnection::connect(None)?.0;

        let setup = display.setup();
        // TODO: what if there's more than one screen?
        let root_window = setup.roots[0].root;
        let min_keycode = setup.min_keycode;
        let max_keycode = setup.max_keycode;
        let keymap = GetKeyboardMappingRequest {
            first_keycode: min_keycode,
            count: max_keycode - min_keycode,
        }
        .send(&display)?
        .reply()?;

        Ok(Self {
            display,
            root_window,
            min_keycode,
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

    /// Gets the lowest keycode corresponding to a keysym.
    fn keysym_to_keycode(&self, keysym: Keysym) -> u8 {
        // TODO: do this better. This is O(n) in time.
        // TODO: deal with missing keysyms.
        let keysym_idx = self
            .keymap
            .keysyms
            .iter()
            .position(|&x| x == keysym.0)
            .unwrap();

        ((keysym_idx / self.keymap.keysyms_per_keycode as usize) + self.min_keycode as usize)
            .try_into()
            .unwrap()
    }

    /// Gets the first keysym corresponding to a keycode.
    fn keycode_to_keysym(&self, keycode: u8) -> Keysym {
        let idx = (keycode - self.min_keycode) as usize * self.keymap.keysyms_per_keycode as usize;
        let keysym = self.keymap.keysyms[idx];
        Keysym(keysym)
    }

    /// Globally grabs the given set of keys from the keybaord.
    fn grab_keys(&self, keys: &[Key]) -> Result<(), Box<dyn Error>> {
        for key in keys {
            // TODO: why am I making a Keysym here?
            let keycode = self.keysym_to_keycode(Keysym(key.main_key.0));

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
                key: self.keysym_to_keycode(Keysym(key.main_key.0)),
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
            let keysym = self.keycode_to_keysym(keycode);

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
