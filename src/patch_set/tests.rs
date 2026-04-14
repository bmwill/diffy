//! Tests for patchset parsing.

use super::{error::PatchSetParseErrorKind, FileOperation, ParseOptions, PatchSet};

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
    use crate::patch_set::PatchKind;

    fn parse_gitdiff(input: &str) -> Vec<super::super::FilePatch<'_, str>> {
        PatchSet::parse(input, ParseOptions::gitdiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
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
        match patches[0].patch() {
            PatchKind::Text(p) => assert!(p.hunks().is_empty()),
            PatchKind::Binary(_) => panic!("expected text patch"),
        }
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
        assert!(patches[1].patch().as_text().is_some());
    }

    #[test]
    fn binary_skip_does_not_stall() {
        // Binary marker must be skipped and offset must advance
        // to the next patch without infinite loop.
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
        let patches: Vec<_> = PatchSet::parse(input, ParseOptions::gitdiff().skip_binary())
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1, "binary should be skipped, text parsed");
    }
}

mod patchset_parse_bytes {
    use super::*;

    #[test]
    fn non_utf8_hunk_content() {
        // Hunk lines contain invalid UTF-8 bytes (0x80, 0xff).
        let mut input: Vec<u8> = Vec::new();
        input.extend(b"--- a/file.bin\n");
        input.extend(b"+++ b/file.bin\n");
        input.extend(b"@@ -1,3 +1,3 @@\n");
        input.extend(b" valid ascii\n");
        input.extend(b"-old \x80\xff bytes\n");
        input.extend(b"+new \x80\xff bytes\n");
        input.extend(b" more ascii\n");

        let patches = PatchSet::parse_bytes(&input, ParseOptions::unidiff())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(patches.len(), 1);

        let patch = patches[0].patch().as_text().expect("expected text patch");
        let hunks = patch.hunks();
        assert_eq!(hunks.len(), 1);

        use crate::patch::Line;
        let lines = hunks[0].lines();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], Line::Context(b"valid ascii\n" as &[u8]));
        assert_eq!(lines[1], Line::Delete(b"old \x80\xff bytes\n" as &[u8]));
        assert_eq!(lines[2], Line::Insert(b"new \x80\xff bytes\n" as &[u8]));
        assert_eq!(lines[3], Line::Context(b"more ascii\n" as &[u8]));
    }

    #[test]
    fn non_utf8_path_returns_error() {
        // File paths (currently) must be valid UTF-8 even in byte mode.
        let mut input: Vec<u8> = Vec::new();
        input.extend(b"--- a/bad\xffpath\n");
        input.extend(b"+++ b/good\n");
        input.extend(b"@@ -1 +1 @@\n");
        input.extend(b"-old\n");
        input.extend(b"+new\n");

        let result: Result<Vec<_>, _> =
            PatchSet::parse_bytes(&input, ParseOptions::unidiff()).collect();
        let err = result.unwrap_err();
        assert_eq!(err.kind, PatchSetParseErrorKind::InvalidUtf8Path);
    }
}
