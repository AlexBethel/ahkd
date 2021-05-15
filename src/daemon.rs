// Main loop and daemon logic.
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

use crate::cfgfile::Action;
use crate::cfgfile::Config;
use crate::cfgfile::ConfigLine;
use crate::keyseq::Key;
use crate::keyseq::KeySequence;
use crate::x11::X11Conn;
use crate::AhkdError;
use std::collections::HashSet;
use std::convert::Infallible;
use std::error::Error;

/// Runs the daemon with the given configuration..
pub fn daemon(cfg: Config) -> Result<Infallible, Box<dyn Error>> {
    let conn = X11Conn::new()?;
    loop {
        let init_keys = match get_prefixes(&cfg, &[]) {
            PrefixState::None => return Err(Box::new(AhkdError::NoKeysError)),
            PrefixState::Match(_) => {
                // TODO: Guarantee that this is true.
                panic!("Blank key sequences not allowed");
            }
            PrefixState::Prefix(keys) => keys,
        };

        let mut seen_keys = vec![conn.next_key(&init_keys)?];
        loop {
            match get_prefixes(&cfg, &seen_keys) {
                PrefixState::None => {
                    break;
                }
                PrefixState::Prefix(_keys) => {
                    // Do nothing. TODO: we don't need that `_keys`
                    // variable there, get rid of it.
                }
                PrefixState::Match(line) => {
                    do_action(&line.action);
                    break;
                }
            }

            seen_keys.push(conn.next_key_kbd()?);
        }
    }
}

fn do_action(_action: &Action) {
    todo!();
}

/// The state of the keybinding manager at a particular point in time.
enum PrefixState<'a> {
    /// The user has typed something that can't possibly match any key
    /// sequence we're listening for. We should therefore discard any
    /// keys we might have saved.
    None,

    /// The user has typed something that matches one or more key
    /// sequences we're listening for; the vector is of possible keys
    /// to listen for that would match future hotkeys.
    Prefix(Vec<Key>),

    /// The user has typed something that perfectly matches a key
    /// binding we're listening for, with this associated
    /// configuration line.
    Match(&'a ConfigLine),
}

/// The result of matching a key sequence with a set of prefix keys.
enum SeqMatch {
    /// The key sequence does not match the prefix.
    None,

    /// The key sequence partially matches the prefix, and this is the
    /// next key that would be required for a full match.
    Partial(Key),

    /// The key sequence prefectly matches the prefix, i.e., the user
    /// has typed this key sequence to completion..
    Full,
}

/// Attempts to determine what the user meant, given that they've
/// typed the given set of keys `seen_keys' and we're listening for
/// `bindings'.
fn get_prefixes<'a>(config: &'a Config, seen_keys: &[Key]) -> PrefixState<'a> {
    let mut next_keys = HashSet::new();
    for command in &config.commands {
        match match_keyseq(&command.keyseq, seen_keys) {
            SeqMatch::None => {}
            SeqMatch::Partial(key) => {
                next_keys.insert(key);
            }
            SeqMatch::Full => return PrefixState::Match(&command),
        };
    }

    match next_keys.len() {
        0 => PrefixState::None,
        _ => PrefixState::Prefix(next_keys.into_iter().collect()),
    }
}

/// Matches a key sequence with a set of keys we've seen from the
/// user.
fn match_keyseq(seq: &KeySequence, seen_keys: &[Key]) -> SeqMatch {
    for (i, key) in seen_keys.iter().enumerate() {
        if seq.keys[i] != *key {
            return SeqMatch::None;
        }
    }

    if seq.keys.len() == seen_keys.len() {
        SeqMatch::Full
    } else {
        SeqMatch::Partial(seq.keys[seen_keys.len()].clone())
    }
}
