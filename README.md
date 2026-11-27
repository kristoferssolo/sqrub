# sqrub

sqrub formats the SQL inside [sqlx](https://github.com/launchbadge/sqlx)
query macros. It finds `query!`, `query_as!`, `query_scalar!` and their
`_unchecked` variants, pipes each SQL string through a formatter of your
choice, and writes the result back as a Rust string literal.

```rust
// before
fn user() {
    let user = sqlx::query_as!(User, "select id, name from users where id = $1", id);
}

// after `sqrub --formatter sleek --min-hashes 1` and `cargo fmt`
fn user() {
    let user = sqlx::query_as!(
        User,
        r#"
        SELECT
            id,
            name
        FROM
            users
        WHERE
            id = $1
        "#,
        id
    );
}
```

## Installation

```sh
cargo install --locked sqrub
```

Prebuilt binaries for Linux and macOS are attached to each
[release](https://github.com/kristoferssolo/sqrub/releases) and work with
`cargo binstall sqrub`.

## Usage

```sh
sqrub --formatter <COMMAND> [OPTIONS] [INPUTS]...
```

`INPUTS` are files or directories and default to `.`. Directories are searched
recursively for `.rs` files, skipping hidden paths and anything excluded by
`.gitignore` or `.ignore`.

The formatter command is split like a shell command line. sqrub writes the SQL
to its stdin and reads the formatted SQL from its stdout, so pass whatever
argument your formatter needs to use stdin:

```sh
sqrub --formatter sleek
sqrub --formatter "pg_format -"
sqrub --formatter "sqlfluff fix --dialect postgres -" --ignore-exit-code
```

sqrub runs the formatter for a file's queries in parallel, one process per CPU
core. If your formatter command cannot run more than once at a time, such as a
wrapper that uses a fixed scratch file or lock, or starting it fails with "Too
many open files", set `RAYON_NUM_THREADS` to run one at a time:

```sh
RAYON_NUM_THREADS=1 sqrub --formatter ./format-sql.sh
```

| Option                        | Effect                                                             |
| ----------------------------- | ------------------------------------------------------------------ |
| `-f`, `--formatter <COMMAND>` | Formatter to run for each query (required)                         |
| `--check`                     | Report files that would change without writing them                |
| `--min-hashes <N>`            | Write raw strings with at least `N` `#`s, such as `r#"..."#` for 1 |
| `--ignore-exit-code`          | Accept formatter output even when it exits non-zero                |
| `-q`, `--quiet`               | Print only errors                                                  |
| `-v`, `--verbose`             | Also list files that are already formatted                         |

Multi-line SQL is written as a raw string, indented for the line rustfmt will
move it to. Run sqrub before `cargo fmt`; after that, neither tool changes the
other's output. Single-line SQL stays a plain string unless it contains `"` or `\`. If
the formatter fails on a query, or prints nothing, that query is left unchanged
and the error is reported with its location.

### Exit status

| Code | Meaning                                 |
| ---- | --------------------------------------- |
| 0    | Every file is formatted                 |
| 1    | `--check` found files that would change |
| 2    | A query, file or the formatter failed   |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

sqrub is a rewrite inspired by
[sqlx-inline-fmt](https://github.com/code-trust/sqlx-inline-fmt).
