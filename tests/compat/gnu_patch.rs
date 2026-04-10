//! GNU patch compatibility tests. See [`crate`] for test structure and usage.
//!
//! Focus areas:
//!
//! - UniDiff format edge cases (missing headers, reversed order)
//! - Agreement between diffy and `patch` command

use crate::common::Case;

// Success cases

#[test]
fn create_file() {
    Case::gnu_patch("create_file").run();
}

// GNU patch decodes C-style named escapes (\a, \b, \f, \v) in quoted
// filenames in ---/+++ headers.
//
// Observed with GNU patch 2.7.1:
//   $ patch -p0 < test.patch   # with +++ "bel\a"
//   patching file bel<BEL>
//
// diffy now decodes these correctly.
#[test]
fn path_quoted_named_escape() {
    Case::gnu_patch("path_quoted_named_escape").run();
}

// GNU patch decodes 3-digit octal escapes (\000–\377) in quoted filenames.
//
// Observed with GNU patch 2.7.1:
//   $ patch -p0 < test.patch   # with +++ "tl\033"
//   patching file tl<ESC>
//
// diffy currently misparsed \033: the \0 is consumed as a standalone NUL
// byte, leaving "33" as literal characters.
//
#[test]
fn path_quoted_octal_escape() {
    Case::gnu_patch("path_quoted_octal_escape").run();
}

#[test]
fn reversed_header_order() {
    Case::gnu_patch("reversed_header_order").run();
}

#[test]
fn missing_plus_header() {
    Case::gnu_patch("missing_plus_header").run();
}

#[test]
fn missing_minus_header() {
    Case::gnu_patch("missing_minus_header").run();
}

// Empty file creation using unified diff format with empty hunk.
//
// Platform compatibility:
// - Apple patch 2.0 (macOS/BSD): ✅ Accepts, creates empty file (0 bytes)
// - GNU patch 2.8 (Linux): ❌ Rejects as "malformed patch at line 3"
// - diffy: ✅ Accepts (with our current implementation)
#[test]
fn create_empty_file_unidiff() {
    Case::gnu_patch("create_empty_file_unidiff")
        .expect_compat(false)
        .run();
}

// Empty file creation using git diff format (no unified diff headers/hunks).
//
// - GNU patch: succeeds, creates empty file
// - diffy: fails (no ---/+++ headers means no valid UniDiff patches)
#[test]
fn create_empty_file_gitdiff() {
    Case::gnu_patch("create_empty_file_gitdiff")
        .strip(1)
        .expect_success(false)
        .expect_compat(false)
        .run();
}

#[test]
fn delete_file() {
    Case::gnu_patch("delete_file").run();
}

#[test]
fn preamble_git_headers() {
    Case::gnu_patch("preamble_git_headers").run();
}

// Multi-file patch with junk/preamble text between different files.
//
// GNU patch behavior: Treats content before `---` as "text leading up to"
// the next patch (preamble), which is silently ignored.
//
// Verified with:
// ```
// patch -p0 --dry-run --verbose < multi-file-junk.patch
// ```
// Output shows:
// ```
// Hmm...  The next patch looks like a unified diff to me...
// The text leading up to this was:
// --------------------------
// |JUNK BETWEEN FILES!!!!
// |This preamble text should be ignored
// ...
// ```
//
// This is different from junk between HUNKS of the same file (which fails).
#[test]
fn junk_between_files() {
    Case::gnu_patch("junk_between_files").run();
}

#[test]
fn trailing_signature() {
    Case::gnu_patch("trailing_signature").run();
}

// Patch that deletes a diff file containing `-- ` patterns within its content,
// followed by a real email signature at the end.
//
// This tests that we correctly distinguish between:
// - `-- ` appearing as patch content (from inner diff's empty context lines)
// - `-- ` appearing as the actual email signature separator
//
// Both GNU patch and git apply handle this correctly without pre-stripping.
#[test]
fn nested_diff_signature() {
    Case::gnu_patch("nested_diff_signature").strip(1).run();
}

// A hunk that adds a line whose content is literally "++ foo" renders in the
// diff as "+++ foo" (the leading "+" is the add marker). Both GNU patch and
// git apply parse this correctly as 2 patches without splitting the hunk.
#[test]
fn false_positive_plus_plus_in_hunk() {
    Case::gnu_patch("false_positive_plus_plus_in_hunk").run();
}

// Failure cases

#[test]
fn fail_context_mismatch() {
    Case::gnu_patch("fail_context_mismatch")
        .expect_success(false)
        .run();
}

#[test]
fn fail_hunk_not_found() {
    Case::gnu_patch("fail_hunk_not_found")
        .expect_success(false)
        .run();
}

#[test]
fn fail_truncated_file() {
    Case::gnu_patch("fail_truncated_file")
        .expect_success(false)
        .run();
}

// Single-file patch with junk between hunks.
//
// - GNU patch: succeeds, ignores trailing junk, applies first hunk only
// - git apply: errors ("patch fragment without header")
// - diffy: succeeds, matches GNU patch behavior
#[test]
fn junk_between_hunks() {
    Case::gnu_patch("junk_between_hunks").run();
}

// Patch with ---/+++ headers but no @@ hunks.
//
// - GNU patch: rejects ("Only garbage was found in the patch input")
// - diffy: succeeds, parses as 1 patch with 0 hunks
//
// diffy allows 0-hunk patches for GitDiff mode where empty/binary files have no hunks.
#[test]
fn no_hunk() {
    Case::gnu_patch("no_hunk")
        .expect_success(true)
        .expect_compat(false)
        .run();
}

// Both --- and +++ point to /dev/null.
// GNU patch rejects: "can't find file to patch" (exit 1)
#[test]
fn fail_both_devnull() {
    Case::gnu_patch("fail_both_devnull")
        .expect_success(false)
        .run();
}
