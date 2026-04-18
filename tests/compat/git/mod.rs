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

// Git uses `\"` inside quoted filenames for literal double-quote characters.
//
// Observed with git 2.53.0:
//   $ printf 'x' > 'with"quote.txt' && git add -A
//   $ git diff --cached | grep '+++'
//   +++ "b/with\"quote.txt"
#[test]
fn path_quoted_inner_quote() {
    Case::git("path_quoted_inner_quote").strip(1).run();
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

// Mbox stream: three concatenated `git format-patch` emails in one file.
// Each email has full headers, commit message, `---` separator, and signature.
// `git apply` splits on `diff --git` boundaries, ignoring inter-email content.
#[test]
fn format_patch_mbox() {
    Case::git("format_patch_mbox").strip(1).run();
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

// Ambiguous `diff --git` line where two splits produce the same suffix length.
// `diff --git a/x b/x c/x` (from `--dst-prefix='b/x c/'` on file `x`):
//   split at 3: a/x vs b/x c/x → suffix `x` (len 1)
//   split at 7: a/x b/x vs c/x → suffix `x` (len 1)
//
// diffy succeeds (picks first/leftmost split); git apply rejects.
#[test]
fn fail_ambiguous_suffix_tie() {
    Case::git("fail_ambiguous_suffix_tie")
        .strip(1)
        .expect_success(true)
        .expect_compat(false)
        .expect_external_error(snapbox::str![[r#"
error: git diff header lacks filename information when removing 1 leading pathname component (line 4)

"#]])
        .run();
}

// Both --- and +++ point to /dev/null.
#[test]
fn fail_both_devnull() {
    Case::git("fail_both_devnull")
        .strip(1)
        .expect_success(false)
        .expect_diffy_error(snapbox::str!["parse error: error parsing patches at byte 0: patch has both original and modified as /dev/null"])
        .expect_external_error(snapbox::str![[r#"
error: dev/null: No such file or directory

"#]])
        .run();
}

// Mixed quoted/unquoted paths in `diff --git` line and rename headers.
//
// Rename from a file with tab in its name (quoted) to a normal name (unquoted):
//   `diff --git "a/foo\tbar.txt" b/normal.txt`
//   `rename from "foo\tbar.txt"`
//   `rename to normal.txt`
#[test]
fn path_mixed_quoted() {
    Case::git("path_mixed_quoted").strip(1).run();
}

// Custom prefix with slash (e.g. `--src-prefix=src/ --dst-prefix=dst/`).
//
// Produces `diff --git src/old.txt dst/old.txt` and matching ---/+++ headers.
// Both git apply and diffy handle this correctly with strip(1).
#[test]
fn path_custom_prefix() {
    Case::git("path_custom_prefix").strip(1).run();
}

// Custom prefix without slash (e.g. `--src-prefix=foo --dst-prefix=bar`).
//
// Produces paths like `fooold.txt` / `barold.txt` with no `/` separator,
// making strip impossible. Both git apply and diffy fail.
#[test]
fn fail_prefix_no_slash() {
    Case::git("fail_prefix_no_slash")
        .strip(1)
        .expect_success(false)
        .expect_diffy_error(snapbox::str!["apply error: error applying hunk #1"])
        .expect_external_error(snapbox::str![[r#"
error: git diff header lacks filename information when removing 1 leading pathname component (line 5)

"#]])
        .run();
}

// Patch with non-UTF-8 bytes (0x80, 0xff) in hunk content.
// Both git apply and diffy handle raw bytes correctly.
#[test]
fn non_utf8_hunk_content() {
    Case::git("non_utf8_hunk_content").strip(1).run();
}

// Single-file patch with junk between hunks.
//
// diffy succeeds (ignores trailing junk, matches GNU patch); git apply rejects.
#[test]
fn junk_between_hunks() {
    Case::git("junk_between_hunks")
        .strip(1)
        .expect_compat(false)
        .expect_external_error(snapbox::str![[r#"
error: patch fragment without header at line 11: @@ -7,3 +7,3 @@

"#]])
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
fn binary_literal() {
    Case::git("binary_literal").strip(1).run();
}

// Binary patch in delta format (modify existing file).
#[test]
fn binary_delta() {
    Case::git("binary_delta").strip(1).run();
}

// Binary literal patch applied to wrong original content.
//
// diffy succeeds (literal format doesn't need the original); git rejects.
#[test]
fn binary_literal_wrong_original() {
    Case::git("binary_literal_wrong_original")
        .strip(1)
        .expect_compat(false)
        .expect_external_error(snapbox::str![[r#"
error: corrupt binary patch at line 9: 
error: No valid patches in input (allow with "--allow-empty")

"#]])
        .run();
}

// Declared literal size (99) doesn't match actual decompressed data (10 bytes).
// Both git apply and diffy reject this.
#[test]
fn binary_literal_size_mismatch() {
    Case::git("binary_literal_size_mismatch")
        .strip(1)
        .expect_success(false)
        .expect_diffy_error(snapbox::str![[
            "binary patch error: error parsing binary patch: decompressed size mismatch: expected 99, got 10"
        ]])
        .expect_external_error(snapbox::str![[r#"
error: corrupt binary patch at line 7: 
error: No valid patches in input (allow with "--allow-empty")

"#]])
        .run();
}

// Binary delta patch applied to wrong original content.
// Both diffy and git fail, but for different reasons (see snapshots).
#[test]
fn binary_delta_wrong_original() {
    Case::git("binary_delta_wrong_original")
        .strip(1)
        .expect_success(false)
        .expect_diffy_error(snapbox::str!["binary patch error: error parsing binary patch: original size mismatch: expected 5120, got 13"])
        .expect_external_error(snapbox::str![[r#"
error: the patch applies to 'large.bin' (0d6307ba5442d0fdfe89dd3d78f82604fe0c0d80), which does not match the current contents.
error: large.bin: patch does not apply

"#]])
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
fn binary_mixed_delta_literal() {
    Case::git("binary_mixed_delta_literal").strip(1).run();
}

// Binary delta with a zero control byte (0x00) in the instruction stream.
//
// Hand-crafted fixture: `hello` -> `hellX`. Forward delta instructions:
//
// - 0x05       orig_size = 5
// - 0x05       mod_size = 5
// - 0x91 0x00 0x04  COPY offset=0, len=4 ("hell")
// - 0x00       zero control byte
// - 0x01 0x58  ADD 1 byte: 'X'
#[test]
fn binary_delta_zero_control() {
    Case::git("binary_delta_zero_control")
        .strip(1)
        .expect_success(false)
        .expect_diffy_error(snapbox::str![[
            "binary patch error: error parsing binary patch: unexpected delta opcode 0"
        ]])
        .expect_external_error(snapbox::str![[r#"
error: unexpected delta opcode 0
error: binary patch does not apply to 'file.bin'
error: file.bin: patch does not apply

"#]])
        .run();
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
