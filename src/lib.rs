pub mod cli_parser;
pub mod hid;
pub mod macros;
pub mod prelude;
pub mod setup;

include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
