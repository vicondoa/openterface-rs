//! The `openterface-rs` command-line frontend entry point.
//!
//! Parsing is the W1.4 contract (see [`cli`]); command implementations land in
//! W4.1/W5.

mod cli;

use clap::Parser;

fn main() -> std::process::ExitCode {
    cli::Cli::parse().run().into()
}
