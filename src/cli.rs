//! Command-line arguments

use clap::{Args, Parser};
use sqrub::ShellCommand;
use std::{num::NonZeroUsize, path::PathBuf};

/// Format the SQL inside sqlx query macros with an external formatter
#[derive(Debug, Parser)]
#[command(version)]
pub struct Cli {
    /// Files or directories to format; directories are searched recursively,
    /// skipping hidden and ignored paths
    #[arg(default_value = ".")]
    pub inputs: Vec<PathBuf>,

    /// Formatter command, split like a shell command line; it receives SQL on
    /// stdin and must write the formatted SQL to stdout
    #[arg(short, long, value_name = "COMMAND")]
    pub formatter: ShellCommand,

    /// Report files that would change without writing them, exiting with 1 if
    /// any would
    #[arg(long)]
    pub check: bool,

    /// Accept formatter output even when the formatter exits with a non-zero
    /// status
    #[arg(long)]
    pub ignore_exit_code: bool,

    /// Number of formatter commands to run at once; defaults to one per CPU
    /// core
    #[arg(short, long, value_name = "N")]
    pub jobs: Option<NonZeroUsize>,

    /// Minimum number of `#`s around raw string literals sqrub writes
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub min_hashes: u8,

    #[command(flatten)]
    pub verbosity: Verbosity,
}

#[derive(Debug, Args)]
pub struct Verbosity {
    /// Print only errors
    #[arg(short, long, conflicts_with = "verbose")]
    quiet: bool,

    /// Also list files that are already formatted
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Quiet,
    Normal,
    Verbose,
}

impl Verbosity {
    pub const fn level(&self) -> Level {
        match (self.quiet, self.verbose) {
            (true, _) => Level::Quiet,
            (_, true) => Level::Verbose,
            _ => Level::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn arguments_are_consistent() {
        Cli::command().debug_assert();
    }
}
