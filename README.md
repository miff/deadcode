# deadcode

A heuristic dead-code finder for **Swift/SwiftUI** and **Kotlin** codebases.

It is a fast, dependency-light text scanner — not a compiler. It tells you where
to look, not what to delete. Read [Limits](#limits) before you trust a single
line of output.

## Layout

```
deadcode/
├── Cargo.toml              # workspace
└── crates/
    ├── core/               # deadcode-core — all logic, no I/O side effects
    │   └── src/
    │       ├── lib.rs      # scan() entry point, walking, bucketing
    │       ├── types.rs    # Bucket, Finding, ScanReport, ScanOptions
    │       ├── lang.rs     # per-language patterns, suppression lists
    │       └── lexer.rs    # comment/string stripping, identifier tallying
    └── cli/                # deadcode-cli — arg parsing and output only
        └── src/main.rs
```

The split exists so the scanner is callable as a library. `scan()` returns a
`ScanReport`; nothing in `core` prints or exits. Any other frontend — a GUI,
an LSP server, a CI plugin — depends on `deadcode-core` and never touches the CLI.

## Build

```sh
cargo build --release
cargo test --workspace
```

Binary lands at `target/release/deadcode`. Dependencies: `regex` and `serde` in
core, `serde_json` in the CLI.

## Usage

```sh
./target/release/deadcode /path/to/repo
```

| Flag | Default | Effect |
|---|---|---|
| `--json` | off | Emit the full `ScanReport` as JSON instead of text |
| `--bucket <B>` | all | Report only one bucket: `dead`, `test-only`, `dynamic` |
| `--include-overrides` | off | Also flag `override func` / `override fun`. Off because these satisfy superclass/framework contracts. |
| `--include-tests` | off | Also flag declarations *inside* test targets. Off because XCTest/JUnit methods are runtime-discovered. |
| `--min-len N` | `3` | Ignore identifiers shorter than N characters |
| `--fail-on-dead` | off | Exit `2` if any DEAD finding exists |

Exit codes: `0` completed, `1` usage or I/O error, `2` DEAD findings with
`--fail-on-dead`.

## Output

Findings are split into three confidence buckets:

```
scanned 214 code files, 38 resource files, 1902 declarations

== DEAD (17) — no reference anywhere — strongest candidates
  App/Sources/Legacy.swift:3  [struct]  OldMigrator
  android/src/main/java/Repo.kt:12  [fun]  deprecatedSyncLoad

== TEST-ONLY (1) — production code never touches these
  App/Sources/Legacy.swift:7  [func]  helperUsedOnlyByTests

== DYNAMIC? (1) — only found in a string literal or resource file — verify before deleting
  App/Sources/LegacyVC.swift:2  [class]  LegacyStoryboardVC
```

- **DEAD** — the identifier appears exactly once in the whole tree: at its own
  declaration. Highest-confidence bucket, still not a guarantee.
- **TEST-ONLY** — declared in production code, referenced only from test files.
  Either the feature is gone and the test is vestigial, or the code exists purely
  to make a test pass. Both worth a look.
- **DYNAMIC?** — no code reference, but the name shows up in a string literal or
  a resource file. Typical causes: storyboard `customClass`, `AndroidManifest.xml`
  entries, `NSSelectorFromString`, DI keys, analytics event names. Usually alive.
  Lowest confidence.

## JSON output

`--json` emits the whole report, camelCase keys, ready to pipe:

```json
{
  "root": "/path/to/repo",
  "codeFiles": 5,
  "resourceFiles": 2,
  "declarations": 41,
  "findings": [
    {
      "bucket": "dead",
      "language": "swift",
      "name": "OldMigrator",
      "kind": "struct",
      "file": "App/Sources/Legacy.swift",
      "line": 3,
      "prodRefs": 1,
      "testRefs": 0,
      "dynamicRefs": 0
    }
  ]
}
```

`bucket` is `dead` | `testOnly` | `dynamic`. `language` is `swift` | `kotlin`.
`file` is relative to the scan root. The `*Refs` counts are the raw evidence
behind the bucket — useful for building your own threshold.

Broken pipes are handled, so `| head` and `| jq 'first'` are safe.

```sh
# how many dead candidates per file
deadcode . --json --bucket dead | jq -r '.findings[].file' | sort | uniq -c | sort -rn

# just the names, for a grep sweep
deadcode . --json --bucket dead | jq -r '.findings[].name'

# CI gate
deadcode . --fail-on-dead --bucket dead
```

**Ratcheting in CI** is more useful than a hard gate, since no real codebase
starts at zero. Commit a baseline and fail only on regressions:

```sh
deadcode . --json --bucket dead | jq -r '.findings[] | "\(.file):\(.name)"' | sort > current.txt
comm -13 baseline.txt current.txt | tee new.txt
[ -s new.txt ] && echo "new dead code introduced" && exit 1
```

## How it works

1. Walks the tree collecting `.swift`, `.kt`, `.kts` (code) and `.xib`,
   `.storyboard`, `.plist`, `.strings`, `.stringsdict`, `.entitlements`, `.xml`,
   `.json`, `.xcstrings` (resources). Skips `build`, `.build`, `DerivedData`,
   `Pods`, `Carthage`, `node_modules`, `.gradle`, `generated`, and dotfiles.
2. For each code file, strips comments and string literals. A name mentioned only
   in a `// TODO` no longer counts as "used".
3. Tokenizes the stripped code once into an identifier→count map, kept separate
   for production files and test files. String literals and resource files go into
   a third "dynamic reference" map.
4. Regex-extracts declarations per language, then buckets each by its counts.

Counting is one pass over the corpus, not one grep per identifier, so cost is
linear in total bytes rather than declarations × bytes.

### Declarations recognised

**Swift:** `func`, `class`, `struct`, `enum`, `protocol`, `actor`, `typealias`,
top-level/type-level `let`/`var`, `enum case`.

**Kotlin:** `fun`, `class`, `interface`, `object`, `typealias`, top-level/class-level
`val`/`var`, screaming-case enum entries.

Locals inside function bodies are deliberately skipped — the compiler already
warns about those.

### Built-in suppressions

Without these the output is unusable on any real app.

- **Protocol / interface members.** Any name declared inside a `protocol` or
  `interface` body is skipped everywhere. A conformance requirement must exist
  even if nothing calls it directly.
- **Overrides**, unless `--include-overrides`.
- **Runtime/codegen attributes** on the declaration or the three lines above it:
  `@objc`, `@IBAction`, `@IBOutlet`, `@NSManaged`, `@main`, `@Composable`,
  `@Test`, `@Inject`, `@Serializable`, `@Parcelize`, `@Entity`, `@Dao`, and more
  (see `DYNAMIC_ATTRS` in `crates/core/src/lang.rs`).
- **Lifecycle noise list**: `body`, `makeUIView`, `viewDidLoad`, `previews`,
  `onCreate`, `onBindViewHolder`, `toString`, `equals`, and similar (see `NOISE`).

## Using it as a library

```rust
use deadcode_core::{scan, Bucket, ScanOptions};
use std::path::Path;

let opts = ScanOptions {
    min_len: 4,
    extra_noise: vec!["configureCell".into()],
    ..Default::default()
};
let report = scan(Path::new("./MyApp"), &opts)?;

for f in report.in_bucket(Bucket::Dead) {
    println!("{}:{} {}", f.file.display(), f.line, f.name);
}
```

`ScanOptions` takes `extra_noise`, `extra_dynamic_attrs`, and `extra_skip_dirs`,
which merge with the built-in lists — so a frontend can extend suppressions
without patching `lang.rs`. All types derive `Serialize`/`Deserialize`.

## Limits

This is regex plus token counting. No type information, no call graph, no module
boundaries. It **cannot** see:

- Reflection and runtime lookup beyond the attribute list above
- KSP / annotation-processor / SwiftGen-generated call sites
- SwiftUI `ViewModifier` and result-builder members resolved by type
- Generic constraint witnesses
- `Codable`-synthesized property access
- Public API consumed by another module, another target, or another repo —
  **framework/SDK targets will report their entire public surface as DEAD**

Two identifiers with the same name in different types share a count. A method
named `load` on a class nobody uses looks alive if any other `load` exists. That
inflates false negatives (missed dead code), not false positives.

**Never pipe this into an automated deletion.** Treat output as a work queue.

## Tuning it for your repo

Constants at the top of `crates/core/src/lang.rs`:

- `NOISE` — framework methods your codebase implements implicitly
- `DYNAMIC_ATTRS` — your DI framework's annotations
- `SKIP_DIRS` — generated-source directories
- `is_test_file()` in `lib.rs` — if your test paths don't match the defaults
  (`/test`, `tests/`, `androidTest`, `uiTest`, `*Test.swift`, `*Tests.kt`,
  `/mock`, `fixtures`)

Prefer `ScanOptions::extra_*` over editing the constants when you can — it
survives updates.

If the first run flags hundreds of items, the suppression lists need work before
the results mean anything. Start with `--min-len 5` to cut the tail.

## When to use something else

For **Swift**, [Periphery](https://github.com/peripheryapp/periphery) does real
index-store-based analysis via the compiler and is strictly more accurate.
For **Kotlin**, IntelliJ/Android Studio's *Analyze → Unused declaration* uses
actual resolution.