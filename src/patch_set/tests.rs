//! Tests for patchset parsing.

use alloc::borrow::ToOwned;
use alloc::string::ToString;
use alloc::vec::Vec;

use super::FileOperation;
use super::ParseOptions;
use super::PatchKind;
use super::PatchSet;
use super::error::PatchSetParseErrorKind;
use crate::Patch;
use crate::binary::BinaryBlockKind;
use crate::binary::BinaryPatch;
use crate::patch::HunkRange;
use crate::patch::Line;

/// Asserts that `lf` parses to the same patches after every `\n` in it is
/// replaced by `\r\n`.
///
/// Hunk lines are compared without their line endings, because CRLF hunk
/// lines keep their `\r` as content (see `crlf::hunk_lines_keep_cr`).
fn assert_crlf_equivalent(lf: &str, opts: ParseOptions) {
    let crlf = lf.replace('\n', "\r\n");
    let lf_patches = PatchSet::parse(lf, opts.clone())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let crlf_patches = PatchSet::parse(&crlf, opts)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|e| panic!("the CRLF form of the input failed to parse: {e}"));

    assert_eq!(lf_patches.len(), crlf_patches.len(), "patch counts differ");
    for (lf, crlf) in lf_patches.iter().zip(&crlf_patches) {
        assert_eq!(lf.operation(), crlf.operation());
        assert_eq!(lf.old_mode(), crlf.old_mode());
        assert_eq!(lf.new_mode(), crlf.new_mode());
        match (lf.patch(), crlf.patch()) {
            (PatchKind::Text(lf), PatchKind::Text(crlf)) => {
                assert_eq!(lf.original(), crlf.original());
                assert_eq!(lf.modified(), crlf.modified());
                assert_eq!(
                    hunks_without_line_endings(lf),
                    hunks_without_line_endings(crlf)
                );
            }
            // Binary payloads are slices of the input that keep their line
            // endings, so compare what the headers declare.
            (PatchKind::Binary(lf), PatchKind::Binary(crlf)) => {
                assert_eq!(binary_blocks(lf), binary_blocks(crlf));
            }
            (lf, crlf) => panic!("patch kinds differ: {lf:?} vs {crlf:?}"),
        }
    }
}

type HunkWithoutLineEndings<'a> = (HunkRange, HunkRange, Option<&'a str>, Vec<Line<'a, str>>);

fn hunks_without_line_endings<'a>(patch: &'a Patch<'_, str>) -> Vec<HunkWithoutLineEndings<'a>> {
    let trim = |s: &'a str| s.trim_end_matches(['\r', '\n']);
    patch
        .hunks()
        .iter()
        .map(|hunk| {
            let lines = hunk
                .lines()
                .iter()
                .map(|line| match *line {
                    Line::Context(s) => Line::Context(trim(s)),
                    Line::Delete(s) => Line::Delete(trim(s)),
                    Line::Insert(s) => Line::Insert(trim(s)),
                })
                .collect();
            let function_context = hunk.function_context().map(trim);
            (hunk.old_range(), hunk.new_range(), function_context, lines)
        })
        .collect()
}

fn binary_blocks(patch: &BinaryPatch<'_>) -> Option<[(BinaryBlockKind, u64); 2]> {
    match patch {
        BinaryPatch::Full { forward, reverse } => Some([
            (forward.kind, forward.data.size),
            (reverse.kind, reverse.data.size),
        ]),
        BinaryPatch::Marker => None,
    }
}

mod file_operation {
    use super::*;

    #[test]
    fn test_strip_prefix() {
        let op = FileOperation::Modify {
            original: "a/src/lib.rs".to_owned().into(),
            modified: "b/src/lib.rs".to_owned().into(),
        };
        let stripped = op.strip_prefix(1);
        assert_eq!(
            stripped,
            FileOperation::Modify {
                original: "src/lib.rs".to_owned().into(),
                modified: "src/lib.rs".to_owned().into(),
            }
        );
    }

    #[test]
    fn test_strip_prefix_no_slash() {
        let op = FileOperation::Create("file.rs".to_owned().into());
        let stripped = op.strip_prefix(1);
        assert_eq!(stripped, FileOperation::Create("file.rs".to_owned().into()));
    }
}

mod patchset_unidiff {
    use super::*;

    #[test]
    fn single_file() {
        let content = "\
--- a/file.rs
+++ b/file.rs
@@ -1,3 +1,4 @@
 line1
 line2
+line3
 line4
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
    }

    #[test]
    fn multi_file() {
        let content = "\
--- a/file1.rs
+++ b/file1.rs
@@ -1 +1 @@
-old1
+new1
--- a/file2.rs
+++ b/file2.rs
@@ -1 +1 @@
-old2
+new2
";
        let patches: Vec<_> = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(patches.len(), 2);
        assert!(patches[0].operation().is_modify());
        assert!(patches[1].operation().is_modify());
    }

    #[test]
    fn with_preamble() {
        let content = "\
This is a preamble
It should be ignored
--- a/file.rs
+++ b/file.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
    }

    #[test]
    fn plus_plus_content_in_hunk() {
        // A hunk that adds a line whose content is literally "++ foo" renders
        // in the diff as "+++ foo" (the leading "+" is the add marker).
        // The parser must not treat this as a patch header boundary.
        let content = "\
--- a/file1.rs
+++ b/file1.rs
@@ -1,2 +1,2 @@
 line1
-old
+++ foo
--- a/file2.rs
+++ b/file2.rs
@@ -1 +1 @@
-a
+b
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 2);
    }

    #[test]
    fn false_positive_in_hunk() {
        // Line starting with "--- " inside hunk is not a patch boundary.
        let content = "\
--- a/file.rs
+++ b/file.rs
@@ -1,3 +1,3 @@
 line1
---- this is not a patch boundary
+--- this line starts with dashes
 line3
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
    }

    #[test]
    fn empty_content() {
        let err: Result<Vec<_>, _> = PatchSet::parse("", ParseOptions::unidiff()).collect();
        let err = err.unwrap_err();
        assert!(
            err.to_string().contains("no valid patches found"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn not_a_patch() {
        let content = "Some random text\nNo patches here\n";
        let err: Result<Vec<_>, _> = PatchSet::parse(content, ParseOptions::unidiff()).collect();
        let err = err.unwrap_err();
        assert!(
            err.to_string().contains("no valid patches found"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn incomplete_header() {
        // Has --- but no following +++ or @@.
        // parse_one treats it as a valid (header-only, no hunks) patch,
        // consistent with how GNU patch handles lone headers.
        let content = "\
--- a/file.rs
Some random text
No patches here
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
    }

    #[test]
    fn create_file() {
        let content = "\
--- /dev/null
+++ b/new.rs
@@ -0,0 +1 @@
+content
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_create());
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Create("b/new.rs".to_owned().into())
        );
    }

    #[test]
    fn delete_file() {
        let content = "\
--- a/old.rs
+++ /dev/null
@@ -1 +0,0 @@
-content
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_delete());
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Delete("a/old.rs".to_owned().into())
        );
    }

    #[test]
    fn different_paths() {
        let content = "\
--- a/old.rs
+++ b/new.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Modify {
                original: "a/old.rs".to_owned().into(),
                modified: "b/new.rs".to_owned().into(),
            }
        );
    }

    #[test]
    fn both_dev_null_error() {
        let content = "\
--- /dev/null
+++ /dev/null
@@ -1 +1 @@
-old
+new
";
        let result: Result<Vec<_>, _> = PatchSet::parse(content, ParseOptions::unidiff()).collect();
        assert_eq!(
            result.unwrap_err().kind,
            PatchSetParseErrorKind::BothDevNull
        );
    }

    #[test]
    fn error_advances_past_bad_patch() {
        // Iterator advances past a malformed patch and continues
        // to yield subsequent valid patches (GNU patch behavior).
        let content = "\
--- /dev/null
+++ /dev/null
@@ -1 +1 @@
-old
+new
--- a/file.rs
+++ b/file.rs
@@ -1 +1 @@
-old
+new
";
        let items: Vec<_> = PatchSet::parse(content, ParseOptions::unidiff()).collect();
        assert_eq!(items.len(), 2);
        assert!(items[0].is_err(), "first item should be the error");
        assert!(items[1].is_ok(), "second item should be the valid patch");
    }

    #[test]
    fn diff_git_ignored_in_unidiff_mode() {
        // In UniDiff mode, `diff --git` is noise before `---` boundary.
        let content = "\
diff --git a/file1.rs b/file1.rs
--- a/file1.rs
+++ b/file1.rs
@@ -1 +1 @@
-old1
+new1
diff --git a/file2.rs b/file2.rs
--- a/file2.rs
+++ b/file2.rs
@@ -1 +1 @@
-old2
+new2
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 2);
    }

    #[test]
    fn git_format_patch() {
        // Full git format-patch output with email headers and signature.
        let content = "\
From 1234567890abcdef1234567890abcdef12345678 Mon Sep 17 00:00:00 2001
From: Gandalf <gandalf@the.grey>
Date: Mon, 25 Mar 3019 00:00:00 +0000
Subject: [PATCH] fix!: destroy the one ring at mount doom

In a hole in the ground there lived a hobbit
---
 src/frodo.rs | 2 +-
 src/sam.rs   | 1 +
 2 files changed, 2 insertions(+), 1 deletion(-)

--- a/src/frodo.rs
+++ b/src/frodo.rs
@@ -1 +1 @@
-finger
+peace
--- a/src/sam.rs
+++ b/src/sam.rs
@@ -1 +1,2 @@
 food
+more food
--
2.40.0
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 2);
        assert!(patches[0].operation().is_modify());
        assert!(patches[1].operation().is_modify());
    }

    #[test]
    fn missing_modified_header() {
        // Only --- header, no +++ header.
        let content = "\
--- a/file.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
    }

    #[test]
    fn missing_original_header() {
        // Only +++ header, no --- header.
        let content = "\
+++ b/file.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
    }

    #[test]
    fn reversed_header_order() {
        // +++ before ---.
        let content = "\
+++ b/file.rs
--- a/file.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
    }

    #[test]
    fn multi_file_mixed_headers() {
        // Various combinations of missing headers.
        let content = "\
--- a/file1.rs
+++ b/file1.rs
@@ -1 +1 @@
-old1
+new1
--- a/file2.rs
@@ -1 +1 @@
-old2
+new2
+++ b/file3.rs
@@ -1 +1 @@
-old3
+new3
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 3);
    }

    #[test]
    fn missing_modified_uses_original() {
        // When +++ is missing, original path is used for both.
        let content = "\
--- a/file.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Modify {
                original: "a/file.rs".to_owned().into(),
                modified: "a/file.rs".to_owned().into(),
            }
        );
    }

    #[test]
    fn missing_original_uses_modified() {
        // When --- is missing, modified path is used for both.
        let content = "\
+++ b/file.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse(content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Modify {
                original: "b/file.rs".to_owned().into(),
                modified: "b/file.rs".to_owned().into(),
            }
        );
    }

    #[test]
    fn hunk_only_no_headers() {
        // Only @@ header, no --- or +++ paths.
        // is_unidiff_boundary requires --- or +++ to identify patch start,
        // so this is not recognized as a patch at all.
        let content = "\
@@ -1 +1 @@
-old
+new
";
        let err: Result<Vec<_>, _> = PatchSet::parse(content, ParseOptions::unidiff()).collect();
        let err = err.unwrap_err();
        assert!(
            err.to_string().contains("no valid patches found"),
            "unexpected error: {}",
            err
        );
    }
}

mod patchset_gitdiff {
    use super::*;

    /// Parses `input`, and also checks that its CRLF form parses the same.
    fn parse_gitdiff(input: &str) -> Vec<super::super::FilePatch<'_, str>> {
        assert_crlf_equivalent(input, ParseOptions::gitdiff());
        PatchSet::parse(input, ParseOptions::gitdiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    /// Paths sharing trailing UTF-8 continuation bytes
    /// but belonging to different codepoints must not panic
    #[test]
    fn multibyte_char_boundary_in_diff_git_path() {
        // U+00BF INVERTED QUESTION MARK (¿) = 0xC2 0xBF
        // U+00FF LATIN SMALL LETTER Y WITH DIAERESIS (ÿ) = 0xC3 0xBF
        // Both share trailing byte 0xBF.
        let input = "diff --git a/\u{bf} b/\u{ff}\n";
        let result: Result<Vec<_>, _> = PatchSet::parse(input, ParseOptions::gitdiff()).collect();
        assert_eq!(
            result.unwrap_err().kind,
            PatchSetParseErrorKind::InvalidDiffGitPath
        );
    }

    /// `parse_one` must stop at `diff --git` boundaries so that
    /// back-to-back patches are split correctly.
    /// Without this, the second patch's `diff --git` line would be
    /// swallowed as trailing junk by the first patch's hunk parser.
    #[test]
    fn multi_file_stops_at_diff_git_boundary() {
        let input = "\
diff --git a/foo b/foo
--- a/foo
+++ b/foo
@@ -1 +1 @@
-old foo
+new foo
diff --git a/bar b/bar
--- a/bar
+++ b/bar
@@ -1 +1 @@
-old bar
+new bar
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 2);
    }

    #[test]
    fn pure_rename() {
        let input = "\
diff --git a/old.rs b/new.rs
similarity index 100%
rename from old.rs
rename to new.rs
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Rename {
                from: "old.rs".into(),
                to: "new.rs".into(),
            }
        );
    }

    /// Empty file creation has no ---/+++ headers, so the path comes
    /// from the `diff --git` line and retains the `b/` prefix.
    /// Callers use `strip_prefix(1)` to remove it.
    #[test]
    fn new_empty_file() {
        let input = "\
diff --git a/empty b/empty
new file mode 100644
index 0000000..e69de29
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Create("b/empty".into())
        );
        let p = patches[0].patch().as_text().unwrap();
        assert!(p.hunks().is_empty());
    }

    #[test]
    fn rename_then_modify() {
        // Rename with no hunks followed by a modify with hunks.
        // Tests that offset advances correctly across both.
        let input = "\
diff --git a/old.rs b/new.rs
similarity index 100%
rename from old.rs
rename to new.rs
diff --git a/foo b/foo
--- a/foo
+++ b/foo
@@ -1 +1 @@
-old
+new
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 2);
        assert!(matches!(
            patches[0].operation(),
            FileOperation::Rename { .. }
        ));
        assert!(matches!(
            patches[1].operation(),
            FileOperation::Modify { .. }
        ));
    }

    /// Quoted path containing an escaped quote (`\"`).
    /// Git produces this for filenames with literal double quotes.
    ///
    /// Observed with git 2.53.0:
    ///   $ printf 'x' > 'with"quote' && git add -A
    ///   $ git diff --cached | head -1
    ///   diff --git "a/with\"quote" "b/with\"quote"
    #[test]
    fn path_quoted_with_escaped_quote() {
        let input = "\
diff --git \"a/with\\\"quote\" \"b/with\\\"quote\"
--- \"a/with\\\"quote\"
+++ \"b/with\\\"quote\"
@@ -1 +1 @@
-old
+new
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Modify {
                original: "a/with\"quote".to_owned().into(),
                modified: "b/with\"quote".to_owned().into(),
            }
        );
    }

    /// Copy operation extracted from git extended headers.
    #[test]
    fn copy_operation() {
        let input = "\
diff --git a/original.rs b/copied.rs
similarity index 100%
copy from original.rs
copy to copied.rs
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Copy {
                from: "original.rs".into(),
                to: "copied.rs".into(),
            }
        );
    }

    /// Rename with both paths quoted (escapes in both).
    #[test]
    fn rename_both_quoted() {
        let input = "\
diff --git \"a/foo\\tbar.rs\" \"b/baz\\tqux.rs\"
similarity index 100%
rename from \"foo\\tbar.rs\"
rename to \"baz\\tqux.rs\"
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Rename {
                from: "foo\tbar.rs".into(),
                to: "baz\tqux.rs".into(),
            }
        );
    }

    /// Rename from quoted (has escape) to unquoted (plain).
    #[test]
    fn rename_quoted_to_unquoted() {
        let input = "\
diff --git \"a/foo\\tbar.rs\" b/normal.rs
similarity index 100%
rename from \"foo\\tbar.rs\"
rename to normal.rs
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Rename {
                from: "foo\tbar.rs".into(),
                to: "normal.rs".into(),
            }
        );
    }

    /// Rename from unquoted to quoted (has escape).
    #[test]
    fn rename_unquoted_to_quoted() {
        let input = "\
diff --git a/normal.rs \"b/foo\\tbar.rs\"
similarity index 100%
rename from normal.rs
rename to \"foo\\tbar.rs\"
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Rename {
                from: "normal.rs".into(),
                to: "foo\tbar.rs".into(),
            }
        );
    }

    /// Deleted file: `deleted file mode` header + /dev/null in +++.
    #[test]
    fn deleted_file_with_mode() {
        let input = "\
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
index abc1234..0000000
--- a/gone.rs
+++ /dev/null
@@ -1 +0,0 @@
-content
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_delete());
        assert_eq!(
            patches[0].old_mode(),
            Some(&super::super::FileMode::Regular)
        );
    }

    /// Mode-only change: no hunks, no ---/+++ headers.
    /// File operation falls back to `diff --git` line paths.
    #[test]
    fn mode_only_change() {
        let input = "\
diff --git a/script.sh b/script.sh
old mode 100644
new mode 100755
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
        assert_eq!(
            patches[0].old_mode(),
            Some(&super::super::FileMode::Regular),
        );
        assert_eq!(
            patches[0].new_mode(),
            Some(&super::super::FileMode::Executable),
        );
        let p = patches[0].patch().as_text().unwrap();
        assert!(p.hunks().is_empty());
    }

    /// New file with content: `new file mode` header + /dev/null in ---.
    #[test]
    fn new_file_with_content() {
        let input = "\
diff --git a/new.rs b/new.rs
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/new.rs
@@ -0,0 +1 @@
+hello
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_create());
        assert_eq!(
            patches[0].new_mode(),
            Some(&super::super::FileMode::Regular),
        );
    }

    /// `diff --git` line with no-prefix paths (`git diff --no-prefix`).
    /// Fallback path parsing works when ---/+++ are absent.
    #[test]
    fn no_prefix_empty_file() {
        let input = "\
diff --git file.rs file.rs
new file mode 100644
index 0000000..e69de29
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_create());
    }

    #[test]
    fn binary_marker_kept_by_default() {
        // Default is Keep: binary marker is returned as BinaryPatch::Marker.
        let input = "\
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
diff --git a/foo b/foo
--- a/foo
+++ b/foo
@@ -1 +1 @@
-old
+new
";
        let patches = parse_gitdiff(input);
        assert_eq!(patches.len(), 2);
        assert!(patches[0].patch().as_binary().is_some());
        assert!(patches[0].operation().is_modify());
        assert!(patches[1].patch().as_text().is_some());
    }
}

mod patchset_unidiff_bytes {
    use super::*;

    #[test]
    fn single_file_bytes() {
        let content = b"\
--- a/file.rs
+++ b/file.rs
@@ -1 +1 @@
-old
+new
";
        let patches = PatchSet::parse_bytes(content.as_slice(), ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_modify());
    }

    #[test]
    fn non_utf8_hunk_content() {
        // Simulate a patch where hunk content has non-UTF-8 bytes.
        // This is the primary use case for parse_bytes: git may produce
        // text-format hunks for files it misdetects as text (e.g. small
        // PNGs without NUL bytes).
        let mut content = Vec::new();
        content.extend_from_slice(b"--- a/icon.png\n");
        content.extend_from_slice(b"+++ b/icon.png\n");
        content.extend_from_slice(b"@@ -1 +1 @@\n");
        content.extend_from_slice(b"-old\x89PNG\n");
        content.extend_from_slice(b"+new\x89PNG\n");

        let patches = PatchSet::parse_bytes(&content, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);

        let patch = patches[0].patch().as_text().unwrap();
        let lines = patch.hunks()[0].lines();
        assert_eq!(lines[0], Line::Delete(b"old\x89PNG\n".as_slice()));
        assert_eq!(lines[1], Line::Insert(b"new\x89PNG\n".as_slice()));
    }

    #[test]
    fn multi_file_bytes() {
        let content = b"\
--- a/file1.rs
+++ b/file1.rs
@@ -1 +1 @@
-old1
+new1
--- a/file2.rs
+++ b/file2.rs
@@ -1 +1 @@
-old2
+new2
";
        let patches = PatchSet::parse_bytes(content.as_slice(), ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 2);
    }

    #[test]
    fn create_file_bytes() {
        let content = b"\
--- /dev/null
+++ b/new.rs
@@ -0,0 +1 @@
+content
";
        let patches = PatchSet::parse_bytes(content.as_slice(), ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_create());
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Create(b"b/new.rs".to_vec().into())
        );
    }

    #[test]
    fn delete_file_bytes() {
        let content = b"\
--- a/old.rs
+++ /dev/null
@@ -1 +0,0 @@
-content
";
        let patches = PatchSet::parse_bytes(content.as_slice(), ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].operation().is_delete());
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Delete(b"a/old.rs".to_vec().into())
        );
    }
}

/// Patches with CRLF line endings, as written by Windows tools or by a git
/// checkout with `core.autocrlf` enabled.
mod crlf {
    use super::*;
    use crate::patch_set::FileMode;

    /// A `git format-patch` email whose commit message has a line starting
    /// with `diff --git`. Only the `---` line ends the commit message.
    const FORMAT_PATCH: &str = "\
From 1234567890abcdef1234567890abcdef12345678 Mon Sep 17 00:00:00 2001
From: Gandalf <gandalf@the.grey>
Date: Mon, 25 Mar 3019 00:00:00 +0000
Subject: [PATCH] fix the diff header parser

This message line is not a patch header:
diff --git a/phantom.rs b/phantom.rs
---
 src/frodo.rs | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

diff --git a/src/frodo.rs b/src/frodo.rs
index 1111111..2222222 100644
--- a/src/frodo.rs
+++ b/src/frodo.rs
@@ -1 +1 @@
-finger
+peace
-- 
2.40.0
";

    #[test]
    fn format_patch_gitdiff() {
        let input = FORMAT_PATCH.replace('\n', "\r\n");
        let patches = PatchSet::parse(&input, ParseOptions::gitdiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Modify {
                original: "a/src/frodo.rs".into(),
                modified: "b/src/frodo.rs".into(),
            }
        );
        assert_crlf_equivalent(FORMAT_PATCH, ParseOptions::gitdiff());
    }

    #[test]
    fn format_patch_unidiff() {
        assert_crlf_equivalent(FORMAT_PATCH, ParseOptions::unidiff());
    }

    /// Hunk lines keep their `\r` as content, matching `git apply`. The
    /// `\ No newline at end of file` marker removes only the `\n`, so the `\r`
    /// before it stays too: `git apply` matches such a line against a file
    /// ending in `old\r`.
    #[test]
    fn hunk_lines_keep_cr() {
        let input = "\
--- a/f
+++ b/f
@@ -1,2 +1,2 @@
 keep
-old
\\ No newline at end of file
+new
\\ No newline at end of file
"
        .replace('\n', "\r\n");
        let patches = PatchSet::parse(&input, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let patch = patches[0].patch().as_text().unwrap();
        assert_eq!(
            patch.hunks()[0].lines(),
            [
                Line::Context("keep\r\n"),
                Line::Delete("old\r"),
                Line::Insert("new\r"),
            ]
        );
    }

    #[test]
    fn rename_with_mode_change_bytes() {
        let input = b"\
diff --git a/old.sh b/new.sh\r
old mode 100644\r
new mode 100755\r
similarity index 100%\r
rename from old.sh\r
rename to new.sh\r
";
        let patches = PatchSet::parse_bytes(input, ParseOptions::gitdiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(
            patches[0].operation(),
            &FileOperation::Rename {
                from: b"old.sh".as_slice().into(),
                to: b"new.sh".as_slice().into(),
            }
        );
        assert_eq!(patches[0].old_mode(), Some(&FileMode::Regular));
        assert_eq!(patches[0].new_mode(), Some(&FileMode::Executable));
    }

    /// A `\r` that no `\n` follows is not a line ending, so it stays part of
    /// the header value.
    #[test]
    fn lone_cr_is_not_a_line_ending() {
        let input = "\
diff --git a/x b/x
old mode 100644
new mode 100755\r";
        let err = PatchSet::parse(input, ParseOptions::gitdiff())
            .next()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            err.kind,
            PatchSetParseErrorKind::InvalidFileMode("100755\r".to_owned())
        );
    }
}
