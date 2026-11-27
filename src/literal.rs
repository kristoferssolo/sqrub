//! Writing SQL back into Rust string literals

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
    use claims::{assert_err_eq, assert_ok_eq};
    use rstest::rstest;

    use super::*;

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
