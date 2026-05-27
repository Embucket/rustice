//! Embucket SQL Logic Test harness binary.
//!
//! Discovers `.slt` files under `tests/slt/` and runs each one against a fresh
//! in-memory rustice session via the upstream `sqllogictest` `Runner`.
//! Parsing (including glob-based `include` resolution) is delegated to
//! `sqllogictest::parse_file`. The `${CRATE_ROOT}` variable is published to
//! the runner via `set_var` so corpora can reach committed fixtures with
//! `control substitution on` scoped substitution.
//!
//! Failure mode is soft by default: errors are aggregated into a per-directory
//! summary and the process exits 0 unless `--strict` is passed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use clap::Parser;
use embucket_sqllogictest::embucket_validator;
use embucket_sqllogictest::engine::EmbucketSession;
use executor::test_helpers::create_df_session_with_catalog_url;
use futures::{FutureExt, StreamExt};
use sqllogictest::{Runner, parse_file};
use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
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

    /// List the files that would run and exit.
    #[arg(long)]
    list: bool,

    /// Write a markdown report of the run to this path.
    #[arg(long, value_name = "PATH")]
    report: Option<PathBuf>,

    // Libtest-compatible no-op flags so `cargo test -- --nocapture` works.
    // Output already streams to stderr because the harness uses `harness = false`.
    #[arg(long, hide = true)]
    nocapture: bool,
    #[arg(long, hide = true)]
    show_output: bool,
    #[arg(long, hide = true)]
    exact: bool,
    #[arg(long, hide = true)]
    quiet: bool,
    #[arg(long, hide = true)]
    include_ignored: bool,
    #[arg(long, hide = true)]
    ignored: bool,
    #[arg(long, hide = true, value_name = "WHEN")]
    color: Option<String>,
    #[arg(long, hide = true, value_name = "FORMAT")]
    format: Option<String>,
    #[arg(long, hide = true, value_name = "PATTERN")]
    skip: Vec<String>,
}

fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
}

const ERRS_PER_FILE_LIMIT: usize = 10;

thread_local! {
    static LAST_PANIC: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

#[derive(Debug)]
struct FileOutcome {
    path: PathBuf,
    errors: Vec<String>,
    duration_ms: u128,
    // Per-file Iceberg catalog tempdir. Held until the outcome is consumed so
    // the directory survives every query the runner executed against it.
    _catalog_tempdir: Option<tempfile::TempDir>,
}

/// Suite directories whose `.slt` files require a real filesystem-backed
/// `file://` Iceberg catalog instead of the in-memory `/dev` default. They
/// typically need `COPY INTO file://` to reach committed fixture data.
const FILE_CATALOG_SUITES: &[&str] = &["dbt_snowplow_web"];

/// Per-file configuration derived from the `.slt` path.
struct FileProfile {
    /// Catalog URL passed to `create_df_session_with_catalog_url`.
    catalog_url: String,
    /// Tempdir backing `catalog_url` for `file://` suites. Kept alive on the
    /// `FileOutcome` so it survives every query the runner runs.
    catalog_tempdir: Option<tempfile::TempDir>,
}

fn profile_for(path: &Path) -> std::io::Result<FileProfile> {
    let needs_file_catalog = path
        .components()
        .any(|c| FILE_CATALOG_SUITES.iter().any(|s| c.as_os_str() == *s));
    if needs_file_catalog {
        let td = tempfile::tempdir()?;
        Ok(FileProfile {
            catalog_url: format!("file://{}", td.path().display()),
            catalog_tempdir: Some(td),
        })
    } else {
        Ok(FileProfile {
            catalog_url: "/dev".to_string(),
            catalog_tempdir: None,
        })
    }
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

    // Capture the most recent panic message per thread so `catch_unwind` payloads
    // (which can be opaque) get a human-readable companion. Also suppresses the
    // default stderr backtrace so it doesn't interleave with the summary.
    std::panic::set_hook(Box::new(|info| {
        LAST_PANIC.with(|cell| {
            *cell.borrow_mut() = Some(info.to_string());
        });
    }));

    let total = files.len();
    let progress_root = slt_root.clone();
    let outcomes: Vec<FileOutcome> = futures::stream::iter(files)
        .map(|path| {
            let path_for_panic = path.clone();
            let start = Instant::now();
            LAST_PANIC.with(|cell| *cell.borrow_mut() = None);
            AssertUnwindSafe(run_file(path))
                .catch_unwind()
                .map(move |result| match result {
                    Ok(outcome) => outcome,
                    Err(payload) => {
                        let hook_msg = LAST_PANIC.with(|cell| cell.borrow_mut().take());
                        let payload_msg = panic_message(&payload);
                        let msg = hook_msg.unwrap_or(payload_msg);
                        FileOutcome {
                            path: path_for_panic,
                            errors: vec![format!("panicked: {msg}")],
                            duration_ms: start.elapsed().as_millis(),
                            _catalog_tempdir: None,
                        }
                    }
                })
        })
        .buffer_unordered(cli.test_threads)
        .enumerate()
        .map(|(idx, outcome)| {
            let rel = outcome
                .path
                .strip_prefix(&progress_root)
                .unwrap_or(&outcome.path)
                .display();
            let status = if outcome.errors.is_empty() {
                "PASS".to_string()
            } else {
                format!("FAIL ({} err)", outcome.errors.len())
            };
            eprintln!(
                "[{:>4}/{:<4}] {:<14} {} ({} ms)",
                idx + 1,
                total,
                status,
                rel,
                outcome.duration_ms
            );
            outcome
        })
        .collect()
        .await;

    print_summary(&slt_root, &outcomes);

    if let Some(report_path) = cli.report.as_ref() {
        match write_report(report_path, &slt_root, &outcomes) {
            Ok(()) => eprintln!("\nreport written to {}", report_path.display()),
            Err(e) => eprintln!("\nfailed to write report to {}: {e}", report_path.display()),
        }
    }

    let failures = outcomes.iter().filter(|o| !o.errors.is_empty()).count();
    if cli.strict && failures > 0 { 1 } else { 0 }
}

fn write_report(path: &Path, slt_root: &Path, outcomes: &[FileOutcome]) -> std::io::Result<()> {
    use std::fmt::Write as _;

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

    let mut buf = String::new();
    let _ = writeln!(buf, "# sqllogictest report");
    let _ = writeln!(buf);
    let _ = writeln!(
        buf,
        "**Total:** {} pass, {} fail ({} ms)",
        total_pass, total_fail, total_ms
    );
    let _ = writeln!(buf);
    let _ = writeln!(buf, "## Per-directory");
    let _ = writeln!(buf);
    let _ = writeln!(buf, "| Directory | Pass | Fail |");
    let _ = writeln!(buf, "|---|---:|---:|");
    for (dir, (pass, fail)) in &by_dir {
        let dir_str = if dir.as_os_str().is_empty() {
            ".".to_string()
        } else {
            dir.display().to_string()
        };
        let _ = writeln!(buf, "| `{dir_str}` | {pass} | {fail} |");
    }

    if total_fail > 0 {
        let _ = writeln!(buf);
        let _ = writeln!(buf, "## Failing files");
        let _ = writeln!(buf);
        for outcome in outcomes.iter().filter(|o| !o.errors.is_empty()) {
            let rel = outcome
                .path
                .strip_prefix(slt_root)
                .unwrap_or(&outcome.path)
                .display();
            let _ = writeln!(buf, "### `{}` ({} error(s))", rel, outcome.errors.len());
            let _ = writeln!(buf);
            let _ = writeln!(buf, "```");
            for err in &outcome.errors {
                let _ = writeln!(buf, "{err}");
            }
            let _ = writeln!(buf, "```");
            let _ = writeln!(buf);
        }
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, buf)
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

fn collect_files(root: &Path, cli: &Cli) -> Vec<PathBuf> {
    let mut acc = Vec::new();
    if root.exists() {
        walk(root, &mut acc);
    }
    acc.retain(|p| p.extension().is_some_and(|e| e == "slt"));
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

    let profile = match profile_for(&path) {
        Ok(p) => p,
        Err(e) => {
            return FileOutcome {
                path,
                errors: vec![format!("tempdir error: {e}")],
                duration_ms: start.elapsed().as_millis(),
                _catalog_tempdir: None,
            };
        }
    };
    let FileProfile {
        catalog_url,
        catalog_tempdir,
    } = profile;

    let records = match parse_file::<embucket_sqllogictest::output::DFColumnType>(&path) {
        Ok(r) => r,
        Err(e) => {
            return FileOutcome {
                path,
                errors: vec![format!("parse error: {e}")],
                duration_ms: start.elapsed().as_millis(),
                _catalog_tempdir: catalog_tempdir,
            };
        }
    };

    let session = create_df_session_with_catalog_url(&catalog_url).await;
    let make_session = move || {
        let session = Arc::clone(&session);
        async move { Ok::<_, embucket_sqllogictest::error::Error>(EmbucketSession::new(session)) }
    };

    let mut runner = Runner::new(make_session);
    runner.add_label("embucket");
    runner.with_validator(embucket_validator);
    // Published for `control substitution on` blocks (e.g. snowplow setup
    // referencing `${CRATE_ROOT}/tests/fixtures/snowplow/events.csv`).
    // Harmless for files that don't reference the variable.
    runner.set_var(
        "CRATE_ROOT".to_string(),
        env!("CARGO_MANIFEST_DIR").to_string(),
    );

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
        _catalog_tempdir: catalog_tempdir,
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
            eprintln!();
            eprintln!(
                "===== {} ({} error(s)) =====",
                outcome
                    .path
                    .strip_prefix(slt_root)
                    .unwrap_or(&outcome.path)
                    .display(),
                outcome.errors.len()
            );
            for (i, err) in outcome.errors.iter().enumerate() {
                eprintln!("--- error {} ---", i + 1);
                for line in err.lines() {
                    eprintln!("  {line}");
                }
            }
        }
    }
}
