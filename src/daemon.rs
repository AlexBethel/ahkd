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

use crate::cfgfile::{Action, Config, ConfigLine};
use crate::keyseq::{Key, KeySequence};
use crate::x11::X11Conn;
use std::convert::Infallible;
use std::error::Error;
use std::process::Command;
use std::thread;

/// Runs the daemon with the given configuration and on the given X11
/// display (or the default display if none is specified).
pub fn daemon(cfg: Config, display_name: Option<&str>) -> Result<Infallible, Box<dyn Error>> {
    let conn = X11Conn::new(display_name)?;
    let init_keys = get_init_keys(&cfg);
    loop {
        let mut seen_keys = vec![conn.next_key(&init_keys)?];
        loop {
            match get_prefixes(&cfg, &seen_keys) {
                PrefixState::Prefix => {
                    seen_keys.push(conn.next_key_kbd()?);
                }
                PrefixState::None => {
                    break;
                }
                PrefixState::Match(line) => {
                    do_action(&line.action);
                    break;
                }
            }
        }
    }
}

/// Performs the action indicated by the Action structure.
fn do_action(action: &Action) {
    match action {
        Action::Bind { command } => {
            match Command::new(&command[0]).args(command[1..].iter()).spawn() {
                Ok(mut handle) => {
                    // Need to call `wait()` at some point because
                    // Unix.
                    thread::spawn(move || {
                        // Ignore errors here. We don't care about the
                        // return status of whatever the user had us
                        // invoke, and dealing with errors there is
                        // their problem.
                        let _ignored = handle.wait();
                    });
                }
                Err(err) => {
                    println!("Error launching \"{}\": {}", &command[0], err);
                }
            }
        }
        Action::Map { to: _ } => {
            todo!();
        }
    }
}

/// The state of the keybinding manager at a particular point in time.
enum PrefixState<'a> {
    /// The user has typed something that can't possibly match any key
    /// sequence we're listening for. We should therefore discard any
    /// keys we might have saved.
    None,

    /// The user has typed something that matches one or more key
    /// sequences we're listening for.
    Prefix,

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
    Partial,

    /// The key sequence prefectly matches the prefix, i.e., the user
    /// has typed this key sequence to completion..
    Full,
}

// TODO: this description is a little unclear for my taste.
/// Gets the set of all keys that should be grabbed initially, given
/// the configuration.
fn get_init_keys(config: &Config) -> Vec<Key> {
    config
        .commands
        .iter()
        .map(|cmd| cmd.keyseq.keys[0])
        .collect()
}

/// Attempts to determine what the user meant, given that they've
/// typed the given set of keys `seen_keys' and we're listening for
/// `bindings'.
fn get_prefixes<'a>(config: &'a Config, seen_keys: &[Key]) -> PrefixState<'a> {
    let mut partial = false;
    for command in &config.commands {
        match match_keyseq(&command.keyseq, seen_keys) {
            SeqMatch::None => {}
            SeqMatch::Partial => {
                partial = true;
            }
            SeqMatch::Full => return PrefixState::Match(&command),
        };
    }

    if partial {
        PrefixState::Prefix
    } else {
        PrefixState::None
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
        SeqMatch::Partial
    }
}
