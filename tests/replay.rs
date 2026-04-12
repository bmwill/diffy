//! Validate PatchSet parsing and application by replaying a git repository's history.
//!
//! Note: Git extended header paths (rename/copy) don't have a/b prefixes,
//! while ---/+++ paths do. This test handles both cases appropriately.
//!
//! ## Usage
//!
//! ```console
//! $ cargo test --test replay -- --ignored --nocapture
//! ```
//!
//! ## Environment Variables
//!
//! * `DIFFY_TEST_REPO`: Path to the git repository to test against.
//!   Defaults to the package directory (`CARGO_MANIFEST_DIR`).
//! * `DIFFY_TEST_COMMITS`: Commits to verify. Accepts either:
//!   * A number (e.g., `200`) for the last N commits from HEAD
//!   * A range (e.g., `abc123..def456`) for a specific commit range
//!
//!   Defaults to 200. Use `0` to verify entire history.
//! * `DIFFY_TEST_PARSE_MODE`: Parse mode to use (`unidiff` or `gitdiff`).
//!   Defaults to `unidiff`.
//!
//! ## Requirements
//!
//! * Git must be installed and available in the system's PATH.
//!
//! ## Runbook
//!
//! Repo history for upstream projects (e.g., rust-lang/cargo, rust-lang/rust)
//! is too long to run at full depth on every PR.
//!
//! This runbook guide you how run the workflow manually.
//!
//! Replay rust-lang/cargo with deeper history:
//!
//! ```console
//! $ gh workflow run Replay -f repo_url=https://github.com/rust-lang/cargo -f commits=2000
//! ```
//!
//! Replay rust-lang/rust with a smaller depth first:
//!
//! ```console
//! $ gh workflow run Replay -f repo_url=https://github.com/rust-lang/rust -f commits=200
//! ```
//!
//! Monitor:
//!
//! ```console
//! $ gh run list -w Replay --limit 5
//! $ gh run view --log-failed
//! ```

use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

use diffy::patch_set::{FileOperation, ParseOptions, PatchKind, PatchSet};
use rayon::prelude::*;

/// Persistent `git cat-file --batch` process for fast object lookups.
///
/// See <https://git-scm.com/docs/git-cat-file> for more.
struct CatFile {
    // Field order matters: stdin must drop (close) before child is reaped.
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    #[allow(dead_code)] // held for drop order: reaped after stdin closes
    child: std::process::Child,
}

impl CatFile {
    fn new(repo: &Path) -> Self {
        let mut child = Command::new("git")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .arg("-C")
            .arg(repo)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn git cat-file --batch");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            stdin,
            stdout,
            child,
        }
    }

    /// Look up an object by `<rev>:<path>`.
    ///
    /// Returns `None` for submodules, commit/tree/tag object types, and missing objects.
    fn get(&mut self, rev: &str, path: &str) -> Option<Vec<u8>> {
        writeln!(self.stdin, "{rev}:{path}").expect("cat-file stdin write failed");

        let mut header = String::new();
        self.stdout
            .read_line(&mut header)
            .expect("cat-file stdout read failed");

        // Response formats:
        //
        // * regular file: `<oid> blob <size>\n<content>\n`
        // * submodule:
        //   * `<oid> commit <size>\n<content>\n`
        //   * `<oid> submodule\n`
        // * not found: `<oid> missing\n`
        //
        // `tag` and `tree` object type are not relevant here.
        //
        // See <https://git-scm.com/docs/git-cat-file#_batch_output>

        let header = header.trim_end();
        let mut it = header.splitn(3, ' ');

        let Some(_oid) = it.next() else {
            panic!("unexpected cat-file header on {rev}: {header}");
        };
        let Some(ty) = it.next() else {
            panic!("unexpected cat-file header on {rev}: {header}");
        };

        // Types may have no `size` field, like "missing" or "submodule"
        let size: usize = it
            .next()?
            .parse()
            .unwrap_or_else(|e| panic!("invalid size in cat-file header on {rev}: {header}: {e}"));

        let mut buf = vec![0u8; size];
        self.stdout.read_exact(&mut buf).expect("short read");

        let mut nl = [0];
        self.stdout
            .read_exact(&mut nl)
            .expect("missing trailing LF");

        // Only blobs are regular file content
        if ty != "blob" {
            return None;
        }

        Some(buf)
    }

    /// Like [`CatFile::get`] but returns only UTF-8 string.
    fn get_text(&mut self, rev: &str, path: &str) -> Option<String> {
        self.get(rev, path).and_then(|b| String::from_utf8(b).ok())
    }
}

/// Local enum for test configuration (maps to ParseOptions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestMode {
    UniDiff,
    GitDiff,
}

impl From<TestMode> for ParseOptions {
    fn from(value: TestMode) -> Self {
        match value {
            TestMode::UniDiff => ParseOptions::unidiff(),
            TestMode::GitDiff => ParseOptions::gitdiff(),
        }
    }
}

/// Commit selection for replay testing.
enum CommitSelection {
    /// Last N commits from HEAD.
    Last(usize),
    /// Specific commit range (from..to).
    Range { from: String, to: String },
}

/// Result of processing a single commit pair.
struct CommitResult {
    parent_short: String,
    child_short: String,
    files: Vec<String>,
    applied: usize,
    skipped: usize,
}

/// Get the repository path from environment variable.
///
/// Defaults to package directory if `DIFFY_TEST_REPO` is not set.
fn repo_path() -> PathBuf {
    env::var("DIFFY_TEST_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn commit_selection() -> CommitSelection {
    let Ok(val) = env::var("DIFFY_TEST_COMMITS") else {
        return CommitSelection::Last(200);
    };
    let val = val.trim();

    // Check for range syntax (from..to)
    if let Some((from, to)) = val.split_once("..") {
        return CommitSelection::Range {
            from: from.to_string(),
            to: to.to_string(),
        };
    }

    // Parse as number
    if val == "0" {
        CommitSelection::Last(usize::MAX)
    } else {
        let n = val
            .parse()
            .unwrap_or_else(|e| panic!("invalid DIFFY_TEST_COMMITS='{val}': {e}"));
        CommitSelection::Last(n)
    }
}

fn test_mode() -> TestMode {
    let Ok(val) = env::var("DIFFY_TEST_PARSE_MODE") else {
        return TestMode::UniDiff;
    };
    match val.trim().to_lowercase().as_str() {
        "unidiff" => TestMode::UniDiff,
        "gitdiff" => TestMode::GitDiff,
        _ => panic!("invalid DIFFY_TEST_PARSE_MODE='{val}': expected 'unidiff' or 'gitdiff'"),
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new("git");
    cmd.env("GIT_CONFIG_NOSYSTEM", "1");
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.arg("-C").arg(repo);
    cmd.args(args);

    let output = cmd.output().expect("failed to execute git");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("git {args:?} failed: {stderr}");
    }

    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Get the list of commits from oldest to newest.
fn commit_history(repo: &Path, selection: &CommitSelection) -> Vec<String> {
    match selection {
        CommitSelection::Last(max) => {
            // We want newest N in chronological order, so: fetch newest, then reverse.
            // Use --first-parent to ensure consecutive commits are actual parent-child pairs,
            // not unrelated commits from different branches before a merge.
            let output = if *max == usize::MAX {
                git(repo, &["rev-list", "--first-parent", "--reverse", "HEAD"])
            } else {
                // fetches only the most recent `max + 1` commits
                // to have `max` commit pairs for diffing.
                let n = (max + 1).to_string();
                git(repo, &["rev-list", "--first-parent", "-n", &n, "HEAD"])
            };
            let mut commits: Vec<_> = output.lines().map(String::from).collect();
            if *max != usize::MAX {
                commits.reverse();
            }
            commits
        }
        CommitSelection::Range { from, to } => {
            let range = format!("{from}..{to}");
            let output = git(repo, &["rev-list", "--first-parent", "--reverse", &range]);
            let mut commits: Vec<_> = output.lines().map(String::from).collect();
            // Include 'from' commit as the base for diffing
            commits.insert(0, from.clone());
            commits
        }
    }
}

/// Check if a `git diff --raw` line is a type change (status `T`).
///
/// Type changes (e.g., symlink → regular file) produce two patches
/// (delete + create) but only one `--raw` line.
///
/// Example from llvm/llvm-project 3fa3e65d..caaaf2ee:
///
/// ```text
/// $ git diff --raw 3fa3e65d caaaf2ee
/// :120000 100644 ca10bf54 dda5db9c T    clang/tools/scan-build/c++-analyzer
/// :100755 100755 2b07d6b6 35f852e7 M    clang/tools/scan-build/scan-build
/// :000000 100644 00000000 77be6746 A    clang/tools/scan-build/scan-build.bat
/// ```
///
/// The `T` entry (symlink 120000 → regular file 100644) produces two
/// patches in `git diff` output, while `M` and `A` produce one each.
///
/// See <https://git-scm.com/docs/diff-format#_raw_output_format> for
/// the `--raw` format specification.
fn is_type_change(raw_line: &str) -> bool {
    // --raw format: `:old_mode new_mode old_hash new_hash status\tpath`
    raw_line
        .split('\t')
        .next()
        .is_some_and(|meta| meta.ends_with(" T"))
}

fn process_commit(
    cat: &mut CatFile,
    repo: &Path,
    parent: &str,
    child: &str,
    mode: TestMode,
) -> CommitResult {
    let parent_short = parent[..8].to_string();
    let child_short = child[..8].to_string();
    let mut files = Vec::new();
    let mut applied = 0;
    let mut skipped = 0;

    // UniDiff format cannot express pure renames (no ---/+++ headers).
    // Use `--no-renames` to represent them as delete + create instead.
    // GitDiff mode handles renames via extended headers natively.
    let diff_output = match mode {
        TestMode::UniDiff => git(repo, &["diff", "--no-renames", parent, child]),
        // TODO: pass `--binary` once binary patch support lands,
        // so binary files get actual delta/literal data instead of
        // "Binary files differ" markers.
        TestMode::GitDiff => git(repo, &["diff", parent, child]),
    };

    if diff_output.is_empty() {
        // No changes (could be metadata-only commit)
        return CommitResult {
            parent_short,
            child_short,
            files,
            applied,
            skipped,
        };
    }

    // Calculate expected file count BEFORE parsing.
    // This allows early return for binary-only commits.
    //
    // Type changes (status `T`, e.g., symlink → regular file) produce two
    // patches (delete + create) for one `--raw`/`--numstat` entry, so we
    // count them separately and add to the expected total.
    // See llvm/llvm-project commits 3fa3e65d..caaaf2ee, d069d2f6..3a7f73d9,
    // 2b08718b..06c93976 for examples.
    let expected_file_count = match mode {
        TestMode::UniDiff => {
            // Combine `--raw` and `--numstat` into a single git call.
            // Output: raw lines (start with `:`) followed by numstat lines.
            //
            // `--numstat` format:
            // - `added\tdeleted\tpath` for text files
            // - `-\t-\tpath` for binary files (skipped - no patch data in unidiff)
            // - `0\t0\tpath` for empty/no-content changes (skipped)
            let raw_numstat = git(
                repo,
                &["diff", "--raw", "--numstat", "--no-renames", parent, child],
            );
            let (mut type_changes, mut text_files) = (0, 0);
            for line in raw_numstat.lines().filter(|l| !l.is_empty()) {
                if line.starts_with(':') {
                    if is_type_change(line) {
                        type_changes += 1;
                    }
                } else if line.starts_with("-\t-\t") || line.starts_with("0\t0\t") {
                    skipped += 1;
                } else {
                    text_files += 1;
                }
            }
            text_files + type_changes
        }
        TestMode::GitDiff => {
            // Can't use `--numstat` for GitDiff: it shows `-\t-\t` for both
            // actual binary diffs AND pure binary renames (100% similarity).
            // Parser correctly handles pure renames (rename headers, no binary content).
            //
            // Use `--raw` for total count, subtract actual binary markers from diff.
            //
            // TODO: once `--binary` is passed above, count ALL `--raw`
            // entries — every file will have patch data (delta, literal, or text).
            let raw = git(repo, &["diff", "--raw", parent, child]);
            let (mut total, mut type_changes) = (0, 0);
            for line in raw.lines().filter(|l| !l.is_empty()) {
                total += 1;
                if is_type_change(line) {
                    type_changes += 1;
                }
            }
            let binary = diff_output
                .lines()
                .filter(|l| l.starts_with("Binary files ") || l.starts_with("GIT binary patch"))
                .count();
            skipped += binary;
            total - binary + type_changes
        }
    };

    if expected_file_count == 0 {
        return CommitResult {
            parent_short,
            child_short,
            files,
            applied,
            skipped,
        };
    }

    let patchset: Vec<_> = match PatchSet::parse(&diff_output, mode.into()).collect() {
        Ok(ps) => ps,
        Err(e) => {
            panic!(
                "Failed to parse patch for {parent_short}..{child_short}: {e}\n\n\
                Diff:\n{diff_output}"
            );
        }
    };

    // Verify we parsed the same number of patches as git reports files changed.
    // This catches both missing and spurious patches.
    if patchset.len() != expected_file_count {
        let n = patchset.len();
        panic!(
            "Patch count mismatch for {parent_short}..{child_short}: \
             expected {expected_file_count} files, parsed {n} patches\n\n\
             Diff:\n{diff_output}",
        );
    }

    for file_patch in patchset.iter() {
        // Paths from ---/+++ headers have a/b prefixes that need stripping.
        // Paths from git extended headers (rename/copy) are already clean.
        let operation = file_patch.operation();
        let strip = match &operation {
            FileOperation::Rename { .. } | FileOperation::Copy { .. } => 0,
            _ => 1,
        };
        let operation = operation.strip_prefix(strip);

        let (base_path, target_path, desc): (Option<&str>, Option<&str>, _) = match &operation {
            FileOperation::Create(path) => (None, Some(path.as_ref()), format!("create {path}")),
            FileOperation::Delete(path) => (Some(path.as_ref()), None, format!("delete {path}")),
            FileOperation::Modify { original, modified } => {
                let desc = if original == modified {
                    format!("modify {original}")
                } else {
                    format!("modify {original} -> {modified}")
                };
                (Some(original.as_ref()), Some(modified.as_ref()), desc)
            }
            FileOperation::Rename { from, to } => (
                Some(from.as_ref()),
                Some(to.as_ref()),
                format!("rename {from} -> {to}"),
            ),
            FileOperation::Copy { from, to } => (
                Some(from.as_ref()),
                Some(to.as_ref()),
                format!("copy {from} -> {to}"),
            ),
        };

        match file_patch.patch() {
            PatchKind::Text(patch) => {
                let base_content = if let Some(path) = base_path {
                    let Some(content) = cat.get_text(parent, path) else {
                        skipped += 1;
                        continue;
                    };
                    content
                } else {
                    String::new()
                };

                let expected_content = if let Some(path) = target_path {
                    let Some(content) = cat.get_text(child, path) else {
                        skipped += 1;
                        continue;
                    };
                    content
                } else {
                    String::new()
                };

                let result = match diffy::apply(&base_content, patch) {
                    Ok(r) => r,
                    Err(e) => {
                        panic!(
                            "Failed to apply patch at {parent_short}..{child_short} for {desc}: {e}\n\n\
                            Patch:\n{patch}\n\n\
                            Base content:\n{base_content}"
                        );
                    }
                };

                if result != expected_content {
                    panic!(
                        "Content mismatch at {parent_short}..{child_short} for {desc}\n\n\
                        --- Expected ---\n{expected_content}\n\n\
                        --- Got ---\n{result}\n\n\
                        --- Patch ---\n{patch}"
                    );
                }
            }
            PatchKind::Binary(_) => {
                // Binary patch application not yet wired up in replay tests.
                // Will be done once the `binary` Cargo feature is added.
                skipped += 1;
            }
        }

        applied += 1;
        files.push(desc);
    }

    CommitResult {
        parent_short,
        child_short,
        files,
        applied,
        skipped,
    }
}

// Ignored by default so `cargo test` stays fast; CI opts in via `--ignored`.
// Using `#[ignore]` instead of `test = false` keeps the file in clippy's
// `--all-targets` view so lints still fire here.
#[test]
#[ignore = "replay test runs git subprocesses; opt in via --ignored"]
fn replay() {
    let repo = repo_path();
    let selection = commit_selection();
    let mode = test_mode();
    let commits = commit_history(&repo, &selection);

    if commits.len() < 2 {
        panic!("Not enough commits to test");
    }

    let total_diffs = commits.len() - 1;
    let repo_name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string());
    let mode_name = match mode {
        TestMode::UniDiff => "unidiff",
        TestMode::GitDiff => "gitdiff",
    };

    // Shared state for progress reporting
    struct Progress {
        completed: usize,
        total_applied: usize,
        total_skipped: usize,
    }

    let progress = Mutex::new(Progress {
        completed: 0,
        total_applied: 0,
        total_skipped: 0,
    });

    (0..total_diffs).into_par_iter().for_each_init(
        || CatFile::new(&repo),
        |cat, i| {
            let result = process_commit(cat, &repo, &commits[i], &commits[i + 1], mode);

            let completed = {
                let mut p = progress.lock().unwrap();
                p.completed += 1;
                p.total_applied += result.applied;
                p.total_skipped += result.skipped;
                p.completed
            };

            eprintln!(
                "[{completed}/{total_diffs}] ({repo_name}, {mode_name}) Processing {}..{}",
                result.parent_short, result.child_short
            );
            for desc in &result.files {
                eprintln!("  ✓ {desc}");
            }
        },
    );

    let p = progress.lock().unwrap();
    eprintln!(
        "History replay completed: {} patches applied, {} skipped",
        p.total_applied, p.total_skipped
    );

    // Sanity check: we should have applied at least some patches
    assert!(p.total_applied > 0, "No patches were applied");
}
