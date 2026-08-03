use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use eonsort_core::copy::{self, CopyOptions, CopyProgress};
use eonsort_core::model::DEFAULT_FOLDER_PATTERN;
use eonsort_core::providers::{DetectOptions, Provider, Strategy};
use eonsort_core::scan::{ScanOptions, ScanPhase, ScanProgress};
use eonsort_core::verify::{VerifyOptions, VerifyProgress, VerifyReport};
use eonsort_core::{default_plan_name, read_plan, scan, verify};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "eonsort",
    version,
    about = "Sort files into date-based folders"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Analyse sources and write a plan describing where each file would go.
    Scan(ScanArgs),
    /// Copy the files described by a plan. Safe to re-run after an interruption.
    Copy(CopyArgs),
    /// Compare a plan against what is actually on disk.
    Verify(VerifyArgs),
    /// Scan and then copy in one go.
    Sort(SortArgs),
    /// Print the entries of a plan.
    Show(ShowArgs),
}

#[derive(Args)]
struct ScanArgs {
    /// One or more source directories.
    #[arg(short, long, required = true, num_args = 1..)]
    source: Vec<PathBuf>,
    /// Root directory the sorted tree is written to.
    #[arg(short, long)]
    destination: PathBuf,
    /// Where to write the plan file.
    #[arg(short, long)]
    plan: Option<PathBuf>,
    /// Destination folder layout, as a strftime pattern.
    #[arg(long, default_value = DEFAULT_FOLDER_PATTERN)]
    pattern: String,
    /// Date sources to consult, in priority order.
    #[arg(long, value_enum, num_args = 1.., default_values = ["filename", "exif", "media", "filesystem"])]
    provider: Vec<ProviderArg>,
    /// How to choose between the dates different providers report.
    #[arg(long, value_enum, default_value = "oldest")]
    strategy: StrategyArg,
    /// Follow symbolic links while walking the sources.
    #[arg(long)]
    follow_symlinks: bool,
}

#[derive(Args)]
struct CopyArgs {
    #[arg(short, long)]
    plan: PathBuf,
    /// Number of files to copy in parallel.
    #[arg(short, long)]
    jobs: Option<usize>,
    /// Do not carry the source timestamps over to the copies.
    #[arg(long)]
    no_preserve_times: bool,
}

#[derive(Args)]
struct VerifyArgs {
    #[arg(short, long)]
    plan: PathBuf,
    /// Compare file contents instead of just sizes.
    #[arg(long)]
    hash: bool,
}

#[derive(Args)]
struct SortArgs {
    #[command(flatten)]
    scan: ScanArgs,
    #[arg(short, long)]
    jobs: Option<usize>,
}

#[derive(Args)]
struct ShowArgs {
    #[arg(short, long)]
    plan: PathBuf,
    /// Emit one JSON object per entry instead of a table.
    #[arg(long)]
    json: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum ProviderArg {
    Filename,
    Exif,
    Media,
    Filesystem,
}

impl From<ProviderArg> for Provider {
    fn from(value: ProviderArg) -> Self {
        match value {
            ProviderArg::Filename => Provider::Filename,
            ProviderArg::Exif => Provider::Exif,
            ProviderArg::Media => Provider::Media,
            ProviderArg::Filesystem => Provider::Filesystem,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum StrategyArg {
    Oldest,
    Priority,
}

impl From<StrategyArg> for Strategy {
    fn from(value: StrategyArg) -> Self {
        match value {
            StrategyArg::Oldest => Strategy::Oldest,
            StrategyArg::Priority => Strategy::Priority,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cancel = install_cancel_handler()?;

    match cli.command {
        Command::Scan(args) => {
            let (plan_path, _) = run_scan(&args, &cancel)?;
            println!("Plan written to {}", plan_path.display());
        }
        Command::Copy(args) => run_copy(&args.plan, args.jobs, !args.no_preserve_times, &cancel)?,
        Command::Verify(args) => {
            let report = run_verify(&args.plan, args.hash, &cancel)?;
            print_verify_report(&report);
        }
        Command::Sort(args) => {
            let (plan_path, _) = run_scan(&args.scan, &cancel)?;
            run_copy(&plan_path, args.jobs, true, &cancel)?;
        }
        Command::Show(args) => show(&args)?,
    }

    Ok(())
}

fn install_cancel_handler() -> Result<Arc<AtomicBool>> {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    ctrlc::set_handler(move || {
        eprintln!("\nStopping. Re-run the same command to continue where this left off.");
        flag.store(true, Ordering::Relaxed);
    })
    .context("could not install the Ctrl-C handler")?;
    Ok(cancel)
}

fn run_scan(args: &ScanArgs, cancel: &AtomicBool) -> Result<(PathBuf, u64)> {
    let options = ScanOptions {
        sources: args.source.clone(),
        destination: args.destination.clone(),
        folder_pattern: args.pattern.clone(),
        detect: DetectOptions {
            providers: args.provider.iter().map(|p| (*p).into()).collect(),
            strategy: args.strategy.into(),
        },
        follow_symlinks: args.follow_symlinks,
    };

    let plan_path = args.plan.clone().unwrap_or_else(|| {
        PathBuf::from(default_plan_name(&options.sources, &options.destination))
    });

    let bar = spinner("Scanning");
    let plan = scan(&plan_path, &options, cancel, &|progress: ScanProgress| {
        match progress.phase {
            ScanPhase::Counting => {
                bar.set_message(format!("Counting: {} files", progress.files_seen))
            }
            ScanPhase::Analysing => {
                bar.set_message(format!(
                    "Analysing: {}/{} files",
                    progress.files_seen, progress.files_total
                ));
            }
        }
        bar.tick();
    })
    .context("scan failed")?;
    bar.finish_and_clear();

    println!(
        "{} files planned, {} skipped, {}",
        plan.entries.len(),
        plan.skipped.len(),
        human_bytes(plan.total_bytes())
    );
    Ok((plan_path, plan.entries.len() as u64))
}

fn run_copy(
    plan_path: &Path,
    jobs: Option<usize>,
    preserve_times: bool,
    cancel: &AtomicBool,
) -> Result<()> {
    let options = CopyOptions {
        concurrency: jobs.unwrap_or_else(copy::default_concurrency),
        preserve_times,
    };

    let plan = read_plan(plan_path).context("could not read the plan")?;
    let bar = byte_bar(plan.total_bytes());

    let report = copy::execute(plan_path, &options, cancel, &|progress: CopyProgress| {
        bar.set_position(progress.bytes_done);
        bar.set_message(format!(
            "{}/{} files",
            progress.files_done, progress.files_total
        ));
    })
    .context("copy failed")?;
    bar.finish_and_clear();

    println!(
        "Copied {}, duplicates {}, already present {}, failed {}",
        report.progress.copied,
        report.progress.duplicates,
        report.progress.already_present,
        report.progress.failed
    );
    for failure in &report.failures {
        eprintln!("  failed: {}", failure.source.display());
    }
    Ok(())
}

fn run_verify(plan_path: &Path, hash: bool, cancel: &AtomicBool) -> Result<VerifyReport> {
    let plan = read_plan(plan_path).context("could not read the plan")?;
    let bar = count_bar(plan.entries.len() as u64);

    let report = verify(
        plan_path,
        &VerifyOptions {
            compare_hashes: hash,
        },
        cancel,
        &|progress: VerifyProgress| bar.set_position(progress.checked),
    )
    .context("verify failed")?;
    bar.finish_and_clear();
    Ok(report)
}

fn print_verify_report(report: &VerifyReport) {
    println!("OK                    {}", report.ok);
    println!("Missing at destination {}", report.destination_missing);
    println!("Content mismatch       {}", report.content_mismatch);
    println!("Source missing         {}", report.source_missing);
    println!(
        "Source bytes           {}",
        human_bytes(report.source_bytes)
    );
    println!(
        "Destination bytes      {}",
        human_bytes(report.destination_bytes)
    );
    println!(
        "Duplicates             {} ({})",
        report.duplicate_files,
        human_bytes(report.duplicate_bytes)
    );
    for issue in &report.issues {
        println!("  {:?}: {}", issue.kind, issue.source.display());
    }
}

fn show(args: &ShowArgs) -> Result<()> {
    let plan = read_plan(&args.plan).context("could not read the plan")?;
    for entry in &plan.entries {
        if args.json {
            println!("{}", serde_json::to_string(entry)?);
        } else {
            println!(
                "{}  {}  {}  ->  {}",
                entry.taken,
                entry.provider.label(),
                entry.source.display(),
                entry.destination.display()
            );
        }
    }
    Ok(())
}

fn spinner(prefix: &str) -> ProgressBar {
    let bar = ProgressBar::new_spinner();
    bar.set_style(
        ProgressStyle::with_template("{spinner} {prefix} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    bar.set_prefix(prefix.to_string());
    bar
}

fn byte_bar(total: u64) -> ProgressBar {
    let bar = ProgressBar::new(total);
    bar.set_style(
        ProgressStyle::with_template(
            "{bar:40} {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta}) {msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    bar
}

fn count_bar(total: u64) -> ProgressBar {
    let bar = ProgressBar::new(total);
    bar.set_style(
        ProgressStyle::with_template("{bar:40} {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    bar
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_byte_counts() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.00 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.00 MB");
    }

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
