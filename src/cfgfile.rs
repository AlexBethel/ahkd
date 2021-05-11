// Configuration file parsing.
// Copyright (C) 2021 by Alexander Bethel.

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

use std::error::Error;
use std::fmt;
use std::io::Read;
use std::io::{BufRead, BufReader};

/// A functional line in the configuration file.
#[derive(Debug)]
pub enum Command {
    /// A `bind' command, indicating that a particular key sequence
    /// should run a shell command.
    Bind {
        // TODO: type this.
        keybinding: (),
        command: Vec<String>,
    },

    /// A `map' command, indicating that a key sequence should trigger
    /// another key sequence.
    Map {
        // TODO: Type these.
        from: (),
        to: (),
    },
}

/// The information from the configuration file.
#[derive(Debug)]
pub struct Config {
    /// The set of commands specified in the file.
    pub commands: Vec<Command>,
}

/// An error arising from parsing.
#[derive(Debug)]
pub struct SyntaxError {
    /// The error message to print.
    pub err_msg: String,

    /// The text of the line on which the error occurred.
    pub line: String,

    /// The line number (starting from 1) on which the error occurred.
    pub line_num: u32,

    /// The column number (starting from 0) of the first erroneous
    /// character.
    pub col_num: u32,

    /// The number of characters past `col_num' to indicate as
    /// erroneous in an error message. This should always be at least
    /// one.
    pub len: u32,
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
            prespace = " ".repeat(self.col_num as usize),
            underline = "~".repeat(self.len as usize)
        )?;
        writeln!(f, "")?;

        // And then the actual message.
        write!(f, "{}", self.err_msg)?;
        Ok(())
    }
}

impl Error for SyntaxError {}

/// Parses a configuration file from an input source.
pub fn parse_config<T: Read>(reader: BufReader<T>) -> Result<Config, Box<dyn Error>> {
    let mut commands = Vec::new();
    for line in reader.lines() {
        if let Some(command) = parse_command(line?)? {
            commands.push(command);
        }
    }

    Ok(Config { commands })
}

/// Attempts to parse the string (which was found at the given line
/// number) as a configuration command. Returns Ok(None) if the line
/// was blank or a comment.
fn parse_command(_command: String) -> Result<Option<Command>, SyntaxError> {
    todo!();
}
