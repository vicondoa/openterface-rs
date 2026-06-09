//! The `openterface-rs` command-line interface contract.
//!
//! This module defines the clap command/flag surface (`connect` / `scan` /
//! `status` / `reset`, the global `-v/--verbose`, and `--version`).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Process exit codes. The parser (clap) uses `2` for usage errors and `0` for
/// `--help`/`--version`; these are the runtime codes the command layer returns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum ExitCode {
    /// Command completed successfully.
    Success = 0,
    /// A runtime failure (e.g. no device found, connection failed).
    Failure = 1,
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        std::process::ExitCode::from(code as u8)
    }
}

/// Openterface USB KVM — native-Linux, Wayland-only Rust implementation.
#[derive(Parser, Debug)]
#[command(
    name = "openterface-rs",
    version,
    about = "Openterface USB KVM CLI — control a target's keyboard, video, and mouse over USB.",
    disable_version_flag = true
)]
pub(crate) struct Cli {
    /// Print version.
    //
    // Root-only, long-form `--version` (no `-V` short, not propagated to
    // subcommands).
    #[arg(long, action = clap::ArgAction::Version)]
    pub version: Option<bool>,

    /// Enable verbose output.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// The top-level KVM subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Connect to KVM device (auto-discovers devices if none specified).
    Connect(ConnectArgs),

    /// Scan for Openterface devices.
    Scan,

    /// Show device status.
    Status,

    /// Perform factory reset of CH9329 chip.
    Reset(ResetArgs),
}

/// Options for `connect`.
#[derive(Args, Debug)]
pub(crate) struct ConnectArgs {
    /// Video device path (optional - auto-detected if omitted).
    #[arg(long, value_name = "PATH")]
    pub video: Option<PathBuf>,

    /// Serial device path (optional - auto-detected if omitted).
    #[arg(long, value_name = "PATH")]
    pub serial: Option<PathBuf>,

    /// Disable video capture (even if device detected).
    #[arg(long)]
    pub no_video: bool,

    /// Disable input forwarding (even if device detected).
    #[arg(long)]
    pub no_serial: bool,

    /// Run in dummy mode (no device connection, GUI only).
    #[arg(long)]
    pub dummy: bool,

    /// Enable debug output for input events (mouse/keyboard).
    #[arg(long)]
    pub debug: bool,
}

/// Options for `reset`.
#[derive(Args, Debug)]
pub(crate) struct ResetArgs {
    /// Serial device path (required for reset).
    //
    // Kept optional at the parser level: the handler validates presence and
    // prints usage. A `//` comment (not `///`) so this rationale is not rendered
    // in `--help`.
    #[arg(long, value_name = "PATH")]
    pub serial: Option<PathBuf>,
}

impl Cli {
    /// Whether logging should be raised so input/debug output is visible:
    /// `-v/--verbose` or `connect --debug`.
    pub(crate) fn wants_verbose_logging(&self) -> bool {
        self.verbose || matches!(&self.command, Command::Connect(a) if a.debug)
    }

    /// Runs the parsed command.
    pub(crate) fn run(self) -> ExitCode {
        // The command contract prints this banner before command output.
        if self.verbose {
            println!("Verbose mode enabled");
        }
        match &self.command {
            Command::Connect(args) => crate::commands::connect(args, self.verbose),
            Command::Scan => crate::commands::scan(self.verbose),
            Command::Status => crate::commands::status(self.verbose),
            Command::Reset(args) => crate::commands::reset(args, self.verbose),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_contract_is_valid() {
        // Catches clap derive misconfiguration at test time.
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_all_subcommands() {
        assert!(matches!(
            Cli::try_parse_from(["openterface-rs", "scan"])
                .unwrap()
                .command,
            Command::Scan
        ));
        assert!(matches!(
            Cli::try_parse_from(["openterface-rs", "status"])
                .unwrap()
                .command,
            Command::Status
        ));
        assert!(matches!(
            Cli::try_parse_from(["openterface-rs", "connect"])
                .unwrap()
                .command,
            Command::Connect(_)
        ));
        assert!(matches!(
            Cli::try_parse_from(["openterface-rs", "reset", "--serial", "/dev/ttyACM0"])
                .unwrap()
                .command,
            Command::Reset(_)
        ));
    }

    #[test]
    fn connect_flags_parse() {
        let cli = Cli::try_parse_from([
            "openterface-rs",
            "-v",
            "connect",
            "--video",
            "/dev/video2",
            "--serial",
            "/dev/ttyACM0",
            "--no-video",
            "--no-serial",
            "--dummy",
            "--debug",
        ])
        .unwrap();
        assert!(cli.verbose);
        let Command::Connect(args) = cli.command else {
            panic!("expected connect");
        };
        assert_eq!(
            args.video.as_deref(),
            Some(std::path::Path::new("/dev/video2"))
        );
        assert_eq!(
            args.serial.as_deref(),
            Some(std::path::Path::new("/dev/ttyACM0"))
        );
        assert!(args.no_video);
        assert!(args.no_serial);
        assert!(args.dummy);
        assert!(args.debug);
    }

    #[test]
    fn reset_serial_is_optional_at_parse_time() {
        // `--serial` is validated in the handler, not by the parser, so `reset`
        // parses with `serial == None`.
        let cli = Cli::try_parse_from(["openterface-rs", "reset"]).unwrap();
        let Command::Reset(args) = cli.command else {
            panic!("expected reset");
        };
        assert!(args.serial.is_none());
    }

    #[test]
    fn missing_subcommand_is_usage_error() {
        // With no subcommand, clap prints help and exits with a usage error (2).
        let err = Cli::try_parse_from(["openterface-rs"]).unwrap_err();
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn unknown_flag_is_usage_error() {
        let err = Cli::try_parse_from(["openterface-rs", "connect", "--nope"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn version_flag_short_circuits() {
        let err = Cli::try_parse_from(["openterface-rs", "--version"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    }

    #[test]
    fn no_short_version_alias() {
        // Only `--version` is exposed; `-V` must not be accepted.
        let err = Cli::try_parse_from(["openterface-rs", "-V"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn version_is_not_propagated_to_subcommands() {
        // Subcommands do not accept `--version`; only the root does.
        let err = Cli::try_parse_from(["openterface-rs", "scan", "--version"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn verbose_is_global_after_subcommand() {
        let cli = Cli::try_parse_from(["openterface-rs", "scan", "--verbose"]).unwrap();
        assert!(cli.verbose);
    }
}
