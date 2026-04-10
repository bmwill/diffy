//! Common utilities for compat tests.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Once,
};

use diffy::patch_set::{FileOperation, ParseOptions, PatchKind, PatchSet, PatchSetParseError};

/// A test case with fluent builder API.
pub struct Case<'a> {
    case_name: &'a str,
    /// Strip level for path prefixes (default: 0)
    strip_level: u32,
    /// Whether diffy is expected to succeed (default: true)
    expect_success: bool,
    /// Whether diffy and external tool should agree on success/failure (default: true)
    expect_compat: bool,
}

impl<'a> Case<'a> {
    /// Create a test case for GNU patch comparison.
    pub fn gnu_patch(name: &'a str) -> Self {
        Self {
            case_name: name,
            strip_level: 0,
            expect_success: true,
            expect_compat: true,
        }
    }

    /// Get the case directory path.
    fn case_dir(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compat/gnu_patch")
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

    /// Run the test case.
    pub fn run(self) {
        let case_dir = self.case_dir();
        let in_dir = case_dir.join("in");
        let patch_path = in_dir.join("foo.patch");
        let patch = fs::read_to_string(&patch_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", patch_path.display()));

        let case_name = self.case_name;
        let temp_base = temp_base();

        let diffy_output = temp_base.join(format!("gnu-{case_name}-diffy"));
        create_output_dir(&diffy_output);

        let opts = ParseOptions::unidiff();

        // Apply with diffy
        let diffy_result = apply_diffy(&in_dir, &patch, &diffy_output, opts, self.strip_level);

        // Verify diffy result matches expectation
        if self.expect_success {
            diffy_result.as_ref().expect("diffy should succeed");
        } else {
            diffy_result.as_ref().expect_err("diffy should fail");
        }

        // In CI mode, also verify external tool behavior
        if is_ci() {
            let external_output = temp_base.join(format!("gnu-{case_name}-external"));
            create_output_dir(&external_output);

            print_patch_version();
            let external_result =
                gnu_patch_apply(&in_dir, &patch_path, &external_output, self.strip_level);

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
}

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestError::Parse(e) => write!(f, "parse error: {e}"),
            TestError::Apply(e) => write!(f, "apply error: {e}"),
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

/// Apply patch using diffy to output directory.
pub fn apply_diffy(
    in_dir: &Path,
    patch: &str,
    output_dir: &Path,
    opts: ParseOptions,
    strip_prefix: u32,
) -> Result<(), TestError> {
    let patches: Vec<_> = PatchSet::parse(patch, opts)
        .collect::<Result<_, _>>()
        .map_err(TestError::Parse)?;

    for file_patch in patches.iter() {
        let operation = file_patch.operation().strip_prefix(strip_prefix as usize);

        let (original_name, target_name) = match &operation {
            FileOperation::Create(path) => (None, path.as_ref()),
            FileOperation::Delete(path) => (Some(path.as_ref()), path.as_ref()),
            FileOperation::Modify { original, modified } => {
                (Some(original.as_ref()), modified.as_ref())
            }
            FileOperation::Rename { from, to } | FileOperation::Copy { from, to } => {
                (Some(from.as_ref()), to.as_ref())
            }
        };

        match file_patch.patch() {
            PatchKind::Text(patch) => {
                let original = if let Some(name) = original_name {
                    let original_path = in_dir.join(name);
                    fs::read_to_string(&original_path).unwrap_or_default()
                } else {
                    String::new()
                };

                let result = diffy::apply(&original, patch).map_err(TestError::Apply)?;

                let result_path = output_dir.join(target_name);
                if let Some(parent) = result_path.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&result_path, result.as_bytes()).unwrap();
            }
        }
    }

    Ok(())
}

pub fn is_ci() -> bool {
    std::env::var("CI").is_ok()
}
