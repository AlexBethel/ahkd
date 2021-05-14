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
use std::io::{Read, BufRead, BufReader};
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
    /// The name of the file from which the text originated.
    file_name: &'a str,

    /// The line number of the text.
    line_num: usize,

    /// The text of the complete line.
    text: &'a str,

    /// The range of characters delimiting the substring.
    range: Range<usize>,

    /// The value of `text[range]`, i.e., another pointer into `text`.
    /// This value takes O(n) to compute because of unicode, so we
    /// cache it here and can therefore fetch it in O(1).
    substring: &'a str,
}

/// Iterator over the sections in a line of text separated by some
/// pattern.
pub struct LineSplit<'a, P: Fn(char) -> bool> {
    /// Whether we're done reading subsections.
    done: bool,

    /// The LineText we haven't looked at yet; this shrinks as we read
    /// more subsections.
    line_text: LineText<'a>,

    /// The pattern we're splitting by.
    pattern: P,

    /// Whether to merge repeated instances of the pattern, i.e.,
    /// whether `"foo---bar"` with a pattern of `'-'` would report
    /// `"foo","bar"` rather than `"foo","","","bar"`.
    merge: bool,
}

/// An error arising from parsing.
#[derive(Debug)]
pub struct SyntaxError {
    /// The error message to print.
    err_msg: String,

    /// The text of the line on which the error occurred.
    line: String,

    /// The line number (starting from 1) on which the error occurred.
    line_num: usize,

    /// The column number (starting from 0) of the first erroneous
    /// character.
    col_num: usize,

    /// The number of characters past `col_num` to indicate as
    /// erroneous in an error message. This should always be at least
    /// one.
    len: usize,
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
        write!(f, "{}", self.err_msg)?;
        Ok(())
    }
}

impl Error for SyntaxError {}

impl<'a> LineText<'a> {
    /// Creates a new LineText given the name of the source file, the
    /// line number, and the text of that line.
    pub fn new(file_name: &'a str, line_num: usize, text: &'a str) -> Self {
        Self {
            file_name,
            line_num,
            text,
            range: 0..text.len(),
            substring: text,
        }
    }

    /// Splits the LineText at each occurrence of a character that
    /// satisfies `pattern`. If `merge` is true, merges multiple
    /// instances of the pattern into one, thereby never emitting a
    /// blank string.
    pub fn split<P: Fn(char) -> bool>(&self, pattern: P, merge: bool) -> LineSplit<'a, P> {
        LineSplit {
            done: false,
            line_text: self.clone(),
            pattern,
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
                self.substr(None, Some(split_idx)),
                self.substr(Some(split_idx + 1), None),
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

    /// Takes a substring of a LineText, between the two indices. If
    /// `start` is None, uses the beginning of the string, and if
    /// `end` is None, uses the end of the string.
    pub fn substr(&self, start: Option<usize>, end: Option<usize>) -> Self {
        let start = start.unwrap_or(0);
        let end = end.unwrap_or(self.range.end - self.range.start);

        Self {
            range: self.range.start + start..self.range.start + end,
            substring: &self.substring[start..end],
            ..*self
        }
    }

    /// Converts the LineText into a SyntaxError that highlights this
    /// portion of the line, with the given error message.
    pub fn to_error(self, msg: String) -> SyntaxError {
        SyntaxError {
            err_msg: msg,
            line: self.text.to_string(),
            line_num: self.line_num,
            col_num: self.range.start,
            len: self.substring.len(),
        }
    }
}

impl<'a> AsRef<str> for LineText<'a> {
    fn as_ref(&self) -> &str {
        self.substring
    }
}

impl<'a, P: Fn(char) -> bool> LineSplit<'a, P> {
    /// Gets the next split element, ignoring the `merge` option.
    fn next_nomerge(&mut self) -> Option<LineText<'a>> {
        if self.done {
            None
        } else {
            Some(match self.line_text.substring.find(|c| (self.pattern)(c)) {
                None => {
                    // The whole rest of the string is one block, or
                    // we've reached the end of the string.
                    self.done = true;
                    self.line_text.clone()
                }
                Some(idx) => {
                    let section = self.line_text.substr(None, Some(idx));
                    self.line_text = self.line_text.substr(Some(idx + 1), None);
                    section
                }
            })
        }
    }
}

impl<'a, P: Fn(char) -> bool> Iterator for LineSplit<'a, P> {
    // LineSplit produces more LineTexts that are substrings of the
    // original.
    type Item = LineText<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.next_nomerge();
        if self.merge {
            match item {
                None => None,
                Some(text) => {
                    if text.substring.len() == 0 {
                        // Don't return a blank substring if `merge`
                        // is true.
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
        // let text = "two words";
        // let lt = LineText {
        //     file_name: "foo",
        //     line_num: 10,
        //     text,
        //     range: 0..text.len(),
        //     substring: text,
        // };

        // let split: Vec<_> = lt
        //     .split(char::is_whitespace, true)
        //     .map(|lt| lt.substring)
        //     .collect();
        // assert_eq!(split, vec!["two", "words"]);

        // Check whether repeated delimiters are condensed properly.
        let text = " \t  with  \n  whitespace\t\t   ";
        let lt = LineText {
            file_name: "foo",
            line_num: 10,
            text,
            range: 0..text.len(),
            substring: text,
        };

        let split: Vec<_> = lt
            .split(char::is_whitespace, true)
            .map(|lt| lt.substring)
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
            substring: text,
        };

        // Check normal function.
        let success = lt.split1(char::is_whitespace, "Expected space").unwrap();
        let left = success.0.substring;
        let right = success.1.substring;
        assert_eq!((left, right), ("actually", "three words"));

        // Check for exceptional cases.
        let failure = lt.split1(|c| c == 'x', "Expected character 'x'");
        assert!(failure.is_err());
    }
}
