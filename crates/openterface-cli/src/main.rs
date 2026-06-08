//! The `openterface-rs` command-line frontend entry point.

mod cli;
mod commands;

use clap::Parser;
use tracing_subscriber::EnvFilter;

fn main() -> std::process::ExitCode {
    let cli = cli::Cli::parse();
    init_tracing(cli.wants_verbose_logging());
    cli.run().into()
}

/// Initializes logging. `-v/--verbose` raises the default level to `debug`;
/// `RUST_LOG` overrides either way.
fn init_tracing(verbose: bool) {
    let default = if verbose { "debug" } else { "warn" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "openterface_rs={default},openterface_core={default},openterface_gui={default}"
        ))
    });
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
