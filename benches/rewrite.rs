//! Rewrite throughput, with and without spawning a formatter process
#![expect(
    clippy::expect_used,
    reason = "a benchmark cannot run without its setup"
)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use sqrub::{ExternalFormatter, FormatterError, Rewrite, Rewriter, SqlFormatter};
use std::{fmt::Write as _, hint::black_box};

/// Source whose macros are not sqlx queries, so only parsing and matching run
fn source_without_queries(functions: usize) -> String {
    (0..functions).fold(String::new(), |mut source, index| {
        writeln!(
            source,
            r#"
fn handler_{index}(id: i64) -> String {{
    let label = format!("SELECT id FROM users WHERE id = {{id}}");
    println!("{{label}}");
    label
}}"#
        )
        .expect("writing to a String cannot fail");
        source
    })
}

/// Source with one raw literal per query, each of which the formatter rewrites
/// to a regular string literal
fn source_with_queries(queries: usize) -> String {
    (0..queries).fold(String::new(), |mut source, index| {
        writeln!(
            source,
            r##"
async fn fetch_user_{index}(pool: &PgPool, id: i64) -> Result<User> {{
    sqlx::query_as!(User, r#"SELECT id, name FROM users WHERE id = $1"#, id)
        .fetch_one(pool)
        .await
}}"##
        )
        .expect("writing to a String cannot fail");
        source
    })
}

fn rewriter<F: SqlFormatter>(formatter: F) -> Rewriter<F> {
    Rewriter::new(formatter, 0).expect("the Rust grammar should load")
}

fn rewrite<F: SqlFormatter>(rewriter: &mut Rewriter<F>, source: &str) -> Rewrite {
    rewriter.rewrite(source).expect("rewriting should succeed")
}

/// Formats in-process, so no process is spawned
#[expect(
    clippy::unnecessary_wraps,
    reason = "matches the SqlFormatter closure signature"
)]
fn identity(sql: &str) -> Result<String, FormatterError> {
    Ok(sql.to_owned())
}

fn bytes(source: &str) -> Throughput {
    Throughput::Bytes(source.len().try_into().expect("source length fits in u64"))
}

/// Parsing and finding query macros only
fn extract(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("extract");
    let mut rewriter = rewriter(identity);

    for functions in [100, 1_000, 10_000] {
        let source = source_without_queries(functions);
        assert_eq!(rewrite(&mut rewriter, &source).source, source);

        group.throughput(bytes(&source));
        group.bench_with_input(
            BenchmarkId::from_parameter(functions),
            source.as_str(),
            |bencher, source| bencher.iter(|| rewrite(&mut rewriter, black_box(source))),
        );
    }

    group.finish();
}

/// Extracting, decoding and rendering every query without a formatter process
fn in_process(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("in_process");
    let mut rewriter = rewriter(identity);

    for queries in [10, 100, 1_000] {
        let source = source_with_queries(queries);
        assert!(!rewrite(&mut rewriter, &source).source.contains("r#\""));

        group.throughput(bytes(&source));
        group.bench_with_input(
            BenchmarkId::from_parameter(queries),
            source.as_str(),
            |bencher, source| bencher.iter(|| rewrite(&mut rewriter, black_box(source))),
        );
    }

    group.finish();
}

/// The full pipeline, dominated by one formatter process per query
fn external(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("external");
    group.sample_size(10);
    let command = "cat".parse().expect("`cat` is a valid command");
    let mut rewriter = rewriter(ExternalFormatter::new(command, false));

    for queries in [1, 10, 100] {
        let source = source_with_queries(queries);
        assert!(!rewrite(&mut rewriter, &source).source.contains("r#\""));

        group.throughput(Throughput::Elements(
            queries.try_into().expect("query count fits in u64"),
        ));
        group.bench_with_input(
            BenchmarkId::from_parameter(queries),
            source.as_str(),
            |bencher, source| bencher.iter(|| rewrite(&mut rewriter, black_box(source))),
        );
    }

    group.finish();
}

criterion_group!(benches, extract, in_process, external);
criterion_main!(benches);
