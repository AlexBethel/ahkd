// Configuration file parsing.
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

use crate::keyseq::KeySequence;
use std::error::Error;
use std::fmt;
use std::io::Read;
use std::io::{BufRead, BufReader};
use std::ops::Range;

/// The information from the configuration file.
#[derive(Debug)]
pub struct Config {
    /// The set of commands specified in the file.
    pub commands: Vec<Command>,
}

/// A functional line in the configuration file.
#[derive(Debug)]
pub enum Command {
    /// A `bind` command, indicating that a particular key sequence
    /// should run a shell command.
    Bind {
        keybinding: KeySequence,
        command: Vec<String>,
    },

    /// A `map` command, indicating that a key sequence should trigger
    /// another key sequence.
    Map { from: KeySequence, to: KeySequence },
}

/// A substring of a line of text obtained from an input file.
#[derive(Clone, PartialEq, Debug)]
pub struct LineText<'a> {
    // TODO: once SyntaxError has one of these, we don't need these to
    // be public.
    /// The name of the file from which the text originated.
    pub file_name: &'a str,

    /// The line number of the text.
    pub line_num: usize,

    /// The text of the complete line.
    pub text: &'a str,

    /// The range of characters delimiting the substring.
    pub range: Range<usize>,
}

/// Iterator over the sections in a line of text separated by some
/// pattern.
pub struct LineSplit<'a, 'b, P: Fn(char) -> bool> {
    /// The LineText we're sourcing string data from.
    line_text: &'a LineText<'b>,

    /// The pattern we're splitting by.
    pattern: P,

    /// Whether to merge repeated instances of the pattern, i.e.,
    /// whether `"foo---bar"` with a pattern of `'-'` would report
    /// `"foo","bar"` rather than `"foo","","","bar"`.
    merge: bool,

    /// The index of the first character in the LineText's range that
    /// hasn't been looked at yet (such that 0 means the first
    /// character is `line_text.text[line_text.range.start]`). If this
    /// is None, we're done searching.
    next_char: Option<usize>,
}

/// An error arising from parsing.
#[derive(Debug)]
pub struct SyntaxError {
    // TODO: Build this structure around LineText.
    /// The error message to print.
    pub err_msg: String,

    /// The text of the line on which the error occurred.
    pub line: String,

    /// The line number (starting from 1) on which the error occurred.
    pub line_num: usize,

    /// The column number (starting from 0) of the first erroneous
    /// character.
    pub col_num: usize,

    /// The number of characters past `col_num` to indicate as
    /// erroneous in an error message. This should always be at least
    /// one.
    pub len: usize,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let margin = 4;

        // Declare we have a syntax error.
        writeln!(f, "Syntax error:")?;

        // Draw the offending line next to its line number, with the
        // erroneous section underlined.
        writeln!(f, "{} |", " ".repeat(margin))?;
        writeln!(
            f,
            "{num:>margin$} | {line}",
            margin = margin,
            num = self.line_num,
            line = self.line
        )?;
        writeln!(
            f,
            "{padding} | {prespace}{underline}",
            padding = " ".repeat(margin),
            prespace = " ".repeat(self.col_num),
            underline = "^".repeat(self.len)
        )?;
        // writeln!(f, "")?;

        // And then the actual message.
        write!(f, "{}   {}", " ".repeat(margin), self.err_msg)?;
        Ok(())
    }
}

impl Error for SyntaxError {}

impl<'a> LineText<'a> {
    /// Splits the LineText at each occurrence of a character that
    /// satisfies `pattern`. If `merge` is true, merges multiple
    /// instances of the pattern into one, thereby never emitting a
    /// blank string.
    pub fn split<'b, P: Fn(char) -> bool>(
        &'b self,
        pattern: P,
        merge: bool,
    ) -> LineSplit<'a, 'b, P> {
        LineSplit {
            line_text: self,
            pattern,
            next_char: Some(0),
            merge,
        }
    }

    /// Splits the LineText at the first occurrence of a character
    /// that satisfies `pattern`. If one is found, returns the text
    /// before that character and the text after it; otherwise,
    /// returns None.
    pub fn split1<P: Fn(char) -> bool>(
        &self,
        pattern: P,
        err_msg: &str,
    ) -> Result<(LineText<'a>, LineText<'a>), SyntaxError> {
        let remaining = &self.text[self.range.clone()];
        match remaining.find(|c| pattern(c)) {
            Some(split_idx) => Ok((
                // This range manipulation strikes me as nasty... not
                // sure if Rust has a better way of doing it though.
                LineText {
                    range: self.range.start..self.range.start + split_idx,
                    ..*self
                },
                LineText {
                    range: self.range.start + split_idx + 1..self.range.end,
                    ..*self
                },
            )),
            None => {
                Err(SyntaxError {
                    err_msg: err_msg.to_string(),
                    line: self.text.to_string(),
                    line_num: self.line_num,
                    // Highlight the last character of the LineText.
                    col_num: self.range.end,
                    len: 1,
                })
            }
        }
    }

    /// Gets the actual string underlying the LineText.
    pub fn get_text(&self) -> &'a str {
        // TODO: make a struct field that tracks this rather than
        // creating it using a function like this. This function is
        // O(n) (assuming the compiler doesn't optimize it out or
        // anything), but we could be O(1) if we just kept track of
        // the result.
        &self.text[self.range.clone()]
    }
}

impl<'a, 'b, P: Fn(char) -> bool> LineSplit<'a, 'b, P> {
    /// Gets the next split element, ignoring the `merge` option.
    fn next_nomerge(&mut self) -> Option<LineText<'b>> {
        let next_char = match self.next_char {
            Some(c) => c,
            None => return None,
        };

        let remaining = &self.line_text.get_text()[next_char..];
        match remaining.find(|c| (self.pattern)(c)) {
            None => {
                // The whole rest of the string is one block, or we've
                // reached the end of the string.
                self.next_char = None;

                Some(LineText {
                    range: self.line_text.range.start + next_char..self.line_text.range.end,
                    ..*self.line_text
                })
            }
            Some(idx) => {
                self.next_char = Some(next_char + idx + 1);
                Some(LineText {
                    // TODO: prove this won't go over the end.
                    range: self.line_text.range.start + next_char
                        ..self.line_text.range.start + next_char + idx,
                    ..*self.line_text
                })
            }
        }
    }
}

impl<'a, 'b, P: Fn(char) -> bool> Iterator for LineSplit<'a, 'b, P> {
    // LineSplit produces more LineTexts that are substrings of the
    // original.
    type Item = LineText<'b>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.next_nomerge();
        if self.merge {
            match item {
                None => None,
                Some(text) => {
                    if text.get_text().len() == 0 {
                        self.next()
                    } else {
                        Some(text)
                    }
                }
            }
        } else {
            item
        }
    }
}

/// Parses a configuration file from an input source.
pub fn parse_config<T: Read>(reader: BufReader<T>) -> Result<Config, Box<dyn Error>> {
    let mut commands = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        // For some reason, line numbers have always started at 1, not
        // 0, so we get to add 1 here.
        if let Some(command) = parse_command(line?, idx + 1)? {
            commands.push(command);
        }
    }

    Ok(Config { commands })
}

/// Attempts to parse the string (which was found at the given line
/// number) as a configuration command. Returns Ok(None) if the line
/// was blank or a comment.
fn parse_command(command: String, line: usize) -> Result<Option<Command>, SyntaxError> {
    let trimmed = command.trim_start();
    match trimmed.chars().next() {
        None | Some('#') => {
            // Blank line or comment.
            return Ok(None);
        }
        Some(_) => {}
    }

    // After trimming the command we got a character at the start,
    // therefore we must logically have at least one word.
    let first_word = trimmed.split(' ').next().unwrap();

    match first_word {
        "bind" => parse_cmd_bind(trimmed.to_string(), line),
        "map" => parse_cmd_map(trimmed.to_string(), line),
        _ => {
            let len = first_word.len();
            let indentation = command.len() - trimmed.len();
            Err(SyntaxError {
                err_msg: format!("Unrecognized command \"{}\"", first_word),
                line: command,
                line_num: line,
                col_num: indentation,
                len,
            })
        }
    }
}

fn parse_cmd_bind(command: String, line: usize) -> Result<Option<Command>, SyntaxError> {
    println!("bind ({}): {}", line, command);
    todo!();
}

fn parse_cmd_map(command: String, line: usize) -> Result<Option<Command>, SyntaxError> {
    println!("map ({}): {}", line, command);
    todo!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_test() {
        // Check basic functionality.
        let text = "two words";
        let lt = LineText {
            file_name: "foo",
            line_num: 10,
            text,
            range: 0..text.len(),
        };

        let split: Vec<_> = lt
            .split(char::is_whitespace, true)
            .map(|lt| lt.get_text())
            .collect();
        assert_eq!(split, vec!["two", "words"]);

        // Check whether repeated delimiters are condensed properly.
        let text = " \t  with  \n  whitespace\t\t   ";
        let lt = LineText {
            file_name: "foo",
            line_num: 10,
            text,
            range: 0..text.len(),
        };

        let split: Vec<_> = lt
            .split(char::is_whitespace, true)
            .map(|lt| lt.get_text())
            .collect();
        assert_eq!(split, vec!["with", "whitespace"]);
    }

    #[test]
    fn split1_test() {
        let text = "actually three words";
        let lt = LineText {
            file_name: "foo",
            line_num: 10,
            text,
            range: 0..text.len(),
        };

        // Check normal function.
        let success = lt.split1(char::is_whitespace, "Expected space").unwrap();
        let left = success.0.get_text();
        let right = success.1.get_text();
        assert_eq!((left, right), ("actually", "three words"));

        // Check for exceptional cases.
        let failure = lt.split1(|c| c == 'x', "Expected character 'x'");
        assert!(failure.is_err());
    }
}
