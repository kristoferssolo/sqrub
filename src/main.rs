//! Formats the SQL inside sqlx query macros with an external formatter

mod cli;

use crate::cli::{Cli, Level};
use clap::Parser;
use miette::{Diagnostic, NamedSource, Report};
use sqrub::{
    Error, ExternalFormatter, QueryDiagnostic, RewriteError, Rewriter, SqlFormatter, rust_files,
};
use std::{
    fmt::{self, Display, Formatter},
    fs,
    path::Path,
    process::ExitCode,
    sync::Arc,
};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(outcome) => outcome.into(),
        Err(error) => {
            report(error);
            Outcome::FAILED.into()
        }
    }
}

fn run(cli: &Cli) -> Result<Outcome, Error> {
    let formatter = ExternalFormatter::new(cli.formatter.clone(), cli.ignore_exit_code);
    let mut rewriter = Rewriter::new(formatter, cli.min_hashes).map_err(Error::Grammar)?;
    let level = cli.verbosity.level();
    let mut outcome = Outcome::default();

    for entry in rust_files(&cli.inputs) {
        let result = entry.map_err(Error::from).and_then(|path| {
            let file = format_file(&path, &mut rewriter, cli.check)?;
            Ok((path, file))
        });

        match result {
            Ok((path, file)) => {
                outcome.failed |= !file.failures.is_empty();
                outcome.would_change |= file.status == Status::WouldChange;
                file.failures.into_iter().for_each(report);
                if level == Level::Verbose
                    || (level == Level::Normal && file.status != Status::Unchanged)
                {
                    println!("{}: {}", path.display(), file.status);
                }
            }
            Err(error) if error.is_fatal() => return Err(error),
            Err(error) => {
                outcome.failed = true;
                report(error);
            }
        }
    }

    Ok(outcome)
}

/// Formats one file, writing it back unless `check` is set
fn format_file<F: SqlFormatter>(
    path: &Path,
    rewriter: &mut Rewriter<F>,
    check: bool,
) -> Result<FileReport, Error> {
    let source = fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;

    let rewrite = rewriter.rewrite(&source).map_err(|error| match error {
        RewriteError::Extract(source) => Error::Parse {
            path: path.to_owned(),
            source,
        },
        RewriteError::Formatter(source) => Error::Formatter(source),
    })?;

    let status = write_back(path, &source, &rewrite.source, check)?;

    let name = path.display().to_string();
    let source: Arc<str> = source.into();
    let failures = rewrite
        .failures
        .into_iter()
        .map(|failure| QueryDiagnostic::new(NamedSource::new(&name, Arc::clone(&source)), failure))
        .collect();

    Ok(FileReport { status, failures })
}

/// Writes `rewritten` to `path` unless it matches `source` or `check` is set
fn write_back(path: &Path, source: &str, rewritten: &str, check: bool) -> Result<Status, Error> {
    if rewritten == source {
        return Ok(Status::Unchanged);
    }
    if check {
        return Ok(Status::WouldChange);
    }
    fs::write(path, rewritten).map_err(|source| Error::Write {
        path: path.to_owned(),
        source,
    })?;
    Ok(Status::Changed)
}

fn report<D: Diagnostic + Send + Sync + 'static>(diagnostic: D) {
    eprintln!("{:?}", Report::new(diagnostic));
}

#[derive(Debug)]
struct FileReport {
    status: Status,
    failures: Vec<QueryDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Unchanged,
    Changed,
    WouldChange,
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unchanged => "ok",
            Self::Changed => "changed",
            Self::WouldChange => "would change",
        })
    }
}

/// What a run found, which decides the exit status
#[derive(Debug, Default)]
struct Outcome {
    failed: bool,
    would_change: bool,
}

impl Outcome {
    const FAILED: Self = Self {
        failed: true,
        would_change: false,
    };
}

impl From<Outcome> for ExitCode {
    /// 2 if anything failed, otherwise 1 if `--check` found changes
    fn from(outcome: Outcome) -> Self {
        if outcome.failed {
            return Self::from(2);
        }
        if outcome.would_change {
            return Self::from(1);
        }
        Self::SUCCESS
    }
}
