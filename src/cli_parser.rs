#![allow(unused_assignments)]

use clap::{ArgMatches, Command, arg, crate_authors, value_parser};
use once_cell::sync::Lazy;
use std::{cmp, env, error::Error, process};
use crate::ENV_VARS;

static LONG_VERSION: Lazy<String> = Lazy::new(|| {
    format!(
        " v{}\nAuthor(s): {}\nDescription: {}\nRepository: {}",
        env!("CARGO_PKG_VERSION"),
        crate_authors!(", "),
        env!("CARGO_PKG_DESCRIPTION"),
        env!("CARGO_PKG_REPOSITORY")
    )
});

static SHORT_VERSION: Lazy<String> = Lazy::new(|| format!(" v{}", env!("CARGO_PKG_VERSION"),));

/// Struct to describe passed command-line arguments
#[derive(Debug)]
#[allow(dead_code)]
pub struct Args {
    pub verbose: u8,
}

impl Args {
    pub fn parse() -> Result<Args, Box<dyn Error>> {
        // Possible arguments
        // verbose: `get_count("verbose")`
        let matches: ArgMatches = Self::command()
            .ignore_errors(true)
            .arg(
                arg!(
                    -v --verbose ... "Turns on verbose logging (Max level of 3, level 3 is not recommended)"
                )
                .value_parser(value_parser!(u8).range(0..=3)),
            )
            .arg(arg!(
                --"debug-info" "Prints out debug info about binary"
            ))
            .get_matches();

        if matches.get_flag("debug-info") {
            println!("{}", Self::debug_info());
            process::exit(0);
        }

        let verbose: u8 = if cfg!(debug_assertions) {
            cmp::max(1, matches.get_count("verbose"))
        } else {
            matches.get_count("verbose")
        };

        Ok(Args { verbose })
    }

    pub fn command() -> Command {
        // crate_name!() = env!("CARGO_PKG_NAME");
        // crate_version!() = env!("CARGO_PKG_VERSION");
        // crate_authors!() = env!("CARGO_PKG_AUTHORS") + custom_separator;
        // crate_description!() = env!("CARGO_PKG_DESCRIPTION");

        let cmd: Command = Command::new(env!("CARGO_PKG_NAME"))
            .author(crate_authors!(", "))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .long_about(env!("CARGO_PKG_DESCRIPTION"))
            .version(SHORT_VERSION.as_str())
            .long_version(LONG_VERSION.as_str());
        cmd
    }

    /// Display version message.
    pub fn print_version() {
        println!("{}", Self::command().render_version());
    }

    /// Display help message.
    pub fn print_help() {
        println!("{}", Self::command().render_help())
    }

    /// Prints debug info about host and binary
    fn debug_info() -> String {
        let env_list: Vec<&str> = vec![
            "CUSTOM_BIN_NAME",
            "CARGO_PKG_VERSION",
            "VERGEN_BUILD_TIMESTAMP",
            "VERGEN_GIT_SHA",
            "VERGEN_GIT_COMMIT_TIMESTAMP",
            "VERGEN_GIT_BRANCH",
            "VERGEN_CARGO_TARGET_TRIPLE",
            "VERGEN_RUSTC_CHANNEL",
            "VERGEN_RUSTC_COMMIT_DATE",
            "VERGEN_RUSTC_COMMIT_HASH",
            "VERGEN_RUSTC_SEMVER",
        ];
        let mut info_text: String = String::from("");

        for environ in env_list {
            let mut trunc_environ: String = environ.to_string();
            trunc_environ.replace_range(..environ.find("_").unwrap() + 1, "");

            let concat_string: String = match ENV_VARS.get(environ) {
                Some(value) => format!("{} = {}\n", trunc_environ, value),
                None => {
                    if environ == "CUSTOM_BIN_NAME" {
                        format!(
                            "BIN_NAME = {}\n",
                            match env::current_exe() {
                                Ok(value) => value.display().to_string(),
                                Err(_) => "UNKNOWN".to_string(),
                            }
                        )
                    } else {
                        format!("{} = NOT_SET\n", trunc_environ)
                    }
                },
            };

            info_text.push_str(&concat_string);
        }

        info_text
    }
}
