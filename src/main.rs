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

use std::error::Error;
use std::fs::File;
use std::io::BufReader;

mod cfgfile;

use cfgfile::parse_config;

fn main() {
    std::process::exit(
        if let Err(e) = run() {
            println!("{}", e);
            1
        } else {
            0
        }
    )
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let config_name = if args.len() == 2 {
        &args[1]
    } else {
        // TODO: Add support for reading from standard input (other
        // than the obvious "ahkd /dev/stdin")
        println!("Usage: {} <file>", args[0]);
        return Ok(());
    };

    let config_file = File::open(config_name)?;
    let config_buf = BufReader::new(config_file);
    let config = parse_config(config_buf)?;

    println!("{:?}", config);
    Ok(())
}
