//! Tests for patchset parsing.

use super::{error::PatchSetParseErrorKind, FileOperation, PatchKind, ParseOptions, PatchSet};

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

mod patchset_unidiff_bytes {
    use super::*;
    use crate::patch::Line;

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

        let PatchKind::Text(patch) = patches[0].patch();
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
