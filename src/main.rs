// Main program entry point.
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

use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;

mod cfgfile;
mod daemon;
mod keyseq;
mod x11;

use cfgfile::parse_config;
use daemon::daemon;

fn main() {
    std::process::exit(if let Err(e) = run() {
        println!("{}", e);
        1
    } else {
        0
    })
}

fn run() -> Result<Infallible, Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let config_name = if args.len() == 2 {
        &args[1]
    } else {
        return Err(Box::new(AhkdError::UsageError(
            args.into_iter().next().unwrap_or("ahkd".to_string()),
        )));
    };

    let config_file = File::open(config_name)?;
    let config_buf = BufReader::new(config_file);
    let config = parse_config(config_buf, &config_name)?;

    daemon(config)?;
    todo!()
}

#[derive(Debug)]
pub enum AhkdError {
    UsageError(String),
    X11Error(String),
    NoKeysError,
    KeyboardGrabError,
}

impl fmt::Display for AhkdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use AhkdError::*;
        write!(
            f,
            "{}",
            match self {
                UsageError(filename) => {
                    format!("Usage: {} <file>", filename)
                }
                X11Error(err_msg) => {
                    format!("X11 error: {}", err_msg)
                }
                NoKeysError => {
                    "Nothing to do\nAt least one command is required in the configuration file."
                        .to_string()
                }
                KeyboardGrabError => {
                    "Unable to grab keyboard".to_string()
                }
            }
        )
    }
}

impl Error for AhkdError {}
