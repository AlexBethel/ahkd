// Key sequence data structure, and related structures and functions.
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

use crate::cfgfile::{LineText, SyntaxError};
use std::convert::{TryFrom, TryInto};
use std::hash::{Hash, Hasher};
use x11_keysymdef::{lookup_by_codepoint, lookup_by_name};

/// A sequence of keys that might be pressed. This type represents the
/// selector of the `map` and `bind` commands, and the target of the
/// `map` command.
#[derive(PartialEq, Debug)]
pub struct KeySequence {
    pub keys: Vec<Key>,
}

/// A key, with zero or more modifiers applied.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Key {
    pub modifiers: ModField,
    pub main_key: Keysym,
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.modifiers.hash(state);

        // `Keysym` isn't Hash, so we get to hash its component
        // instead (which slightly annoys me).
        self.main_key.0.hash(state);
    }
}

/// A set of modifiers that might be applied to a key.
#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct ModField {
    pub mod_shift: bool,
    pub mod_control: bool,
    pub mod1: bool, // Alt, Meta
    pub mod2: bool, // Num lock
    pub mod3: bool, // Unused
    pub mod4: bool, // Super, Hyper
    pub mod5: bool, // Unused
}

/// The number corresponding to a symbol on a specific key.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Keysym(pub u32);

impl<'a> TryFrom<LineText<'a>> for KeySequence {
    type Error = SyntaxError;

    fn try_from(value: LineText<'a>) -> Result<Self, Self::Error> {
        let mut keys = Vec::new();
        for word in value.split(char::is_whitespace, true) {
            keys.push(word.try_into()?);
        }

        Ok(Self { keys })
    }
}

impl<'a> TryFrom<LineText<'a>> for Key {
    type Error = SyntaxError;

    fn try_from(text: LineText<'a>) -> Result<Self, Self::Error> {
        let mut subkeys: Vec<_> = text.split(|c| c == '-' || c == '+', false).collect();
        let mut modifiers = ModField {
            mod_shift: false,
            mod_control: false,
            mod1: false,
            mod2: false,
            mod3: false,
            mod4: false,
            mod5: false,
        };

        // This can only fail if `text` is of length 0, which is
        // impossible because we're only called from
        // KeySequence::try_from, which uses LineText::split with
        // `merge = true`, which never emits blank LineTexts.
        let last = subkeys.pop().unwrap();
        for modifier in subkeys.into_iter() {
            modifiers.add(modifier)?;
        }

        Ok(Self {
            main_key: last.try_into()?,
            modifiers,
        })
    }
}

impl ModField {
    /// Attempts to add a modifier key with the given name.
    fn add(&mut self, modifier: LineText<'_>) -> Result<(), SyntaxError> {
        let text = modifier.as_str();
        match text {
            // Case-sensitive short names.
            "C" => self.mod_control = true,
            "S" => self.mod_shift = true,
            "A" | "M" => self.mod1 = true,
            "s" | "h" => self.mod4 = true,

            // Non-case-sensitive long names.
            _ => match &*text.to_ascii_lowercase() {
                "control" | "ctrl" => self.mod_control = true,
                "shift" => self.mod_shift = true,
                "mod1" | "alt" | "meta" => self.mod1 = true,
                "mod2" => self.mod2 = true,
                "mod3" => self.mod3 = true,
                // Hyper and super are bound to the same modifier on
                // my computer; not sure whether that's the case on
                // all keyboards or not...
                "mod4" | "super" | "windows" | "command" | "hyper" => self.mod4 = true,
                "mod5" => self.mod5 = true,
                _ => {
                    let errmsg = format!("Invalid modifier \"{}\"", text);
                    return Err(modifier.to_error(errmsg));
                }
            },
        }

        Ok(())
    }
}

impl<'a> TryFrom<LineText<'a>> for Keysym {
    type Error = SyntaxError;

    fn try_from(text: LineText<'a>) -> Result<Self, Self::Error> {
        let record = match (text.as_str()).len() {
            // len is 1, so we must have a zeroth character, so unwrap
            // is OK here.
            1 => lookup_by_codepoint(text.as_str().chars().next().unwrap()),
            _ => lookup_by_name(text.as_str()),
        };

        match record {
            Some(record) => Ok(Self(record.keysym)),
            None => {
                let errmsg = format!("Invalid keysym \"{}\"", text.as_str());
                Err(text.to_error(errmsg))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructs a LineText from the given string, using some
    /// template values for testing.
    fn mk_lt<'a>(text: &'a str) -> LineText<'a> {
        LineText::new("foo.txt", 10, text)
    }

    #[test]
    fn keysym_parse_test() {
        // Test keysym parse results against results from xev.
        let from_codepoint: Keysym = mk_lt("[").try_into().unwrap();
        assert_eq!(from_codepoint, Keysym(0x5b));

        let from_name: Keysym = mk_lt("bracketleft").try_into().unwrap();
        assert_eq!(from_name, Keysym(0x5b));
    }

    #[test]
    fn key_parse_test() {
        // Basic Emacs-style keybinding.
        let key: Key = mk_lt("C-M-x").try_into().unwrap();
        assert_eq!(
            key,
            Key {
                main_key: mk_lt("x").try_into().unwrap(),
                modifiers: ModField {
                    mod_shift: false,
                    mod_control: true,
                    mod1: true,
                    mod2: false,
                    mod3: false,
                    mod4: false,
                    mod5: false,
                }
            }
        );
    }

    #[test]
    fn ambiguous_modifier_parse_test() {
        // Could be interpreted as "control", but should be
        // interpreted as just a capital C.
        let key: Key = mk_lt("C").try_into().unwrap();
        assert_eq!(
            key,
            Key {
                main_key: mk_lt("C").try_into().unwrap(),
                modifiers: ModField {
                    mod_shift: false,
                    mod_control: false,
                    mod1: false,
                    mod2: false,
                    mod3: false,
                    mod4: false,
                    mod5: false,
                }
            }
        );
    }

    #[test]
    fn missing_main_key_parse_test() {
        // A keybinding without a main key.
        let invalid = Key::try_from(mk_lt("C-M-"));
        assert!(invalid.is_err());
    }

    #[test]
    fn long_key_name_parse_test() {
        // Long key names.
        let key: Key = mk_lt("control-meta-super-shift-hyper-z")
            .try_into()
            .unwrap();
        assert_eq!(
            key,
            Key {
                main_key: mk_lt("z").try_into().unwrap(),
                modifiers: ModField {
                    mod_shift: true,
                    mod_control: true,
                    mod1: true,
                    mod2: false,
                    mod3: false,
                    mod4: true,
                    mod5: false,
                }
            }
        );
    }

    #[test]
    fn short_case_sensitive_parse_test() {
        // Short key names should be case sensitive.
        let key: Key = mk_lt("C-z").try_into().unwrap();
        let invalid = Key::try_from(mk_lt("c-z"));
        // TODO: test shift vs. super

        assert_eq!(
            key,
            Key {
                main_key: mk_lt("z").try_into().unwrap(),
                modifiers: ModField {
                    mod_shift: false,
                    mod_control: true,
                    mod1: false,
                    mod2: false,
                    mod3: false,
                    mod4: false,
                    mod5: false,
                }
            }
        );
        assert!(invalid.is_err());
    }

    #[test]
    fn long_non_case_sensitive_parse_test() {
        // Long key names should not be case-sensitive.
        let key: Key = mk_lt("CoNtRoL-MeTa-sUpEr-z").try_into().unwrap();
        assert_eq!(
            key,
            Key {
                main_key: mk_lt("z").try_into().unwrap(),
                modifiers: ModField {
                    mod_shift: false,
                    mod_control: true,
                    mod1: true,
                    mod2: false,
                    mod3: false,
                    mod4: true,
                    mod5: false,
                }
            }
        );
    }

    #[test]
    #[ignore]
    fn shift_test() {
        // This feature is not implemented yet, and may never be
        // implemented.

        // The "shift" modifier should capitalize the main key.
        let k1: Key = mk_lt("x").try_into().unwrap();
        let k2: Key = mk_lt("shift+x").try_into().unwrap();
        let k3: Key = mk_lt("X").try_into().unwrap();

        assert_ne!(k1.main_key, k2.main_key);
        assert_eq!(k2, k3);
    }

    #[test]
    fn multi_key_binding_test() {
        // KeySequences consisting of multiple keys, separated by
        // whitespace.
        let kb: KeySequence = mk_lt("  C-x M-g\t    \nM-z  ").try_into().unwrap();
        assert_eq!(
            kb,
            KeySequence {
                keys: vec![
                    Key {
                        main_key: mk_lt("x").try_into().unwrap(),
                        modifiers: ModField {
                            mod_shift: false,
                            mod_control: true,
                            mod1: false,
                            mod2: false,
                            mod3: false,
                            mod4: false,
                            mod5: false,
                        },
                    },
                    Key {
                        main_key: mk_lt("g").try_into().unwrap(),
                        modifiers: ModField {
                            mod_shift: false,
                            mod_control: false,
                            mod1: true,
                            mod2: false,
                            mod3: false,
                            mod4: false,
                            mod5: false,
                        },
                    },
                    Key {
                        main_key: mk_lt("z").try_into().unwrap(),
                        modifiers: ModField {
                            mod_shift: false,
                            mod_control: false,
                            mod1: true,
                            mod2: false,
                            mod3: false,
                            mod4: false,
                            mod5: false,
                        },
                    },
                ],
            }
        )
    }
}
