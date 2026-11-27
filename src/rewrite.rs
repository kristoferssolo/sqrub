//! Formatting every SQL literal in one Rust source file

use crate::{
    extract::{ExtractError, Extractor, SqlLiteral},
    formatter::{FormatterError, SqlFormatter},
    literal::{self, LiteralError},
};
use litrs::ParseError;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
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
    /// # Errors
    ///
    /// Returns an error if the Rust grammar fails to load
    pub fn new(formatter: F, min_hashes: u8) -> Result<Self, ExtractError> {
        Ok(Self {
            extractor: Extractor::new()?,
            formatter,
            min_hashes,
        })
    }

    /// Formats every query in `source`, leaving the ones that fail unchanged
    ///
    /// # Errors
    ///
    /// Returns an error if `source` cannot be parsed or the formatter cannot be
    /// started
    pub fn rewrite(&mut self, source: &str) -> Result<Rewrite, RewriteError> {
        let literals = self.extractor.sql_literals(source)?;
        let (formatter, min_hashes) = (&self.formatter, self.min_hashes);
        // Each query runs its own formatter process, so they run in parallel. A
        // formatter that cannot start fails every query, so it stops the rest
        let results = literals
            .par_iter()
            .map(
                |literal| match format_literal(formatter, min_hashes, literal) {
                    Err(QueryError::Format(error @ FormatterError::Spawn { .. })) => Err(error),
                    result => Ok(result),
                },
            )
            .collect::<Result<Vec<_>, _>>()
            .map_err(RewriteError::Formatter)?;

        let mut output = source.to_owned();
        let mut failures = Vec::new();

        // Replacing from the end keeps the spans of earlier literals valid
        for (literal, result) in literals.iter().zip(results).rev() {
            match result {
                Ok(Some(text)) => output.replace_range(literal.span.clone(), &text),
                Ok(None) => {}
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
}

/// Returns the replacement literal, or `None` when it would not change
fn format_literal<F: SqlFormatter>(
    sql_formatter: &F,
    min_hashes: u8,
    literal: &SqlLiteral<'_>,
) -> Result<Option<String>, QueryError> {
    let sql = literal::decode(literal.text)?;
    if sql.trim().is_empty() {
        return Ok(None);
    }

    let formatted = sql_formatter.format(&sql)?;
    let rendered = literal::render(&formatted, &literal.indent, min_hashes)?;
    Ok((rendered != literal.text).then_some(rendered))
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::{assert_matches, assert_ok, assert_some};
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
    fn reports_failures_in_source_order() {
        let queries = (0..25)
            .map(|index| format!(r#"sqlx::query!("bad{index}"); sqlx::query!("good{index}");"#))
            .collect::<Vec<_>>()
            .join("\n    ");
        let source = format!("fn main() {{\n    {queries}\n}}");
        let formatter = |sql: &str| {
            if sql.starts_with("bad") {
                return Err(FormatterError::EmptyOutput);
            }
            uppercase(sql)
        };
        let rewrite = assert_ok!(rewrite(formatter, &source));

        let failed = rewrite
            .failures
            .iter()
            .map(|failure| assert_some!(source.get(failure.span.clone())))
            .collect::<Vec<_>>();
        let expected = (0..25)
            .map(|index| format!(r#""bad{index}""#))
            .collect::<Vec<_>>();
        assert_eq!(failed, expected);
        assert!(!rewrite.source.contains("\"good"));
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
