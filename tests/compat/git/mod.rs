//! Git compatibility tests. See [`crate`] for test structure and usage.
//!
//! Focus areas:
//!
//! - `diff --git` path parsing edge cases (quotes, spaces, ambiguous prefixes)
//! - `git format-patch` email format (preamble/signature stripping)
//! - Agreement between diffy and `git apply`

use crate::common::Case;

#[test]
fn path_no_prefix() {
    Case::git("path_no_prefix").run();
}

#[test]
fn path_quoted_escapes() {
    Case::git("path_quoted_escapes").strip(1).run();
}

// Git uses C-style named escapes (\a, \b, \f, \v) for certain control
// characters in quoted filenames. Both `git apply` and GNU patch decode
// these correctly.
//
// Observed with git 2.53.0:
//   $ printf 'x' > "$(printf 'bel\a')" && git add -A
//   $ git diff --cached | grep '+++'
//   +++ "b/bel\a"
//
// diffy now decodes these correctly.
#[test]
fn path_quoted_named_escape() {
    Case::git("path_quoted_named_escape").strip(1).run();
}

// Git uses 3-digit octal escapes (\000-\377) for bytes that don't have
// a named escape. Both `git apply` and GNU patch decode these correctly.
//
// Observed with git 2.53.0:
//   $ printf 'x' > "$(printf 'tl\033')" && git add -A
//   $ git diff --cached | grep '+++'
//   +++ "b/tl\033"
//
// Found via full-history replay test against llvm/llvm-project
// (commits 17af06ba..229c95ab, 6c031780..0683a1e5).
#[test]
fn path_quoted_octal_escape() {
    Case::git("path_quoted_octal_escape").strip(1).run();
}

#[test]
fn path_with_spaces() {
    Case::git("path_with_spaces").strip(1).run();
}

#[test]
fn path_containing_space_b() {
    Case::git("path_containing_space_b").strip(1).run();
}

#[test]
fn format_patch_preamble() {
    // Ambiguous: where does preamble end? First `\n---\n` - verify matches git
    Case::git("format_patch_preamble").strip(1).run();
}

#[test]
fn format_patch_diff_in_message() {
    // `diff --git` in commit message must NOT trigger early parsing
    Case::git("format_patch_diff_in_message").strip(1).run();
}

#[test]
fn format_patch_multiple_separators() {
    // Git uses first `\n---\n` as separator (observed git mailinfo behavior)
    Case::git("format_patch_multiple_separators").strip(1).run();
}

#[test]
fn format_patch_signature() {
    // Ambiguous: `\n-- \n` could appear in patch content - verify matches git
    Case::git("format_patch_signature").strip(1).run();
}

#[test]
fn nested_diff_signature() {
    // Patch that deletes a diff file containing `-- ` patterns within its content,
    // followed by a real email signature at the end.
    //
    // Tests that we correctly distinguish between:
    // - `-- ` appearing as patch content (from inner diff's empty context lines)
    // - `-- ` appearing as the actual email signature separator
    //
    // Both git apply and GNU patch handle this correctly.
    Case::git("nested_diff_signature").strip(1).run();
}

#[test]
fn path_ambiguous_suffix() {
    // Multiple valid splits in `diff --git` line; algorithm picks longest common suffix.
    // Tests the pathological case from parse.rs comments where custom prefix
    // creates `src/foo.rs src/foo.rs src/foo.rs src/foo.rs` - verify matches git.
    Case::git("path_ambiguous_suffix").strip(1).run();
}

// Both --- and +++ point to /dev/null.
// git apply rejects: "dev/null: No such file or directory"
#[test]
fn fail_both_devnull() {
    Case::git("fail_both_devnull")
        .strip(1)
        .expect_success(false)
        .run();
}

// Single-file patch with junk between hunks.
//
// - git apply: errors ("patch fragment without header")
// - diffy: succeeds, ignores trailing junk (matches GNU patch behavior)
#[test]
fn junk_between_hunks() {
    Case::git("junk_between_hunks")
        .strip(1)
        .expect_compat(false)
        .run();
}

// Mixed binary and text patch.
//
// Both git apply and diffy should apply both the binary and text changes.
#[test]
fn binary_and_text_mixed() {
    Case::git("binary_and_text_mixed").strip(1).run();
}

// Binary patch in literal format (new file creation).
#[test]
#[cfg(feature = "binary")]
fn binary_literal() {
    Case::git("binary_literal").strip(1).run();
}

// Binary patch in delta format (modify existing file).
#[test]
#[cfg(feature = "binary")]
fn binary_delta() {
    Case::git("binary_delta").strip(1).run();
}

// Binary literal patch applied to wrong original content.
//
// This documents a behavioral difference:
// - diffy: succeeds (skips validation, ignores original for literal format)
// - git: fails (validates original content via index hash before applying)
//
// diffy's behavior is intentional - we don't have access to git's object database
// to verify hashes, and for literal format the original content isn't needed anyway.
#[test]
#[cfg(feature = "binary")]
fn binary_literal_wrong_original() {
    Case::git("binary_literal_wrong_original")
        .strip(1)
        .expect_compat(false)
        .run();
}

// Binary delta patch applied to wrong original content.
//
// Both diffy and git fail, but for different reasons:
// - diffy: fails because delta instructions reference wrong offsets/sizes
// - git: fails because index hash doesn't match before even trying to apply
//
// This test verifies diffy correctly rejects invalid delta applications.
#[test]
#[cfg(feature = "binary")]
fn binary_delta_wrong_original() {
    Case::git("binary_delta_wrong_original")
        .strip(1)
        .expect_success(false)
        .run();
}

// Binary patch with mixed delta/literal format.
//
// Git can choose different encodings for forward and reverse transformations
// based on which is more efficient. This patch has:
// - forward (original -> modified): delta
// - reverse (modified -> original): literal
//
// From rust-lang/rust@ad46af24 (favicon-32x32.png update).
#[test]
#[cfg(feature = "binary")]
fn binary_mixed_delta_literal() {
    Case::git("binary_mixed_delta_literal").strip(1).run();
}

// Multi-file patch with junk/preamble text between different files.
//
// git apply behavior: Ignores content between `diff --git` boundaries.
// In GitDiff mode, splitting occurs at `diff --git`, so junk between
// files becomes trailing content of the previous chunk (harmless).
//
// This is different from junk between HUNKS of the same file (which fails).
#[test]
fn junk_between_files() {
    Case::git("junk_between_files").strip(1).run();
}
