//! Finding the SQL string literals inside sqlx query macros

use crate::literal::leading_whitespace;
use std::borrow::Cow;
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
    /// Indentation for the lines of the literal when it spans several, see
    /// [`literal_indent`]
    pub indent: Cow<'source, str>,
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
        indent: literal_indent(source, span.start, tokens.start_byte()),
        span,
    })
}

/// Indentation of a multi-line literal once rustfmt has laid out its macro call
///
/// rustfmt moves a multi-line string argument onto its own line, one level
/// deeper than the line that opens the call, and never changes the string's
/// contents. Indenting for that position keeps sqrub and rustfmt from undoing
/// each other. A literal that already starts its line keeps that line's
/// indentation.
fn literal_indent(source: &str, literal_start: usize, call_start: usize) -> Cow<'_, str> {
    let line = line_before(source, literal_start);
    let indent = leading_whitespace(line);
    if indent == line {
        return Cow::Borrowed(indent);
    }

    let call_indent = leading_whitespace(line_before(source, call_start));
    let level = if call_indent.contains('\t') {
        "\t"
    } else {
        "    "
    };
    Cow::Owned(format!("{call_indent}{level}"))
}

fn is_query_macro(invocation: Node<'_>, source: &str) -> bool {
    invocation
        .child_by_field_name("macro")
        .and_then(|name| name.utf8_text(source.as_bytes()).ok())
        .and_then(|name| name.rsplit("::").next())
        .is_some_and(|name| QUERY_MACROS.contains(&name.trim()))
}

/// Text from the start of the line containing byte `offset` up to `offset`
fn line_before(source: &str, offset: usize) -> &str {
    let before = source.get(..offset).unwrap_or_default();
    before.rsplit('\n').next().unwrap_or_default()
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
    fn records_span_and_text() {
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
                indent: Cow::Owned("        ".to_owned()),
            }]
        );
    }

    #[rstest]
    #[case::alone_on_its_line("    sqlx::query!(\n        \"SELECT 1\"\n    );", "        ")]
    #[case::after_the_call("    sqlx::query!(\"SELECT 1\");", "        ")]
    #[case::after_other_arguments(
        "    sqlx::query_as!(\n        User, \"SELECT 1\"\n    );",
        "        "
    )]
    #[case::in_a_let("    let row = sqlx::query!(\"SELECT 1\", id);", "        ")]
    #[case::tabs("\tsqlx::query!(\"SELECT 1\");", "\t\t")]
    #[case::top_level("sqlx::query!(\"SELECT 1\");", "    ")]
    fn indents_where_rustfmt_places_the_literal(#[case] statement: &str, #[case] expected: &str) {
        let source = format!("fn main() {{\n{statement}\n}}\n");
        let mut extractor = assert_ok!(Extractor::new());
        let literals = assert_ok!(extractor.sql_literals(&source));
        let indents: Vec<&str> = literals
            .iter()
            .map(|literal| literal.indent.as_ref())
            .collect();
        assert_eq!(indents, [expected]);
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
