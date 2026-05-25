//! Embucket SQL Logic Test harness binary.
//!
//! Discovers `.slt` files under `tests/slt/`, pre-processes embucket-specific
//! directives, then runs each file against a fresh in-memory rustice session
//! via the upstream `sqllogictest` `Runner`.
//!
//! Failure mode is soft by default: errors are aggregated into a per-directory
//! summary and the process exits 0 unless `--strict` is passed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use clap::Parser;
use embucket_sqllogictest::embucket_validator;
use embucket_sqllogictest::engine::EmbucketSession;
use embucket_sqllogictest::preprocessor::strip_custom_directives;
use executor::test_helpers::create_df_session_with_catalog_url;
use futures::StreamExt;
use sqllogictest::{Runner, parse_with_name};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "sqllogictests", about = "Embucket SQL Logic Test harness")]
struct Cli {
    /// Substring filters; a file path must contain at least one to run.
    filters: Vec<String>,

    /// Parallel file executions. Defaults to logical CPU count.
    #[arg(long, default_value_t = default_threads())]
    test_threads: usize,

    /// Exit non-zero if any file fails (default: always exit 0).
    #[arg(long)]
    strict: bool,

    /// Also run files under `tests/slt/databend/`. Excluded by default.
    #[arg(long)]
    include_databend: bool,

    /// List the files that would run and exit.
    #[arg(long)]
    list: bool,
}

fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
}

const ERRS_PER_FILE_LIMIT: usize = 10;

#[derive(Debug)]
struct FileOutcome {
    path: PathBuf,
    errors: Vec<String>,
    duration_ms: u128,
}

fn main() {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let exit_code = runtime.block_on(async move { run(cli).await });
    std::process::exit(exit_code);
}

async fn run(cli: Cli) -> i32 {
    let slt_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/slt");
    let files = collect_files(&slt_root, &cli);

    if files.is_empty() {
        eprintln!("no .slt files matched (root: {})", slt_root.display());
        return 0;
    }

    if cli.list {
        for f in &files {
            eprintln!("{}", f.display());
        }
        return 0;
    }

    eprintln!(
        "running {} .slt file(s) with {} thread(s)",
        files.len(),
        cli.test_threads
    );

    let outcomes: Vec<FileOutcome> = futures::stream::iter(files)
        .map(|path| async move { run_file(path).await })
        .buffer_unordered(cli.test_threads)
        .collect()
        .await;

    print_summary(&slt_root, &outcomes);

    let failures = outcomes.iter().filter(|o| !o.errors.is_empty()).count();
    if cli.strict && failures > 0 { 1 } else { 0 }
}

fn collect_files(root: &Path, cli: &Cli) -> Vec<PathBuf> {
    let mut acc = Vec::new();
    if root.exists() {
        walk(root, &mut acc);
    }
    acc.retain(|p| p.extension().is_some_and(|e| e == "slt"));
    if !cli.include_databend {
        acc.retain(|p| !p.components().any(|c| c.as_os_str() == "databend"));
    }
    if !cli.filters.is_empty() {
        acc.retain(|p| {
            let s = p.to_string_lossy();
            cli.filters.iter().any(|f| s.contains(f))
        });
    }
    acc.sort();
    acc
}

fn walk(dir: &Path, acc: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, acc);
        } else {
            acc.push(path);
        }
    }
}

async fn run_file(path: PathBuf) -> FileOutcome {
    let start = Instant::now();
    let mut errors = Vec::new();

    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return FileOutcome {
                path,
                errors: vec![format!("read error: {e}")],
                duration_ms: start.elapsed().as_millis(),
            };
        }
    };

    let cleaned = strip_custom_directives(&raw);
    let file_name = path.to_string_lossy().to_string();

    let records = match parse_with_name::<embucket_sqllogictest::output::DFColumnType>(
        &cleaned,
        file_name.as_str(),
    ) {
        Ok(r) => r,
        Err(e) => {
            return FileOutcome {
                path,
                errors: vec![format!("parse error: {e}")],
                duration_ms: start.elapsed().as_millis(),
            };
        }
    };

    let session = create_df_session_with_catalog_url("/dev").await;
    let make_session = move || {
        let session = Arc::clone(&session);
        async move {
            Ok::<_, embucket_sqllogictest::error::Error>(EmbucketSession::new(session))
        }
    };

    let mut runner = Runner::new(make_session);
    runner.add_label("embucket");
    runner.with_validator(embucket_validator);

    for record in records {
        if errors.len() >= ERRS_PER_FILE_LIMIT {
            errors.push(format!(
                "(truncated: more than {ERRS_PER_FILE_LIMIT} failures in this file)"
            ));
            break;
        }
        if let Err(e) = runner.run_async(record).await {
            errors.push(e.to_string());
        }
    }

    FileOutcome {
        path,
        errors,
        duration_ms: start.elapsed().as_millis(),
    }
}

fn print_summary(slt_root: &Path, outcomes: &[FileOutcome]) {
    let mut by_dir: BTreeMap<PathBuf, (usize, usize)> = BTreeMap::new();
    let mut total_pass = 0usize;
    let mut total_fail = 0usize;
    let mut total_ms = 0u128;

    for outcome in outcomes {
        total_ms += outcome.duration_ms;
        let dir = outcome
            .path
            .parent()
            .and_then(|p| p.strip_prefix(slt_root).ok())
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let entry = by_dir.entry(dir).or_default();
        if outcome.errors.is_empty() {
            entry.0 += 1;
            total_pass += 1;
        } else {
            entry.1 += 1;
            total_fail += 1;
        }
    }

    eprintln!();
    eprintln!("===== sqllogictest summary =====");
    for (dir, (pass, fail)) in &by_dir {
        let dir_str = if dir.as_os_str().is_empty() {
            ".".to_string()
        } else {
            dir.display().to_string()
        };
        eprintln!("  {dir_str:<70} pass={pass:<4} fail={fail}");
    }
    eprintln!(
        "  TOTAL                                                                  pass={total_pass:<4} fail={total_fail}  ({total_ms} ms)"
    );

    if total_fail > 0 {
        eprintln!();
        eprintln!("--- failing files ---");
        for outcome in outcomes.iter().filter(|o| !o.errors.is_empty()) {
            eprintln!(
                "  {} ({} error(s))",
                outcome
                    .path
                    .strip_prefix(slt_root)
                    .unwrap_or(&outcome.path)
                    .display(),
                outcome.errors.len()
            );
            for err in outcome.errors.iter().take(3) {
                eprintln!("    - {}", err.lines().next().unwrap_or(""));
            }
            if outcome.errors.len() > 3 {
                eprintln!("    ... ({} more)", outcome.errors.len() - 3);
            }
        }
    }
}
