use crate::cli_parser::Args;
use crate::prelude::*;
use chrono::format::{DelayedFormat, StrftimeItems};
use chrono::Local;
use colored::*;
use env_logger::fmt::Formatter;
use env_logger::Builder;
use log::{LevelFilter, Record};
use std::io::Write;
use std::process;

/// Sets up the program by:
/// 1. Set default environment variables generated at build
/// 1. Parse command arguments
/// 2. Initialize the logger
pub fn setup_logger_and_args() -> Args {
    let _args: Args = match Args::parse() {
        Ok(arguments) => arguments,
        Err(error) => {
            println!("Error: {}", error);
            Args::print_help();
            process::exit(1);
        }
    };

    init_logger(_args.verbose);
    trace!("Logger was enabled successfully.");
    debug!("Passed Arguments: {:?}", _args);
    _args
}

/// Initializes the logger (env_logger)
fn init_logger(verbose_level: u8) {
    let mut builder: Builder = Builder::new();

    // Determine log level based on build mode and verbosity flag
    let log_level_filter: LevelFilter = if verbose_level == 1 {
        LevelFilter::Debug
    } else if verbose_level == 2 {
        LevelFilter::Trace
    } else {
        LevelFilter::Info
    };

    builder
        .format(move |buf: &mut Formatter, record: &Record<'_>| {
            // Timestamp for time since program start
            let timestamp: DelayedFormat<StrftimeItems<'_>> = Local::now().format("%H:%M:%S.%3f");

            // Check if log is from executable root (main.rs) and replace with 'main'
            let mut target: String = record.target().to_string();
            let upto: usize = target
                .char_indices()
                .map(|(i, _)| i)
                .nth(
                    target
                        .chars()
                        .position(|c: char| c == ':')
                        .unwrap_or(target.len()),
                )
                .unwrap_or(target.len());
            target.truncate(upto);

            let module_path: String = record.module_path().unwrap_or("UNKNOWN").to_string();
            let level: String = format!("{}{}", record.level(), " ".repeat(5 - record.level().to_string().len()));

            // Log output format
            let log_output: String = if verbose_level >= 2 {
                format!(
                    "[{}] [{}] [{}/{}]: {}",
                    timestamp,
                    module_path,
                    target,
                    level,
                    record.args(),
                )
            } else {
                format!("[{}] [{}/{}]: {}", timestamp, target, level, record.args(),)
            };

            // Apply severity color to the whole log line
            let colored_log: ColoredString = match record.level() {
                log::Level::Error => log_output.bright_red().bold(),
                log::Level::Warn => log_output.bright_yellow(),
                log::Level::Info => log_output.normal(),
                log::Level::Debug => log_output.bright_blue(),
                log::Level::Trace => log_output.bright_black(),
            };

            writeln!(buf, "{}", colored_log)
        })
        .filter(None, log_level_filter)
        .init();
}
