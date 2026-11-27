//! Formats the SQL inside sqlx query macros with an external formatter

mod error;
mod extract;
mod files;
mod formatter;
mod literal;
mod rewrite;

pub use crate::{
    error::{Error, QueryDiagnostic},
    extract::ExtractError,
    files::rust_files,
    formatter::{CommandParseError, ExternalFormatter, FormatterError, ShellCommand, SqlFormatter},
    rewrite::{QueryError, QueryFailure, Rewrite, RewriteError, Rewriter},
};
