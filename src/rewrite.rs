//! Formatting every SQL literal in one Rust source file

use crate::extract::{ExtractError, Extractor, SqlLiteral};
use crate::formatter::{FormatterError, SqlFormatter};
use crate::literal::{self, LiteralError};
use litrs::ParseError;
use std::ops::Range;
use thiserror::Error;

/// Error that stops a whole file from being formatted
#[derive(Debug, Error)]
pub enum RewriteError {
    #[error(transparent)]
    Extract(#[from] ExtractError),
    /// The formatter could not be started, so no other query can succeed either
    #[error(transparent)]
    Formatter(FormatterError),
}

/// Error that leaves one query unchanged
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("invalid string literal")]
    Decode(#[from] ParseError),
    #[error(transparent)]
    Format(#[from] FormatterError),
    #[error(transparent)]
    Render(#[from] LiteralError),
}

/// Query left unchanged because formatting it failed
#[derive(Debug)]
pub struct QueryFailure {
    /// Byte range of the query's literal in the original source
    pub span: Range<usize>,
    pub error: QueryError,
}

/// Source with every query that could be formatted rewritten
#[derive(Debug)]
pub struct Rewrite {
    pub source: String,
    /// Failures in source order
    pub failures: Vec<QueryFailure>,
}

/// Rewrites the SQL literals in Rust source using a formatter `F`
pub struct Rewriter<F> {
    extractor: Extractor,
    formatter: F,
    min_hashes: u8,
}

impl<F: SqlFormatter> Rewriter<F> {
    pub fn new(formatter: F, min_hashes: u8) -> Result<Self, ExtractError> {
        Ok(Self {
            extractor: Extractor::new()?,
            formatter,
            min_hashes,
        })
    }

    pub fn rewrite(&mut self, source: &str) -> Result<Rewrite, RewriteError> {
        let literals = self.extractor.sql_literals(source)?;
        let mut output = source.to_owned();
        let mut failures = Vec::new();

        // Replacing from the end keeps the spans of earlier literals valid
        for literal in literals.iter().rev() {
            match self.format_literal(literal) {
                Ok(Some(text)) => output.replace_range(literal.span.clone(), &text),
                Ok(None) => {}
                Err(QueryError::Format(error @ FormatterError::Spawn { .. })) => {
                    return Err(RewriteError::Formatter(error));
                }
                Err(error) => failures.push(QueryFailure {
                    span: literal.span.clone(),
                    error,
                }),
            }
        }

        failures.reverse();
        Ok(Rewrite {
            source: output,
            failures,
        })
    }

    /// Returns the replacement literal, or `None` when it would not change
    fn format_literal(&self, literal: &SqlLiteral<'_>) -> Result<Option<String>, QueryError> {
        let sql = literal::decode(literal.text)?;
        if sql.trim().is_empty() {
            return Ok(None);
        }

        let formatted = self.formatter.format(&sql)?;
        let rendered = literal::render(&formatted, &literal.indent, self.min_hashes)?;
        Ok((rendered != literal.text).then_some(rendered))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::{assert_matches, assert_ok};
    use std::io;

    #[expect(
        clippy::unnecessary_wraps,
        reason = "matches the SqlFormatter closure signature"
    )]
    fn uppercase(sql: &str) -> Result<String, FormatterError> {
        Ok(sql.to_uppercase())
    }

    fn rewrite<F: SqlFormatter>(formatter: F, source: &str) -> Result<Rewrite, RewriteError> {
        assert_ok!(Rewriter::new(formatter, 1)).rewrite(source)
    }

    #[test]
    fn rewrites_every_query() {
        let source = indoc(
            r#"
            fn main() {
                sqlx::query!("select 1");
                sqlx::query_as!(User, "select\n    id\nfrom users");
            }
            "#,
        );
        let rewrite = assert_ok!(rewrite(uppercase, &source));

        assert_eq!(
            rewrite.source,
            indoc(
                r##"
                fn main() {
                    sqlx::query!("SELECT 1");
                    sqlx::query_as!(User, r#"
                        SELECT
                            ID
                        FROM USERS
                        "#);
                }
                "##
            )
        );
        assert_matches!(rewrite.failures.as_slice(), []);
    }

    #[test]
    fn leaves_formatted_source_unchanged() {
        let source = "fn main() { sqlx::query!(\"SELECT 1\"); }";
        assert_eq!(assert_ok!(rewrite(uppercase, source)).source, source);
    }

    #[test]
    fn keeps_failed_queries_and_formats_the_rest() {
        let source = r#"fn main() { sqlx::query!("bad"); sqlx::query!("good"); }"#;
        let formatter = |sql: &str| {
            if sql == "bad" {
                Err(FormatterError::EmptyOutput)
            } else {
                uppercase(sql)
            }
        };
        let rewrite = assert_ok!(rewrite(formatter, source));

        assert_eq!(
            rewrite.source,
            r#"fn main() { sqlx::query!("bad"); sqlx::query!("GOOD"); }"#
        );
        let start = source.find(r#""bad""#).unwrap();
        let end = source.find("); sqlx").unwrap();
        assert_matches!(
            rewrite.failures.as_slice(),
            [QueryFailure { span, error: QueryError::Format(FormatterError::EmptyOutput) }]
                if *span == (start..end)
        );
    }

    #[test]
    fn skips_empty_queries() {
        let source = r#"fn main() { sqlx::query!(""); }"#;
        let formatter = |_: &str| Err(FormatterError::EmptyOutput);
        let rewrite = assert_ok!(rewrite(formatter, source));
        assert_eq!(rewrite.source, source);
        assert_matches!(rewrite.failures.as_slice(), []);
    }

    #[test]
    fn stops_when_the_formatter_cannot_start() {
        let source = r#"fn main() { sqlx::query!("select 1"); }"#;
        let formatter = |_: &str| {
            Err(FormatterError::Spawn {
                program: "missing".to_owned(),
                source: io::ErrorKind::NotFound.into(),
            })
        };
        assert_matches!(
            rewrite(formatter, source),
            Err(RewriteError::Formatter(FormatterError::Spawn { .. }))
        );
    }

    /// Strips the leading newline and the first line's indentation from every line
    fn indoc(text: &str) -> String {
        let text = text.strip_prefix('\n').unwrap();
        let indent = literal::leading_whitespace(text);
        text.lines()
            .map(|line| line.strip_prefix(indent).unwrap_or(line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
