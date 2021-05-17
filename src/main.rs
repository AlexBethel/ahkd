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

use clap::{App, Arg};
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
    let matches = App::new("ahkd")
        .version("0.1.0")
        .author("A. Bethel")
        .about("Hotkey manager for X11")
        .arg(Arg::with_name("config-file").required(true).index(1))
        .arg(
            Arg::with_name("display")
                .short("d")
                .long("display")
                .value_name("DISPLAY")
                .help("Selects the X11 display to connect to")
                .takes_value(true),
        )
        .get_matches();

    // "config" is a required argument, so we can `unwrap` here.
    let config_name = matches.value_of("config-file").unwrap();
    let config_file = File::open(config_name)?;
    let config_buf = BufReader::new(config_file);
    let config = parse_config(config_buf, &config_name)?;

    let display_name = matches.value_of("display");

    daemon(config, display_name)?;
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
