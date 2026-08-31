use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Confidence bucket for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Bucket {
    /// Identifier appears exactly once in the corpus: at its own declaration.
    Dead,
    /// Declared in production code, referenced only from test files.
    TestOnly,
    /// No code reference, but the name occurs in a string literal or a
    /// resource file (storyboard, manifest, DI key, selector...).
    Dynamic,
}

impl Bucket {
    pub const ALL: [Bucket; 3] = [Bucket::Dead, Bucket::TestOnly, Bucket::Dynamic];

    pub fn label(self) -> &'static str {
        match self {
            Bucket::Dead => "DEAD",
            Bucket::TestOnly => "TEST-ONLY",
            Bucket::Dynamic => "DYNAMIC?",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Bucket::Dead => "no reference anywhere — strongest candidates",
            Bucket::TestOnly => "production code never touches these",
            Bucket::Dynamic => {
                "only found in a string literal or resource file — verify before deleting"
            }
        }
    }
}

/// Which language a declaration came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Swift,
    Kotlin,
}

/// One flagged declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub bucket: Bucket,
    pub language: Language,
    /// Declaration name, e.g. `recalculateLegacyTotals`.
    pub name: String,
    /// Declaration kind as written in source: `func`, `class`, `enum entry`...
    pub kind: String,
    /// Path as given, relative to the scan root when possible.
    pub file: PathBuf,
    /// 1-based line number of the declaration.
    pub line: usize,
    /// Occurrences in non-test code (includes the declaration itself).
    pub prod_refs: usize,
    /// Occurrences in test code.
    pub test_refs: usize,
    /// Occurrences in string literals and resource files.
    pub dynamic_refs: usize,
}

/// Aggregate result of a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub root: PathBuf,
    pub code_files: usize,
    pub resource_files: usize,
    /// Total declarations examined, before bucketing.
    pub declarations: usize,
    pub findings: Vec<Finding>,
}

impl ScanReport {
    pub fn count(&self, bucket: Bucket) -> usize {
        self.findings.iter().filter(|f| f.bucket == bucket).count()
    }

    pub fn in_bucket(&self, bucket: Bucket) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.bucket == bucket)
    }
}

/// Knobs for a scan. `Default` matches the CLI defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanOptions {
    /// Flag `override func` / `override fun` too. These normally satisfy a
    /// superclass or framework contract.
    pub include_overrides: bool,
    /// Flag declarations that live inside test targets. XCTest and JUnit
    /// entry points are discovered by the runtime, so this is noisy.
    pub include_tests: bool,
    /// Ignore identifiers shorter than this.
    pub min_len: usize,
    /// Extra identifier names to treat as implicitly reachable, merged with
    /// the built-in noise list.
    pub extra_noise: Vec<String>,
    /// Extra attribute/annotation strings that mark a declaration as
    /// dynamically reachable, merged with the built-in list.
    pub extra_dynamic_attrs: Vec<String>,
    /// Extra directory names to skip, merged with the built-in list.
    pub extra_skip_dirs: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            include_overrides: false,
            include_tests: false,
            min_len: 3,
            extra_noise: Vec::new(),
            extra_dynamic_attrs: Vec::new(),
            extra_skip_dirs: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum ScanError {
    RootNotFound(PathBuf),
    NoSourceFiles(PathBuf),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::RootNotFound(p) => write!(f, "path does not exist: {}", p.display()),
            ScanError::NoSourceFiles(p) => {
                write!(f, "no .swift/.kt/.kts files found under {}", p.display())
            }
        }
    }
}

impl std::error::Error for ScanError {}
