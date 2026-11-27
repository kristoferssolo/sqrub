//! Converting between Rust string literals and the SQL they hold

use litrs::{ParseError, StringLit};
use thiserror::Error;

/// Rust rejects raw string literals delimited by more `#`s than this
const MAX_HASHES: usize = 255;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LiteralError {
    #[error("SQL contains a carriage return outside a CRLF line ending")]
    BareCarriageReturn,
    #[error("SQL needs {needed} `#`s around a raw string, but Rust allows at most {MAX_HASHES}")]
    TooManyHashes { needed: usize },
}

/// Extracts the SQL from a string literal's source text, ready for a formatter
///
/// Drops the newline after the opening quote, the whitespace before the
/// closing quote, and the indentation every line shares, so the formatter sees
/// the SQL as if it started at the beginning of a file.
pub fn decode(source: &str) -> Result<String, ParseError> {
    // rustc reads CRLF as LF, but litrs rejects any carriage return
    let literal = StringLit::parse(source.replace("\r\n", "\n"))?;
    let value = literal.value();
    let value = value.strip_prefix('\n').unwrap_or(value);
    let value = match value.rsplit_once('\n') {
        Some((body, last)) if is_blank(last) => body,
        _ => value,
    };
    Ok(dedent(value))
}

/// Leading spaces and tabs of `line`
pub fn leading_whitespace(line: &str) -> &str {
    let rest = line.trim_start_matches([' ', '\t']);
    line.strip_suffix(rest).unwrap_or_default()
}

fn is_blank(line: &str) -> bool {
    line.trim_start_matches([' ', '\t']).is_empty()
}

/// Strips the indentation shared by every non-blank line and empties blank ones
fn dedent(sql: &str) -> String {
    let indent = sql
        .lines()
        .filter(|line| !is_blank(line))
        .map(leading_whitespace)
        .reduce(common_prefix)
        .unwrap_or_default();

    sql.lines()
        .map(|line| {
            if is_blank(line) {
                ""
            } else {
                line.strip_prefix(indent).unwrap_or(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn common_prefix<'a>(left: &'a str, right: &str) -> &'a str {
    let shared = left
        .bytes()
        .zip(right.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    left.get(..shared).unwrap_or_default()
}

/// Renders formatted SQL as a Rust string literal
///
/// Multi-line SQL becomes a raw string with each line, and the closing quote,
/// prefixed by `indent`. Single-line SQL stays a plain string unless it
/// contains `"` or `\`. Raw strings get at least `min_hashes` `#`s.
pub fn render(sql: &str, indent: &str, min_hashes: u8) -> Result<String, LiteralError> {
    let normalized = sql.replace("\r\n", "\n");
    if normalized.contains('\r') {
        return Err(LiteralError::BareCarriageReturn);
    }

    let sql = normalized.trim_matches('\n');
    let is_multi_line = sql.contains('\n');
    if !is_multi_line && !sql.contains(['"', '\\']) {
        return Ok(format!("\"{sql}\""));
    }

    let hashes = "#".repeat(raw_hashes(sql, min_hashes)?);
    if !is_multi_line {
        return Ok(format!("r{hashes}\"{sql}\"{hashes}"));
    }

    let body = indent_lines(sql, indent);
    Ok(format!("r{hashes}\"\n{body}\n{indent}\"{hashes}"))
}

fn raw_hashes(sql: &str, min_hashes: u8) -> Result<usize, LiteralError> {
    let needed = required_hashes(sql).max(usize::from(min_hashes));
    if needed > MAX_HASHES {
        return Err(LiteralError::TooManyHashes { needed });
    }
    Ok(needed)
}

/// Fewest `#`s that keep any `"` in `sql` from closing the raw string
fn required_hashes(sql: &str) -> usize {
    sql.split('"')
        .skip(1)
        .map(|rest| {
            rest.bytes()
                .take_while(|&byte| byte == b'#')
                .count()
                .saturating_add(1)
        })
        .max()
        .unwrap_or(0)
}

/// Prefixes every non-empty line, so blank lines carry no trailing whitespace
fn indent_lines(sql: &str, indent: &str) -> String {
    sql.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::{assert_err, assert_err_eq, assert_ok_eq};
    use rstest::rstest;

    #[rstest]
    #[case::plain(r#""SELECT 1""#, "SELECT 1")]
    #[case::indented_raw_block("r#\"\n        SELECT\n            1\n        \"#", "SELECT\n    1")]
    #[case::first_line_after_quote("\"SELECT\n    1\"", "SELECT\n    1")]
    #[case::blank_lines_emptied("r\"\n    SELECT\n  \n    1\n    \"", "SELECT\n\n1")]
    #[case::tab_indent("r\"\n\t\tSELECT\n\t\t\t1\n\t\t\"", "SELECT\n\t1")]
    #[case::escapes(r#""SELECT '\\d'""#, r"SELECT '\d'")]
    #[case::line_continuation("\"SELECT \\\n    1\"", "SELECT 1")]
    #[case::crlf("r\"\r\n    SELECT\r\n      1\r\n    \"", "SELECT\n  1")]
    fn decodes(#[case] source: &str, #[case] expected: &str) {
        assert_ok_eq!(decode(source), expected);
    }

    #[test]
    fn rejects_byte_strings() {
        assert_err!(decode(r#"b"SELECT 1""#));
    }

    #[rstest]
    #[case::multi_line("r#\"\n    SELECT\n      1\n    \"#", 1)]
    #[case::plain(r#""SELECT 1""#, 0)]
    #[case::raw_single_line(r#"r"SELECT '\d'""#, 0)]
    fn decode_then_render_round_trips(#[case] source: &str, #[case] min_hashes: u8) {
        let sql = decode(source).unwrap();
        assert_ok_eq!(render(&sql, "    ", min_hashes), source);
    }

    #[rstest]
    #[case::plain_single_line("SELECT 1\n", 1, r#""SELECT 1""#)]
    #[case::quote_needs_raw(r#"SELECT '"'"#, 0, r##"r#"SELECT '"'"#"##)]
    #[case::backslash_needs_raw(r"SELECT '\d'", 0, r#"r"SELECT '\d'""#)]
    #[case::min_hashes_on_single_line_raw(r"SELECT '\d'", 2, r###"r##"SELECT '\d'"##"###)]
    fn renders_single_line(#[case] sql: &str, #[case] min_hashes: u8, #[case] expected: &str) {
        assert_ok_eq!(render(sql, "    ", min_hashes), expected);
    }

    #[rstest]
    #[case::fewest_hashes("SELECT\n  1\n", 0, "r\"\n    SELECT\n      1\n    \"")]
    #[case::min_hashes("SELECT\n  1\n", 1, "r#\"\n    SELECT\n      1\n    \"#")]
    #[case::sql_needs_more_than_min(
        "SELECT\n  '\"#'",
        1,
        "r##\"\n    SELECT\n      '\"#'\n    \"##"
    )]
    #[case::blank_lines_stay_empty("SELECT\n\n  1", 0, "r\"\n    SELECT\n\n      1\n    \"")]
    #[case::crlf_line_endings("SELECT\r\n  1\r\n", 0, "r\"\n    SELECT\n      1\n    \"")]
    #[case::surrounding_newlines("\n\nSELECT\n  1\n\n", 0, "r\"\n    SELECT\n      1\n    \"")]
    fn renders_multi_line(#[case] sql: &str, #[case] min_hashes: u8, #[case] expected: &str) {
        assert_ok_eq!(render(sql, "    ", min_hashes), expected);
    }

    #[test]
    fn rejects_bare_carriage_return() {
        assert_err_eq!(render("SELECT\r1", "", 0), LiteralError::BareCarriageReturn);
    }

    #[test]
    fn rejects_sql_that_needs_more_than_255_hashes() {
        let sql = format!("SELECT '\"{}'", "#".repeat(255));
        assert_err_eq!(
            render(&sql, "", 0),
            LiteralError::TooManyHashes { needed: 256 }
        );
    }

    #[test]
    fn accepts_255_hashes() {
        let sql = format!("SELECT '\"{}'", "#".repeat(254));
        let hashes = "#".repeat(255);
        assert_ok_eq!(render(&sql, "", 0), format!("r{hashes}\"{sql}\"{hashes}"));
    }
}
