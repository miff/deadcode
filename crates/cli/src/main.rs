mod html;

use deadcode_core::{scan, Bucket, ScanOptions, ScanReport};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
deadcode — heuristic dead-code finder for Swift and Kotlin

USAGE:
    deadcode <path> [OPTIONS]

OPTIONS:
    --json                  Emit machine-readable JSON to stdout instead of text
    --save                  Also write ./deadcode-report.json
    --out <PATH>            Also write JSON to PATH (a directory gets
                            deadcode-report.json inside it)
    --html                  Also write an HTML report beside the JSON
                            (same path with a .html extension)
    --bucket <B>            Only report one bucket: dead | test-only | dynamic
    --include-overrides     Also flag `override func` / `override fun`
    --include-tests         Also flag declarations inside test targets
    --min-len <N>           Ignore identifiers shorter than N (default: 3)
    --fail-on-dead          Exit 2 if any DEAD finding exists (for CI)
    -h, --help              Show this help

EXIT CODES:
    0  scan completed
    1  usage or I/O error
    2  DEAD findings present and --fail-on-dead was set

--save and --out are independent of --json: without --json you still get the
human-readable report on stdout and the JSON on disk.

--html implies saving. On its own it writes ./deadcode-report.html only; add
--save or --out to get the .json alongside it.

Findings are candidates to inspect, not verdicts. See README.md for limits.";

const DEFAULT_REPORT_NAME: &str = "deadcode-report.json";

struct Cli {
    root: PathBuf,
    opts: ScanOptions,
    json: bool,
    out: Option<PathBuf>,
    html: bool,
    write_json_file: bool,
    only: Option<Bucket>,
    fail_on_dead: bool,
}

fn main() -> ExitCode {
    let cli = match parse_args() {
        Ok(Some(cli)) => cli,
        Ok(None) => return ExitCode::SUCCESS, // --help
        Err(msg) => {
            eprintln!("error: {msg}\n\n{USAGE}");
            return ExitCode::from(1);
        }
    };

    let report = match scan(&cli.root, &cli.opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    let report = match cli.only {
        Some(b) => filter_bucket(report, b),
        None => report,
    };

    if let Some(dest) = &cli.out {
        match write_outputs(&report, dest, cli.write_json_file, cli.html) {
            Ok(paths) => {
                for p in paths {
                    eprintln!("wrote {}", p.display());
                }
            }
            Err(e) => {
                eprintln!("error: could not write report: {e}");
                return ExitCode::from(1);
            }
        }
    }

    let write_result = if cli.json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => writeln!(io::stdout().lock(), "{s}"),
            Err(e) => {
                eprintln!("error: could not serialize report: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        print_text(&report)
    };

    // `deadcode ... | head` closes the pipe early. That is normal shell usage,
    // not an error worth a panic or a nonzero exit.
    if let Err(e) = write_result {
        if e.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }

    if cli.fail_on_dead && report.count(Bucket::Dead) > 0 {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn parse_args() -> Result<Option<Cli>, String> {
    let mut root: Option<PathBuf> = None;
    let mut opts = ScanOptions::default();
    let mut json = false;
    let mut out: Option<PathBuf> = None;
    let mut html = false;
    let mut wants_json_file = false;
    let mut only = None;
    let mut fail_on_dead = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--json" => json = true,
            "--save" => {
                out = Some(PathBuf::from(DEFAULT_REPORT_NAME));
                wants_json_file = true;
            }
            "--out" => {
                i += 1;
                let v = args.get(i).ok_or("--out needs a path")?;
                out = Some(PathBuf::from(v));
                wants_json_file = true;
            }
            "--html" => html = true,
            "--include-overrides" => opts.include_overrides = true,
            "--include-tests" => opts.include_tests = true,
            "--fail-on-dead" => fail_on_dead = true,
            "--min-len" => {
                i += 1;
                let v = args.get(i).ok_or("--min-len needs a number")?;
                opts.min_len = v.parse().map_err(|_| format!("bad --min-len value: {v}"))?;
            }
            "--bucket" => {
                i += 1;
                let v = args.get(i).ok_or("--bucket needs a value")?;
                only = Some(match v.to_lowercase().as_str() {
                    "dead" => Bucket::Dead,
                    "test-only" | "testonly" => Bucket::TestOnly,
                    "dynamic" => Bucket::Dynamic,
                    other => return Err(format!("unknown bucket: {other}")),
                });
            }
            other if other.starts_with('-') => return Err(format!("unknown flag: {other}")),
            other => {
                if root.is_some() {
                    return Err(format!("unexpected extra argument: {other}"));
                }
                root = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }

    let root = root.ok_or("missing <path>")?;

    // --html on its own still needs a destination to derive the .html name
    // from, but must not silently produce a .json the user did not ask for.
    if html && out.is_none() {
        out = Some(PathBuf::from(DEFAULT_REPORT_NAME));
    }

    Ok(Some(Cli {
        root,
        opts,
        json,
        out,
        html,
        write_json_file: wants_json_file,
        only,
        fail_on_dead,
    }))
}

/// Write the JSON and/or HTML artifacts derived from `dest`.
///
/// A directory destination gets the default filename inside it. The HTML file
/// is the same path with its extension replaced by `.html`, so
/// `--out reports/scan.json --html` yields `reports/scan.json` and
/// `reports/scan.html`. Returns the paths actually written.
fn write_outputs(
    report: &ScanReport,
    dest: &Path,
    write_json: bool,
    write_html: bool,
) -> io::Result<Vec<PathBuf>> {
    let base = if dest.is_dir() {
        dest.join(DEFAULT_REPORT_NAME)
    } else {
        dest.to_path_buf()
    };

    if let Some(parent) = base.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut written = Vec::new();

    if write_json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&base, json)?;
        written.push(base.clone());
    }

    if write_html {
        let path = base.with_extension("html");
        fs::write(&path, html::render(report)?)?;
        written.push(path);
    }

    Ok(written)
}

fn filter_bucket(mut report: ScanReport, bucket: Bucket) -> ScanReport {
    report.findings.retain(|f| f.bucket == bucket);
    report
}

fn print_text(report: &ScanReport) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    writeln!(
        out,
        "scanned {} code files, {} resource files, {} declarations\n",
        report.code_files, report.resource_files, report.declarations
    )?;

    for bucket in Bucket::ALL {
        let n = report.count(bucket);
        if n == 0 {
            continue;
        }
        writeln!(out, "== {} ({}) — {}", bucket.label(), n, bucket.note())?;
        for f in report.in_bucket(bucket) {
            writeln!(
                out,
                "  {}:{}  [{}]  {}",
                f.file.display(),
                f.line,
                f.kind,
                f.name
            )?;
        }
        writeln!(out)?;
    }

    if report.findings.is_empty() {
        writeln!(out, "nothing flagged.")?;
    }
    Ok(())
}
