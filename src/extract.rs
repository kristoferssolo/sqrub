//! Finding the SQL string literals inside sqlx query macros

use crate::literal::leading_whitespace;
use std::ops::Range;
use thiserror::Error;
use tree_sitter::{LanguageError, Node, Parser};

/// Macros whose first string literal is inline SQL, matched by last path segment
const QUERY_MACROS: [&str; 6] = [
    "query",
    "query_as",
    "query_scalar",
    "query_unchecked",
    "query_as_unchecked",
    "query_scalar_unchecked",
];

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("failed to load the Rust grammar")]
    Language(#[from] LanguageError),
    #[error("tree-sitter could not parse the file")]
    Parse,
}

/// SQL string literal found inside a query macro
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlLiteral<'source> {
    /// Byte range of the literal, including its quotes and `r#` delimiters
    pub span: Range<usize>,
    /// Source text of the literal
    pub text: &'source str,
    /// Whitespace at the start of the line the literal begins on
    pub indent: &'source str,
}

/// Parses Rust source and finds the SQL literals in query macros
pub struct Extractor {
    parser: Parser,
}

impl Extractor {
    pub fn new() -> Result<Self, ExtractError> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        Ok(Self { parser })
    }

    /// Returns the SQL literals in `source`, ordered by position
    ///
    /// Macros containing syntax errors are skipped, so a half-written query is
    /// never rewritten.
    pub fn sql_literals<'source>(
        &mut self,
        source: &'source str,
    ) -> Result<Vec<SqlLiteral<'source>>, ExtractError> {
        let tree = self.parser.parse(source, None).ok_or(ExtractError::Parse)?;

        let mut literals = Vec::new();
        let mut pending = vec![tree.root_node()];
        while let Some(node) = pending.pop() {
            // Token trees hold plain tokens, so no macro invocation nests inside another
            if node.kind() == "macro_invocation" {
                literals.extend(sql_literal(node, source));
                continue;
            }
            pending.extend(node.named_children(&mut node.walk()));
        }

        literals.sort_by_key(|literal| literal.span.start);
        Ok(literals)
    }
}

fn sql_literal<'source>(invocation: Node<'_>, source: &'source str) -> Option<SqlLiteral<'source>> {
    if invocation.has_error() || !is_query_macro(invocation, source) {
        return None;
    }

    let mut cursor = invocation.walk();
    let tokens = invocation
        .named_children(&mut cursor)
        .find(|child| child.kind() == "token_tree")?;

    let mut cursor = tokens.walk();
    let literal = tokens
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "string_literal" | "raw_string_literal"))?;

    let span = literal.byte_range();
    Some(SqlLiteral {
        text: source.get(span.clone())?,
        indent: line_indent(source, span.start),
        span,
    })
}

fn is_query_macro(invocation: Node<'_>, source: &str) -> bool {
    invocation
        .child_by_field_name("macro")
        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
        .and_then(|name| name.rsplit("::").next())
        .is_some_and(|name| QUERY_MACROS.contains(&name.trim()))
}

/// Indentation of the line containing byte `offset`
fn line_indent(source: &str, offset: usize) -> &str {
    let before = source.get(..offset).unwrap_or_default();
    let line = before.rsplit('\n').next().unwrap_or_default();
    leading_whitespace(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::assert_ok;
    use rstest::rstest;

    fn texts(source: &str) -> Vec<&str> {
        let mut extractor = assert_ok!(Extractor::new());
        assert_ok!(extractor.sql_literals(source))
            .into_iter()
            .map(|literal| literal.text)
            .collect()
    }

    #[rstest]
    #[case::scoped(r#"sqlx::query!("SELECT 1")"#)]
    #[case::bare(r#"query!("SELECT 1")"#)]
    #[case::query_as_skips_the_type(r#"sqlx::query_as!(User, "SELECT 1")"#)]
    #[case::scalar_unchecked(r#"sqlx::query_scalar_unchecked!("SELECT 1")"#)]
    #[case::first_literal_only(r#"sqlx::query!("SELECT 1", "bind")"#)]
    fn finds_the_sql_literal(#[case] invocation: &str) {
        let source = format!("fn main() {{ {invocation}; }}");
        assert_eq!(texts(&source), [r#""SELECT 1""#]);
    }

    #[test]
    fn finds_raw_literals() {
        let source = "fn main() { sqlx::query!(r#\"SELECT 1\"#); }";
        assert_eq!(texts(source), ["r#\"SELECT 1\"#"]);
    }

    #[rstest]
    #[case::other_macro(r#"println!("SELECT 1")"#)]
    #[case::query_file(r#"sqlx::query_file!("query.sql")"#)]
    #[case::query_function(r#"sqlx::query("SELECT 1")"#)]
    #[case::syntax_error(r#"sqlx::query!("SELECT 1" ]"#)]
    fn ignores(#[case] invocation: &str) {
        let source = format!("fn main() {{ {invocation}; }}");
        assert_eq!(texts(&source), Vec::<&str>::new());
    }

    #[test]
    fn records_span_and_line_indent() {
        let source = "fn main() {\n    let row = sqlx::query!(\"SELECT 1\");\n}\n";
        let mut extractor = assert_ok!(Extractor::new());
        let literals = assert_ok!(extractor.sql_literals(source));

        let start = source.find('"').unwrap();
        let end = source.find(");").unwrap();
        assert_eq!(
            literals,
            [SqlLiteral {
                span: start..end,
                text: "\"SELECT 1\"",
                indent: "    ",
            }]
        );
    }

    #[test]
    fn orders_literals_by_position() {
        let source = r#"
            fn a() { sqlx::query!("SELECT 1"); }
            fn b() { sqlx::query!("SELECT 2"); sqlx::query!("SELECT 3"); }
        "#;
        assert_eq!(
            texts(source),
            [r#""SELECT 1""#, r#""SELECT 2""#, r#""SELECT 3""#]
        );
    }
}
