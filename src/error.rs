//! Errors reported to the user

use crate::extract::ExtractError;
use crate::formatter::FormatterError;
use crate::rewrite::{QueryError, QueryFailure};
use miette::{Diagnostic, NamedSource, SourceSpan};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error("failed to load the Rust grammar")]
    Grammar(#[source] ExtractError),
    #[error("failed to run the formatter")]
    #[diagnostic(help("check that the command is installed and on PATH"))]
    Formatter(#[source] FormatterError),
    #[error("failed to find Rust files")]
    Walk(#[from] ignore::Error),
    #[error("failed to read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse {}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: ExtractError,
    },
}

impl Error {
    /// Whether the error stops the whole run instead of only one file
    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Grammar(_) | Self::Formatter(_))
    }
}

/// Query that could not be formatted, shown with its source location
#[derive(Debug, Error, Diagnostic)]
#[error("failed to format query")]
pub struct QueryDiagnostic {
    #[source_code]
    source_code: NamedSource<Arc<str>>,
    #[label("this query")]
    span: SourceSpan,
    #[source]
    error: QueryError,
    /// Formatter stderr, which usually explains a failure
    #[help]
    stderr: Option<String>,
}

impl QueryDiagnostic {
    pub fn new(source_code: NamedSource<Arc<str>>, failure: QueryFailure) -> Self {
        let stderr = match &failure.error {
            QueryError::Format(FormatterError::Failed { stderr, .. }) => {
                Some(stderr.trim_end().to_owned()).filter(|stderr| !stderr.is_empty())
            }
            _ => None,
        };
        Self {
            source_code,
            span: failure.span.into(),
            error: failure.error,
            stderr,
        }
    }
}
