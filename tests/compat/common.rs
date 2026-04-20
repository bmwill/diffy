//! Common utilities for compat tests.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::Once;

use diffy::binary::BinaryPatch;
use diffy::binary::BinaryPatchParseError;
use diffy::patch_set::FileOperation;
use diffy::patch_set::ParseOptions;
use diffy::patch_set::PatchKind;
use diffy::patch_set::PatchSet;
use diffy::patch_set::PatchSetParseError;

/// Which external tool to compare against.
#[derive(Clone, Copy)]
pub enum CompatMode {
    /// `git apply` with `ParseOptions::gitdiff()`
    Git,
    /// GNU `patch` with `ParseOptions::unidiff()`
    GnuPatch,
}

/// A test case with fluent builder API.
pub struct Case<'a> {
    case_name: &'a str,
    mode: CompatMode,
    /// Strip level for path prefixes (default: 0)
    strip_level: u32,
    /// Whether diffy is expected to succeed (default: true)
    expect_success: bool,
    /// Whether diffy and external tool should agree on success/failure (default: true)
    expect_compat: bool,
    /// Inline snapshot for diffy's error message on failure.
    expect_diffy_error: Option<snapbox::Data>,
    /// Inline snapshot for external tool's stderr on failure.
    expect_external_error: Option<snapbox::Data>,
}

impl<'a> Case<'a> {
    /// Create a test case for `git apply` comparison.
    pub fn git(name: &'a str) -> Self {
        Self {
            case_name: name,
            mode: CompatMode::Git,
            strip_level: 0,
            expect_success: true,
            expect_compat: true,
            expect_diffy_error: None,
            expect_external_error: None,
        }
    }

    /// Create a test case for GNU patch comparison.
    pub fn gnu_patch(name: &'a str) -> Self {
        Self {
            case_name: name,
            mode: CompatMode::GnuPatch,
            strip_level: 0,
            expect_success: true,
            expect_compat: true,
            expect_diffy_error: None,
            expect_external_error: None,
        }
    }

    /// Get the case directory path based on mode.
    fn case_dir(&self) -> PathBuf {
        let subdir = match self.mode {
            CompatMode::Git => "git",
            CompatMode::GnuPatch => "gnu_patch",
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compat")
            .join(subdir)
            .join(self.case_name)
    }

    pub fn strip(mut self, level: u32) -> Self {
        self.strip_level = level;
        self
    }

    pub fn expect_success(mut self, expect: bool) -> Self {
        self.expect_success = expect;
        self
    }

    pub fn expect_compat(mut self, expect: bool) -> Self {
        self.expect_compat = expect;
        self
    }

    /// Assert diffy's error message matches an inline snapshot.
    /// Use with [`snapbox::str!`].
    pub fn expect_diffy_error(mut self, expected: impl Into<snapbox::Data>) -> Self {
        self.expect_diffy_error = Some(expected.into());
        self
    }

    /// Assert external tool's stderr matches an inline snapshot.
    /// Use with [`snapbox::str!`].
    pub fn expect_external_error(mut self, expected: impl Into<snapbox::Data>) -> Self {
        self.expect_external_error = Some(expected.into());
        self
    }

    /// Run the test case.
    pub fn run(self) {
        let case_dir = self.case_dir();
        let in_dir = case_dir.join("in");
        let patch_path = in_dir.join("foo.patch");
        let patch = fs::read(&patch_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", patch_path.display()));

        let case_name = self.case_name;
        let prefix = match self.mode {
            CompatMode::Git => "git",
            CompatMode::GnuPatch => "gnu",
        };
        let temp_base = temp_base();

        let diffy_output = temp_base.join(format!("{prefix}-{case_name}-diffy"));
        create_output_dir(&diffy_output);

        let opts = match self.mode {
            CompatMode::Git => ParseOptions::gitdiff(),
            CompatMode::GnuPatch => ParseOptions::unidiff(),
        };

        // Apply with diffy
        let diffy_result = apply_diffy(&in_dir, &patch, &diffy_output, opts, self.strip_level);

        // Verify diffy result matches expectation
        if self.expect_success {
            diffy_result.as_ref().expect("diffy should succeed");
        } else {
            let err = diffy_result.as_ref().expect_err("diffy should fail");
            let expected = self
                .expect_diffy_error
                .as_ref()
                .expect("expect_diffy_error is required when expect_success(false)");
            snapbox::assert_data_eq!(err.to_string(), expected.clone());
        }

        // In CI mode, also verify external tool behavior
        if is_ci() {
            let external_output = temp_base.join(format!("{prefix}-{case_name}-external"));
            create_output_dir(&external_output);

            let external_result = match self.mode {
                CompatMode::Git => {
                    print_git_version();
                    git_apply(&external_output, &patch, self.strip_level, &in_dir)
                }
                CompatMode::GnuPatch => {
                    print_patch_version();
                    gnu_patch_apply(&in_dir, &patch_path, &external_output, self.strip_level)
                }
            };

            if let Err(stderr) = &external_result {
                let expected = self
                    .expect_external_error
                    .as_ref()
                    .expect("`expect_external_error` is required when the external tool fails");
                snapbox::assert_data_eq!(stderr.as_str(), expected.clone());
            }

            // For success cases where both succeed and are expected to be compatible,
            // verify outputs match
            if diffy_result.is_ok() && external_result.is_ok() && self.expect_compat {
                snapbox::assert_subset_eq(&external_output, &diffy_output);
            }

            // Verify agreement/disagreement based on expectation
            if self.expect_compat {
                assert_eq!(
                    diffy_result.is_ok(),
                    external_result.is_ok(),
                    "diffy and external tool disagree: diffy={diffy_result:?}, external={external_result:?}",
                );
            } else {
                assert_ne!(
                    diffy_result.is_ok(),
                    external_result.is_ok(),
                    "expected diffy and external tool to DISAGREE, but both returned same result: \
                     diffy={diffy_result:?}, external={external_result:?}",
                );
            }
        }

        // Compare against expected snapshot (only for success cases)
        if self.expect_success {
            snapbox::assert_subset_eq(case_dir.join("out"), &diffy_output);
        }
    }
}

// External tool invocations

fn gnu_patch_apply(
    in_dir: &Path,
    patch_path: &Path,
    output_dir: &Path,
    strip_level: u32,
) -> Result<(), String> {
    copy_input_files(in_dir, output_dir, &["patch"]);

    let output = Command::new("patch")
        .arg(format!("-p{strip_level}"))
        .arg("--force")
        .arg("--batch")
        .arg("--input")
        .arg(patch_path)
        .current_dir(output_dir)
        .output()
        .unwrap();

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "GNU patch failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn git_apply(
    output_dir: &Path,
    patch: &[u8],
    strip_level: u32,
    in_dir: &Path,
) -> Result<(), String> {
    copy_input_files(in_dir, output_dir, &["patch"]);

    let mut cmd = Command::new("git");
    cmd.env("GIT_CONFIG_NOSYSTEM", "1");
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.current_dir(output_dir);
    cmd.args(["apply", &format!("-p{strip_level}"), "-"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn git apply");
    child.stdin.as_mut().unwrap().write_all(patch).unwrap();

    let output = child.wait_with_output().unwrap();
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn print_git_version() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let output = Command::new("git").arg("--version").output();
        match output {
            Ok(o) if o.status.success() => {
                let version = String::from_utf8_lossy(&o.stdout);
                eprintln!(
                    "git version: {}",
                    version.lines().next().unwrap_or("unknown")
                );
            }
            Ok(o) => eprintln!("git --version failed: {}", o.status),
            Err(e) => eprintln!("git command not found: {e}"),
        }
    });
}

fn print_patch_version() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let output = Command::new("patch").arg("--version").output();
        match output {
            Ok(o) if o.status.success() => {
                let version = String::from_utf8_lossy(&o.stdout);
                eprintln!(
                    "patch version: {}",
                    version.lines().next().unwrap_or("unknown")
                );
            }
            Ok(o) => eprintln!("patch --version failed: {}", o.status),
            Err(e) => eprintln!("patch command not found: {e}"),
        }
    });
}

/// Error type for compat tests.
#[derive(Debug)]
pub enum TestError {
    Parse(PatchSetParseError),
    Apply(diffy::ApplyError),
    Binary(BinaryPatchParseError),
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Parse(e) => write!(f, "parse error: {e}"),
            TestError::Apply(e) => write!(f, "apply error: {e}"),
            TestError::Binary(e) => write!(f, "binary patch error: {e}"),
        }
    }
}

/// Get temp output directory base path.
pub fn temp_base() -> PathBuf {
    std::env::var("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Create a clean output directory.
pub fn create_output_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}

/// Copy files from src to dst, skipping files with given extensions.
pub fn copy_input_files(src: &Path, dst: &Path, skip_extensions: &[&str]) {
    copy_input_files_impl(src, dst, src, skip_extensions);
}

fn copy_input_files_impl(src: &Path, dst: &Path, base: &Path, skip_extensions: &[&str]) {
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        // Skip files with specified extensions
        if let Some(ext) = path.extension() {
            if skip_extensions.iter().any(|e| ext == *e) {
                continue;
            }
        }

        let rel_path = path.strip_prefix(base).unwrap();
        let target = dst.join(rel_path);

        if path.is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_input_files_impl(&path, dst, base, skip_extensions);
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::copy(&path, &target).unwrap();
        }
    }
}

fn bytes_to_path(b: &[u8]) -> &Path {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Path::new(std::ffi::OsStr::from_bytes(b))
    }
    #[cfg(not(unix))]
    {
        // On Windows, falls back to UTF-8 conversion since `OsStr` is WTF-16.
        Path::new(std::str::from_utf8(b).expect("non-UTF-8 path not supported on Windows"))
    }
}

/// Apply patch using diffy to output directory.
pub fn apply_diffy(
    in_dir: &Path,
    patch: &[u8],
    output_dir: &Path,
    opts: ParseOptions,
    strip_prefix: u32,
) -> Result<(), TestError> {
    let patches: Vec<_> = PatchSet::parse_bytes(patch, opts)
        .collect::<Result<_, _>>()
        .map_err(TestError::Parse)?;

    for file_patch in patches.iter() {
        let operation = file_patch.operation().strip_prefix(strip_prefix as usize);

        let (original_name, target_name): (Option<&[u8]>, &[u8]) = match &operation {
            FileOperation::Create(path) => (None, path.as_ref()),
            FileOperation::Delete(path) => (Some(path.as_ref()), path.as_ref()),
            FileOperation::Modify { original, modified } => {
                (Some(original.as_ref()), modified.as_ref())
            }
            FileOperation::Rename { from, to } | FileOperation::Copy { from, to } => {
                (Some(from.as_ref()), to.as_ref())
            }
        };

        let read_original = || {
            if let Some(name) = original_name {
                let original_path = in_dir.join(bytes_to_path(name));
                fs::read(&original_path).unwrap_or_default()
            } else {
                Vec::new()
            }
        };

        let write_modified = |result: &[u8]| {
            let result_path = output_dir.join(bytes_to_path(target_name));
            if let Some(parent) = result_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&result_path, result).unwrap();
        };

        match file_patch.patch() {
            PatchKind::Text(patch) => {
                let original = read_original();

                let result = diffy::apply_bytes(&original, patch).map_err(TestError::Apply)?;

                write_modified(&result);
            }
            PatchKind::Binary(BinaryPatch::Marker) => {
                // Dont do anything if it is just a binary patch marker.
            }
            PatchKind::Binary(patch) => {
                let original = read_original();

                let result = patch.apply(&original).map_err(TestError::Binary)?;

                write_modified(&result);
            }
        }
    }

    Ok(())
}

pub fn is_ci() -> bool {
    std::env::var("CI").is_ok()
}
