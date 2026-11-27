//! Running the SQL formatter

use std::{
    io::{self, Write as _},
    panic,
    process::{Command, ExitStatus, Stdio},
    str::FromStr,
    string::FromUtf8Error,
    thread,
};
use thiserror::Error;

/// Something that formats SQL text
pub trait SqlFormatter {
    /// # Errors
    ///
    /// Returns an error if the SQL cannot be formatted
    fn format(&self, sql: &str) -> Result<String, FormatterError>;
}

impl<F> SqlFormatter for F
where
    F: Fn(&str) -> Result<String, FormatterError>,
{
    fn format(&self, sql: &str) -> Result<String, FormatterError> {
        self(sql)
    }
}

#[derive(Debug, Error)]
pub enum FormatterError {
    #[error("failed to start `{program}`")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to communicate with the formatter")]
    Io(#[from] io::Error),
    #[error("formatter failed with {status}")]
    Failed { status: ExitStatus, stderr: String },
    #[error("formatter output is not valid UTF-8")]
    NonUtf8(#[from] FromUtf8Error),
    #[error("formatter printed nothing; it must write the formatted SQL to stdout")]
    EmptyOutput,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CommandParseError {
    #[error("unbalanced quotes or a trailing backslash")]
    Syntax,
    #[error("the command is empty")]
    Empty,
}

/// Command line split into words the way a POSIX shell would
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    program: String,
    args: Vec<String>,
}

impl FromStr for ShellCommand {
    type Err = CommandParseError;

    fn from_str(command: &str) -> Result<Self, Self::Err> {
        let mut words = shlex::split(command)
            .ok_or(CommandParseError::Syntax)?
            .into_iter();
        let program = words.next().ok_or(CommandParseError::Empty)?;
        Ok(Self {
            program,
            args: words.collect(),
        })
    }
}

/// Formats SQL by piping it through an external command's stdin and stdout
#[derive(Debug, Clone)]
pub struct ExternalFormatter {
    command: ShellCommand,
    ignore_exit_code: bool,
}

impl ExternalFormatter {
    /// With `ignore_exit_code`, output from a command that exits non-zero is
    /// still accepted, for formatters that report unfixable lint violations
    /// that way
    #[must_use]
    pub const fn new(command: ShellCommand, ignore_exit_code: bool) -> Self {
        Self {
            command,
            ignore_exit_code,
        }
    }
}

impl SqlFormatter for ExternalFormatter {
    fn format(&self, sql: &str) -> Result<String, FormatterError> {
        let ShellCommand { program, args } = &self.command;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| FormatterError::Spawn {
                program: program.clone(),
                source,
            })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("formatter stdin was not captured"))?;
        let input = format!("{sql}\n");

        // Writing on a separate thread keeps a formatter that writes output
        // before reading all its input from deadlocking on a full pipe
        let (written, output) = thread::scope(|scope| {
            let writer = scope.spawn(move || stdin.write_all(input.as_bytes()));
            let output = child.wait_with_output();
            let written = writer
                .join()
                .unwrap_or_else(|payload| panic::resume_unwind(payload));
            (written, output)
        });
        let output = output?;

        // A formatter may exit without reading stdin; its status explains why
        if let Err(error) = written
            && error.kind() != io::ErrorKind::BrokenPipe
        {
            return Err(error.into());
        }

        if !output.status.success() && !self.ignore_exit_code {
            return Err(FormatterError::Failed {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let formatted = String::from_utf8(output.stdout)?;
        if formatted.trim().is_empty() {
            return Err(FormatterError::EmptyOutput);
        }
        Ok(formatted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claims::{assert_err_eq, assert_matches, assert_ok, assert_ok_eq};
    use rstest::rstest;

    fn formatter(command: &str, ignore_exit_code: bool) -> ExternalFormatter {
        ExternalFormatter::new(assert_ok!(command.parse()), ignore_exit_code)
    }

    #[test]
    fn parses_like_a_shell() {
        assert_ok_eq!(
            "sqlfluff fix --dialect 'postgres' -".parse::<ShellCommand>(),
            ShellCommand {
                program: "sqlfluff".to_owned(),
                args: ["fix", "--dialect", "postgres", "-"]
                    .map(str::to_owned)
                    .into(),
            }
        );
    }

    #[rstest]
    #[case::empty("  ", CommandParseError::Empty)]
    #[case::unbalanced_quote("pg_format '", CommandParseError::Syntax)]
    fn rejects_bad_commands(#[case] command: &str, #[case] expected: CommandParseError) {
        assert_err_eq!(command.parse::<ShellCommand>(), expected);
    }

    #[test]
    fn pipes_sql_through_the_command() {
        assert_ok_eq!(formatter("cat", false).format("SELECT 1"), "SELECT 1\n");
    }

    #[test]
    fn handles_input_larger_than_a_pipe_buffer() {
        let sql = "SELECT 1;\n".repeat(100_000);
        let formatted = assert_ok!(formatter("cat", false).format(sql.trim_end()));
        assert_eq!(formatted, sql);
    }

    #[test]
    fn reports_failure_with_stderr() {
        let result =
            formatter("sh -c 'cat >/dev/null; echo oops >&2; exit 3'", false).format("SELECT 1");
        assert_matches!(
            result,
            Err(FormatterError::Failed { status, stderr })
                if status.code() == Some(3) && stderr == "oops\n"
        );
    }

    #[test]
    fn accepts_failing_output_when_ignoring_exit_code() {
        let result = formatter("sh -c 'cat; exit 1'", true).format("SELECT 1");
        assert_ok_eq!(result, "SELECT 1\n");
    }

    #[test]
    fn rejects_empty_output() {
        // `true` exits without reading stdin, so this also covers the broken pipe
        let result = formatter("true", false).format("SELECT 1");
        assert_matches!(result, Err(FormatterError::EmptyOutput));
    }

    #[test]
    fn reports_missing_program() {
        let result = formatter("sqrub-test-missing-formatter", false).format("SELECT 1");
        assert_matches!(result, Err(FormatterError::Spawn { program, .. }) if program == "sqrub-test-missing-formatter");
    }
}
