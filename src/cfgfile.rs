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
use std::convert::TryInto;
use std::error::Error;
use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::ops::Range;

/// The information from the configuration file.
#[derive(Debug)]
pub struct Config {
    /// The set of commands specified in the file.
    pub commands: Vec<ConfigLine>,
}

/// A functional line in the configuration file.
#[derive(Debug)]
pub struct ConfigLine {
    /// The key sequence that must be pressed to trigger the action.
    pub keyseq: KeySequence,

    /// The action that will occur when that key sequence is pressed.
    pub action: Action,
}

/// An action implied by a configuration line.
#[derive(Debug)]
pub enum Action {
    /// A `bind` command, indicating that a particular key sequence
    /// should run a shell command.
    Bind {
        /// The shell command to execute.
        command: Vec<String>,
    },

    /// A `map` command, indicating that a key sequence should trigger
    /// another key sequence.
    Map {
        /// The KeySequence to trigger.
        to: KeySequence,
    },
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

    /// The file from which the error originated.
    file_name: String,

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
        writeln!(
            f,
            "Syntax error: {}:{}:{}",
            self.file_name, self.line_num, self.col_num
        )?;

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
            underline = "^".repeat(self.len.max(1))
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
    /// returns a syntax error with the given error message.
    pub fn split1<P: Fn(char) -> bool>(
        &self,
        pattern: P,
        err_msg: &str,
    ) -> Result<(LineText<'a>, LineText<'a>), SyntaxError> {
        match self.as_str().find(pattern) {
            Some(split_idx) => Ok((
                self.substr(None, Some(split_idx)),
                self.substr(Some(split_idx + 1), None),
            )),

            // Highlight the last character if an error occurs.
            None => Err(self
                .substr(Some(self.as_str().len()), None)
                .to_error(err_msg.to_string())),
        }
    }

    /// Removes leading whitespace from the LineText.
    pub fn trim_start(&self) -> Self {
        let idx = self
            .as_str()
            .find(|c: char| !c.is_whitespace())
            .unwrap_or_else(|| self.as_str().len());
        self.substr(Some(idx), None)
    }

    /// Takes a substring of a LineText, between the two byte indices.
    /// If `start` is None, uses the beginning of the string, and if
    /// `end` is None, uses the end of the string.
    pub fn substr(&self, start: Option<usize>, end: Option<usize>) -> Self {
        let start = start.unwrap_or(0);
        let end = end.unwrap_or_else(|| self.as_str().len());

        if end > self.as_str().len() {
            panic!(
                "Invalid LineText substring {}..{} (string is only {} characters long)",
                start,
                end,
                self.range.end - self.range.start
            );
        }

        Self {
            range: self.range.start + start..self.range.start + end,
            ..*self
        }
    }

    /// Converts the LineText into a SyntaxError that highlights this
    /// portion of the line, with the given error message.
    pub fn to_error(self, msg: String) -> SyntaxError {
        SyntaxError {
            err_msg: msg,
            file_name: self.file_name.to_string(),
            line: self.text.to_string(),
            line_num: self.line_num,
            col_num: self.range.start,
            len: self.as_str().len(),
        }
    }

    /// Gets the contents of the referenced section of text.
    pub fn as_str(&self) -> &'a str {
        &self.text[self.range.clone()]
    }
}

impl<'a, P: Fn(char) -> bool> LineSplit<'a, P> {
    /// Gets the text that has not yet been examined by the
    /// `LineSplit`.
    pub fn rest(self) -> LineText<'a> {
        self.line_text
    }

    /// Gets the next split element, ignoring the `merge` option.
    fn next_nomerge(&mut self) -> Option<LineText<'a>> {
        if self.done {
            None
        } else {
            Some(match self.line_text.as_str().find(|c| (self.pattern)(c)) {
                None => {
                    // The whole rest of the string is one block, or
                    // we've reached the end of the string.
                    self.done = true;
                    let section = self.line_text.clone();
                    self.line_text = self
                        .line_text
                        .substr(Some(self.line_text.as_str().len()), None);
                    section
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
                    if text.as_str().len() == 0 {
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
pub fn parse_config<T: Read>(
    reader: BufReader<T>,
    file_name: &str,
) -> Result<Config, Box<dyn Error>> {
    let mut commands = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        // For some reason, line numbers have always started at 1, not
        // 0, so we get to add 1 here.
        let idx = idx + 1;
        let line = line?;

        if let Some(command) = parse_command(LineText::new(file_name, idx, &line))? {
            commands.push(command);
        }
    }

    Ok(Config { commands })
}

/// Attempts to parse the line of text as a configuration command.
/// Returns Ok(None) if the line was blank or a comment.
fn parse_command<'a>(line: LineText<'a>) -> Result<Option<ConfigLine>, SyntaxError> {
    let trimmed = line.trim_start();
    match trimmed.as_str().chars().next() {
        None | Some('#') => {
            // Blank line or comment.
            return Ok(None);
        }
        Some(_) => {}
    }

    let mut split = trimmed.split(char::is_whitespace, true);

    // After trimming the command we got a character at the start,
    // therefore we must logically have at least one word.
    let first_word = split.next().unwrap();

    Ok(Some(match first_word.as_str() {
        "bind" => parse_cmd_bind(split.rest()),
        "map" => parse_cmd_map(split.rest()),
        _ => {
            let errmsg = format!("Unrecognized command \"{}\"", first_word.as_str());
            Err(first_word.to_error(errmsg))
        }
    }?))
}

fn parse_cmd_bind<'a>(args: LineText<'a>) -> Result<ConfigLine, SyntaxError> {
    let (keys, command) = args.split1(|c| c == ':', "Expected \":\"")?;
    Ok(ConfigLine {
        keyseq: keys.try_into()?,
        action: Action::Bind {
            command: command
                .as_str()
                .split_ascii_whitespace()
                .map(|s| s.to_string())
                .collect(),
        },
    })
}

fn parse_cmd_map<'a>(args: LineText<'a>) -> Result<ConfigLine, SyntaxError> {
    let (from, to) = args.split1(|c| c == ':', "Expected \":\"")?;
    Ok(ConfigLine {
        keyseq: from.try_into()?,
        action: Action::Map {
            to: to.try_into()?,
        }
    })
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
            .map(|lt| lt.as_str())
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
            .map(|lt| lt.as_str())
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
        let left = success.0.as_str();
        let right = success.1.as_str();
        assert_eq!((left, right), ("actually", "three words"));

        // Check for exceptional cases.
        let failure = lt.split1(|c| c == 'x', "Expected character 'x'");
        assert!(failure.is_err());
    }
}
