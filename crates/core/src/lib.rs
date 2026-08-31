//! Heuristic dead-code detection for Swift/SwiftUI and Kotlin codebases.
//!
//! This is a text scanner, not a compiler. It strips comments and string
//! literals, tokenizes what remains into an identifier frequency map, then
//! reports declarations whose only occurrence is their own declaration site.
//!
//! It has no type information, no call graph, and no module boundaries. It
//! cannot see reflection, annotation-processor-generated call sites, result
//! builders resolving members by type, or public API consumed by another
//! module. Findings are candidates to inspect, never verdicts.
//!
//! ```no_run
//! use deadcode_core::{scan, ScanOptions, Bucket};
//! use std::path::Path;
//!
//! let report = scan(Path::new("./MyApp"), &ScanOptions::default())?;
//! println!("{} dead candidates", report.count(Bucket::Dead));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod lang;
mod lexer;
mod types;

pub use lang::{CODE_EXTS, DYNAMIC_ATTRS, NOISE, RESOURCE_EXTS, SKIP_DIRS};
pub use types::{Bucket, Finding, Language, ScanError, ScanOptions, ScanReport};

use lang::{language_for_ext, Patterns};
use lexer::{split_code_and_strings, tally};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Scan `root` for dead-code candidates.
pub fn scan(root: &Path, opts: &ScanOptions) -> Result<ScanReport, ScanError> {
    if !root.exists() {
        return Err(ScanError::RootNotFound(root.to_path_buf()));
    }

    let skip_dirs: HashSet<&str> = SKIP_DIRS
        .iter()
        .copied()
        .chain(opts.extra_skip_dirs.iter().map(String::as_str))
        .collect();

    let mut files = Vec::new();
    walk(root, &skip_dirs, &mut files);

    let code_files: Vec<PathBuf> = files
        .iter()
        .filter(|f| ext_of(f).and_then(language_for_ext).is_some())
        .cloned()
        .collect();

    if code_files.is_empty() {
        return Err(ScanError::NoSourceFiles(root.to_path_buf()));
    }
    let resource_file_count = files.len() - code_files.len();

    // Pass 1 — build frequency maps and the conformance-name set.
    let patterns = Patterns::new();
    let mut prod_counts: HashMap<String, usize> = HashMap::new();
    let mut test_counts: HashMap<String, usize> = HashMap::new();
    let mut dynamic_counts: HashMap<String, usize> = HashMap::new();
    let mut conformance: HashSet<String> = HashSet::new();
    let mut sources: HashMap<PathBuf, String> = HashMap::new();

    for file in &files {
        let Ok(raw) = fs::read_to_string(file) else {
            continue;
        };
        if ext_of(file).and_then(language_for_ext).is_some() {
            let (code, strings) = split_code_and_strings(&raw);
            collect_conformance_names(&code, &patterns, &mut conformance);
            if is_test_file(file) {
                tally(&code, &mut test_counts);
            } else {
                tally(&code, &mut prod_counts);
            }
            tally(&strings, &mut dynamic_counts);
            sources.insert(file.clone(), raw);
        } else {
            tally(&raw, &mut dynamic_counts);
        }
    }

    // Pass 2 — extract declarations and bucket them.
    let noise: HashSet<&str> = NOISE
        .iter()
        .copied()
        .chain(opts.extra_noise.iter().map(String::as_str))
        .collect();
    let dynamic_attrs: Vec<&str> = DYNAMIC_ATTRS
        .iter()
        .copied()
        .chain(opts.extra_dynamic_attrs.iter().map(String::as_str))
        .collect();

    let mut findings: Vec<Finding> = Vec::new();
    let mut declarations = 0usize;

    for file in &code_files {
        let Some(content) = sources.get(file) else {
            continue;
        };
        let Some(language) = ext_of(file).and_then(language_for_ext) else {
            continue;
        };
        let decl_in_test = is_test_file(file);
        // XCTest / JUnit entry points are discovered by the runtime, and test
        // helpers legitimately live only in the test target.
        if decl_in_test && !opts.include_tests {
            continue;
        }

        for (kind, re) in patterns.for_language(language) {
            for cap in re.captures_iter(content) {
                let whole = cap.get(0).expect("group 0 always present");
                let name = cap[1].to_string();
                declarations += 1;

                if name.len() < opts.min_len || noise.contains(name.as_str()) {
                    continue;
                }
                // A conformance requirement must exist even if nothing calls it.
                if conformance.contains(&name) {
                    continue;
                }
                if !opts.include_overrides && whole.as_str().contains("override") {
                    continue;
                }
                if has_dynamic_attr(content, whole.start(), &dynamic_attrs) {
                    continue;
                }

                let prod = *prod_counts.get(&name).unwrap_or(&0);
                let test = *test_counts.get(&name).unwrap_or(&0);
                let dynamic = *dynamic_counts.get(&name).unwrap_or(&0);

                let bucket = if prod + test > 1 {
                    if !decl_in_test && prod <= 1 && test > 0 {
                        Bucket::TestOnly
                    } else {
                        continue; // genuinely referenced
                    }
                } else if dynamic > 0 {
                    Bucket::Dynamic
                } else {
                    Bucket::Dead
                };

                findings.push(Finding {
                    bucket,
                    language,
                    name,
                    kind: (*kind).to_string(),
                    file: relative_to(root, file),
                    line: content[..whole.start()].matches('\n').count() + 1,
                    prod_refs: prod,
                    test_refs: test,
                    dynamic_refs: dynamic,
                });
            }
        }
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.name.cmp(&b.name))
    });
    findings.dedup_by(|a, b| a.name == b.name && a.file == b.file && a.line == b.line);

    Ok(ScanReport {
        root: root.to_path_buf(),
        code_files: code_files.len(),
        resource_files: resource_file_count,
        declarations,
        findings,
    })
}

fn ext_of(p: &Path) -> Option<&str> {
    p.extension().and_then(|e| e.to_str())
}

fn relative_to(root: &Path, file: &Path) -> PathBuf {
    file.strip_prefix(root).unwrap_or(file).to_path_buf()
}

/// True if the path looks like a test target.
pub fn is_test_file(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    s.contains("/test")
        || s.contains("tests/")
        || s.contains("androidtest")
        || s.contains("uitest")
        || s.ends_with("test.swift")
        || s.ends_with("tests.swift")
        || s.ends_with("test.kt")
        || s.ends_with("tests.kt")
        || s.contains("spec.kt")
        || s.contains("/mock")
        || s.contains("fixtures")
}

fn walk(dir: &Path, skip_dirs: &HashSet<&str>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if skip_dirs.contains(name) || name.starts_with('.') {
                    continue;
                }
            }
            walk(&path, skip_dirs, out);
        } else if let Some(ext) = ext_of(&path) {
            if CODE_EXTS.contains(&ext) || RESOURCE_EXTS.contains(&ext) {
                out.push(path);
            }
        }
    }
}

/// Collect names declared inside `protocol` / `interface` bodies.
fn collect_conformance_names(code: &str, patterns: &Patterns, out: &mut HashSet<String>) {
    let b = code.as_bytes();
    for m in patterns.conformance_head.find_iter(code) {
        let mut i = m.end();
        while i < b.len() && b[i] != b'{' && b[i] != b'\n' {
            i += 1;
        }
        if i >= b.len() || b[i] != b'{' {
            continue;
        }
        let body_start = i + 1;
        let mut depth = 1;
        i = body_start;
        while i < b.len() && depth > 0 {
            match b[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        let body_end = i.saturating_sub(1).max(body_start).min(code.len());
        if !code.is_char_boundary(body_start) || !code.is_char_boundary(body_end) {
            continue;
        }
        for c in patterns
            .conformance_member
            .captures_iter(&code[body_start..body_end])
        {
            out.insert(c[1].to_string());
        }
    }
}

/// Attributes often sit on their own line above the declaration, so look at
/// the declaration line plus the three lines above it.
fn has_dynamic_attr(content: &str, decl_start: usize, attrs: &[&str]) -> bool {
    let start = content[..decl_start]
        .rmatch_indices('\n')
        .nth(3)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let end = content[decl_start..]
        .find('\n')
        .map(|i| decl_start + i)
        .unwrap_or(content.len());
    let window = &content[start..end];
    attrs.iter().any(|a| window.contains(a))
}
