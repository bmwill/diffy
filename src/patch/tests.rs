use super::error::ParsePatchErrorKind;
use super::parse::parse;
use super::parse::parse_bytes;
use super::parse::parse_bytes_strict;
use super::parse::parse_strict;
use alloc::format;
use alloc::string::ToString;

#[test]
fn trailing_garbage_after_complete_hunk() {
    let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old line
+new line
this is trailing garbage
that should be ignored
";
    let patch = parse(s).unwrap();
    assert_eq!(patch.hunks().len(), 1);
    assert_eq!(patch.hunks()[0].old_range().len(), 1);
    assert_eq!(patch.hunks()[0].new_range().len(), 1);
}

#[test]
fn garbage_before_hunk_complete_fails() {
    // If hunk line count isn't satisfied, garbage causes error
    let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
-line 1
+LINE 1
garbage before hunk complete
 line 3
";
    assert_eq!(
        parse(s).unwrap_err().kind,
        ParsePatchErrorKind::UnexpectedHunkLine,
    );
}

#[test]
fn git_headers_after_hunk_ignored() {
    // Git extended headers appearing after a complete hunk should be ignored
    let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
diff --git a/other.txt b/other.txt
index 1234567..89abcdef 100644
";
    let patch = parse(s).unwrap();
    assert_eq!(patch.hunks().len(), 1);
}

/// When splitting multi-patch input by `---/+++` boundaries, trailing
/// `diff --git` lines from the next patch may linger. If the last hunk
/// ends with `\ No newline at end of file`, the parser should still
/// recognize the hunk as complete and ignore the trailing content,
/// as GNU patch does.
///
/// Pattern first appeared in rust-lang/cargo@b119b891df93f128abef634215cd8f967c3cd120
/// where HTML files lost their trailing newlines.
#[test]
fn no_newline_at_eof_followed_by_trailing_garbage() {
    let s = "\
--- a/file.html
+++ b/file.html
@@ -1,3 +1,3 @@
 <div>
-<p>old</p>
+<p>new</p>
 </div>
\\ No newline at end of file
diff --git a/other.html b/other.html
index 1234567..89abcdef 100644
";
    let patch = parse(s).unwrap();
    assert_eq!(patch.hunks().len(), 1);
    assert_eq!(patch.hunks()[0].old_range().len(), 3);
    assert_eq!(patch.hunks()[0].new_range().len(), 3);
}

#[test]
fn multi_hunk_with_trailing_garbage() {
    let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-a
+A
@@ -5 +5 @@
-b
+B
some trailing garbage
";
    let patch = parse(s).unwrap();
    assert_eq!(patch.hunks().len(), 2);
}

#[test]
fn garbage_between_hunks_stops_parsing() {
    // GNU patch would try to parse the second @@ as a new patch
    // and fail because there's no `---` header.
    //
    // diffy `Patch` is a single patch parser, so should just ignore everything
    // after the first complete hunk when garbage is encountered.
    let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-a
+A
not a hunk line
@@ -5 +5 @@
-b
+B
";
    let patch = parse(s).unwrap();
    // Only first hunk is parsed; second @@ is ignored as garbage
    assert_eq!(patch.hunks().len(), 1);
}

#[test]
fn context_lines_counted_correctly() {
    let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1,4 +1,4 @@
 context 1
-deleted
+inserted
 context 2
 context 3
trailing garbage
";
    let patch = parse(s).unwrap();
    assert_eq!(patch.hunks().len(), 1);
    assert_eq!(patch.hunks()[0].old_range().len(), 4);
    assert_eq!(patch.hunks()[0].new_range().len(), 4);
}

// Strict mode (git-apply behavior): rejects orphaned hunk headers
// hidden behind trailing content, but allows plain trailing junk.
mod strict_mode {
    use super::*;

    #[test]
    fn trailing_junk_allowed() {
        // git apply accepts trailing junk after all hunks
        let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
this is trailing garbage
";
        let patch = parse_strict(s).unwrap();
        assert_eq!(patch.hunks().len(), 1);
    }

    #[test]
    fn trailing_junk_allowed_bytes() {
        let s = b"\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
this is trailing garbage
";
        let patch = parse_bytes_strict(&s[..]).unwrap();
        assert_eq!(patch.hunks().len(), 1);
    }

    #[test]
    fn orphaned_hunk_header_after_junk() {
        // Junk between hunks hides the second @@ — strict rejects this
        // since git apply errors with "patch fragment without header".
        let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-a
+A
not a hunk line
@@ -5 +5 @@
-b
+B
";
        assert_eq!(
            parse_strict(s).unwrap_err().kind,
            ParsePatchErrorKind::OrphanedHunkHeader,
        );
    }

    #[test]
    fn no_junk_parses_normally() {
        let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let patch = parse_strict(s).unwrap();
        assert_eq!(patch.hunks().len(), 1);
    }

    #[test]
    fn multi_hunk_no_junk() {
        let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-a
+A
@@ -5 +5 @@
-b
+B
";
        let patch = parse_strict(s).unwrap();
        assert_eq!(patch.hunks().len(), 2);
    }

    #[test]
    fn garbage_before_hunk_complete_fails() {
        let s = "\
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
-line 1
+LINE 1
garbage before hunk complete
 line 3
";
        assert_eq!(
            parse_strict(s).unwrap_err().kind,
            ParsePatchErrorKind::UnexpectedHunkLine,
        );
    }
}

#[test]
fn test_escaped_filenames() {
    // No escaped characters
    let s = "\
--- original
+++ modified
@@ -1,0 +1,1 @@
+Oathbringer
";
    parse(s).unwrap();
    parse_bytes(s.as_ref()).unwrap();

    // unescaped characters fail parsing
    let s = "\
--- ori\"ginal
+++ modified
@@ -1,0 +1,1 @@
+Oathbringer
";
    assert_eq!(
        parse(s).unwrap_err().kind,
        ParsePatchErrorKind::InvalidCharInUnquotedFilename,
    );
    parse_bytes(s.as_ref()).unwrap_err();

    // quoted with invalid escaped characters
    let s = "\
--- \"ori\\\"g\rinal\"
+++ modified
@@ -1,0 +1,1 @@
+Oathbringer
";
    assert_eq!(
        parse(s).unwrap_err().kind,
        ParsePatchErrorKind::InvalidUnescapedChar,
    );
    parse_bytes(s.as_ref()).unwrap_err();

    // quoted with escaped characters
    let s = r#"\
--- "ori\"g\tinal"
+++ "mo\000\t\r\n\\dified"
@@ -1,0 +1,1 @@
+Oathbringer
"#;
    let p = parse(s).unwrap();
    assert_eq!(p.original(), Some("ori\"g\tinal"));
    assert_eq!(p.modified(), Some("mo\0\t\r\n\\dified"));
    let b = parse_bytes(s.as_ref()).unwrap();
    assert_eq!(b.original(), Some(&b"ori\"g\tinal"[..]));
    assert_eq!(b.modified(), Some(&b"mo\0\t\r\n\\dified"[..]));
}

// Git uses named escapes \a (BEL), \b (BS), \f (FF), \v (VT) in
// quoted filenames. Both `git apply` and GNU patch decode them.
//
// Observed with git 2.53.0:
//   $ printf 'x' > "$(printf 'f\x07')" && git add -A
//   $ git diff --cached --name-only
//   "f\a"
//
// Observed with GNU patch 2.7.1:
//   $ patch -p0 < test.patch   # with +++ "bel\a"
//   patching file bel<BEL>
//
#[test]
fn escaped_filename_named_escapes() {
    let cases: &[(&str, u8)] = &[
        ("\\a", b'\x07'),
        ("\\b", b'\x08'),
        ("\\f", b'\x0c'),
        ("\\v", b'\x0b'),
    ];
    for (esc, expected_byte) in cases {
        let s = format!(
            "\
--- \"orig{esc}\"
+++ \"mod{esc}\"
@@ -1,0 +1,1 @@
+content
"
        );
        let p = parse(&s).unwrap();
        let expected_orig = format!("orig{}", *expected_byte as char);
        let expected_mod = format!("mod{}", *expected_byte as char);
        assert_eq!(p.original(), Some(expected_orig.as_str()));
        assert_eq!(p.modified(), Some(expected_mod.as_str()));
    }
}

// Git uses 3-digit octal escapes (\000–\377) for bytes without a
// named escape. Both `git apply` and GNU patch decode them.
//
// Observed with git 2.53.0:
//   $ printf 'x' > "$(printf 'f\033')" && git add -A
//   $ git diff --cached | grep '+++'
//   +++ "b/f\033"
//
// Observed with GNU patch 2.7.1:
//   $ patch -p1 < test.patch   # with +++ "b/tl\033"
//   patching file tl<ESC>
//
// Found via llvm/llvm-project full-history replay
// (commits 17af06ba..229c95ab, 6c031780..0683a1e5).
#[test]
fn escaped_filename_octal() {
    // \033 = ESC (0x1B)
    let s = r#"\
--- "orig\033"
+++ "mod\033"
@@ -1,0 +1,1 @@
+content
"#;
    let p = parse(s).unwrap();
    assert_eq!(p.original(), Some("orig\x1b"));
    assert_eq!(p.modified(), Some("mod\x1b"));

    // \000 = NUL
    let s = r#"\
--- "orig\000"
+++ "mod\000"
@@ -1,0 +1,1 @@
+content
"#;
    let p = parse(s).unwrap();
    assert_eq!(p.original(), Some("orig\x00"));
    assert_eq!(p.modified(), Some("mod\x00"));

    // \177 = DEL (0x7F)
    let s = r#"\
--- "orig\177"
+++ "mod\177"
@@ -1,0 +1,1 @@
+content
"#;
    let p = parse(s).unwrap();
    assert_eq!(p.original(), Some("orig\x7f"));
    assert_eq!(p.modified(), Some("mod\x7f"));

    // \377 = 0xFF
    let s = r#"\
--- "orig\377"
+++ "mod\377"
@@ -1,0 +1,1 @@
+content
"#;
    let b = parse_bytes(s.as_ref()).unwrap();
    assert_eq!(b.original(), Some(&b"orig\xff"[..]));
    assert_eq!(b.modified(), Some(&b"mod\xff"[..]));

    // First octal digit > 3 → error (would overflow a byte)
    let s = r#"\
--- "orig\477"
+++ "mod\477"
@@ -1,0 +1,1 @@
+content
"#;
    assert_eq!(
        parse(s).unwrap_err(),
        ParsePatchErrorKind::InvalidEscapedChar.into(),
    );

    // \101 = 'A' (0x41), first octal digit 1
    let s = r#"\
--- "orig\101"
+++ "mod\101"
@@ -1,0 +1,1 @@
+content
"#;
    let p = parse(s).unwrap();
    assert_eq!(p.original(), Some("origA"));
    assert_eq!(p.modified(), Some("modA"));

    // \277 = 0xBF, first octal digit 2
    let s = r#"\
--- "orig\277"
+++ "mod\277"
@@ -1,0 +1,1 @@
+content
"#;
    let b = parse_bytes(s.as_ref()).unwrap();
    assert_eq!(b.original(), Some(&b"orig\xbf"[..]));
    assert_eq!(b.modified(), Some(&b"mod\xbf"[..]));

    // Truncated octal (only 2 digits) → error
    let s = r#"\
--- "orig\03"
+++ "mod\03"
@@ -1,0 +1,1 @@
+content
"#;
    assert_eq!(
        parse(s).unwrap_err().kind,
        ParsePatchErrorKind::InvalidEscapedChar,
    );

    // Non-octal digit in second position → error
    let s = r#"\
--- "orig\08x"
+++ "mod\08x"
@@ -1,0 +1,1 @@
+content
"#;
    assert_eq!(
        parse(s).unwrap_err().kind,
        ParsePatchErrorKind::InvalidEscapedChar,
    );
}

#[test]
fn test_missing_filename_header() {
    // Missing Both '---' and '+++' lines
    let patch = r#"
@@ -1,11 +1,12 @@
 diesel::table! {
     users1 (id) {
-        id -> Nullable<Integer>,
+        id -> Integer,
     }
 }

 diesel::table! {
-    users2 (id) {
-        id -> Nullable<Integer>,
+    users2 (myid) {
+        #[sql_name = "id"]
+        myid -> Integer,
     }
 }
"#;

    parse(patch).unwrap();

    // Missing '---'
    let s = "\
+++ modified
@@ -1,0 +1,1 @@
+Oathbringer
";
    parse(s).unwrap();

    // Missing '+++'
    let s = "\
--- original
@@ -1,0 +1,1 @@
+Oathbringer
";
    parse(s).unwrap();

    // Headers out of order
    let s = "\
+++ modified
--- original
@@ -1,0 +1,1 @@
+Oathbringer
";
    parse(s).unwrap();

    // multiple headers should fail to parse
    let s = "\
--- original
--- modified
@@ -1,0 +1,1 @@
+Oathbringer
";
    assert_eq!(
        parse(s).unwrap_err().kind,
        ParsePatchErrorKind::MultipleOriginalHeaders,
    );
}

#[test]
fn adjacent_hunks_correctly_parse() {
    let s = "\
--- original
+++ modified
@@ -110,7 +110,7 @@
 --

 I am afraid, however, that all I have known - that my story - will be forgotten.
 I am afraid for the world that is to come.
-Afraid that my plans will fail. Afraid of a doom worse than the Deepness.
+Afraid that Alendi will fail. Afraid of a doom brought by the Deepness.

 Alendi was never the Hero of Ages.
@@ -117,7 +117,7 @@
 At best, I have amplified his virtues, creating a Hero where there was none.

-At worst, I fear that all we believe may have been corrupted.
+At worst, I fear that I have corrupted all we believe.

 --
 Alendi must not reach the Well of Ascension. He must not take the power for himself.

";
    parse(s).unwrap();
}

// Verify that formatting a parsed patch with escaped filenames
// produces output that re-parses to the same patch. This covers
// both the `Display` (str) and `to_bytes` ([u8]) paths.
#[test]
fn escaped_filename_roundtrip_named() {
    // Named escapes: \a \b \t \n \v \f \r \\ \"
    let s = r#"\
--- "a\a\b\t\n\v\f\r\\\""
+++ "b\a\b\t\n\v\f\r\\\""
@@ -1,1 +1,1 @@
-old
+new
"#;
    let p = parse(s).unwrap();

    // str roundtrip via Display
    let formatted = p.to_string();
    let p2 = parse(&formatted).unwrap();
    assert_eq!(p.original(), p2.original());
    assert_eq!(p.modified(), p2.modified());

    // bytes roundtrip via to_bytes
    let b = parse_bytes(s.as_ref()).unwrap();
    let bytes = b.to_bytes();
    let b2 = parse_bytes(&bytes).unwrap();
    assert_eq!(b.original(), b2.original());
    assert_eq!(b.modified(), b2.modified());
}

#[test]
fn escaped_filename_roundtrip_octal() {
    // Octal escapes for control chars without named escapes
    // and for high bytes (> 0x7f).
    let s = r#"\
--- "a\001\002\037\177"
+++ "b\001\002\037\177"
@@ -1,1 +1,1 @@
-old
+new
"#;
    let p = parse(s).unwrap();
    let formatted = p.to_string();
    let p2 = parse(&formatted).unwrap();
    assert_eq!(p.original(), p2.original());
    assert_eq!(p.modified(), p2.modified());

    // Bytes roundtrip with a high byte (\377 = 0xFF).
    let s = r#"\
--- "a\377"
+++ "b\377"
@@ -1,1 +1,1 @@
-old
+new
"#;
    let b = parse_bytes(s.as_ref()).unwrap();
    let bytes = b.to_bytes();
    let b2 = parse_bytes(&bytes).unwrap();
    assert_eq!(b.original(), b2.original());
    assert_eq!(b.modified(), b2.modified());
}

// Filenames without special characters should not be quoted.
#[test]
fn plain_filename_roundtrip() {
    let s = "\
--- a/normal.txt
+++ b/normal.txt
@@ -1,1 +1,1 @@
-old
+new
";
    let p = parse(s).unwrap();
    let formatted = p.to_string();
    assert!(!formatted.contains('"'));
    let p2 = parse(&formatted).unwrap();
    assert_eq!(p.original(), p2.original());
    assert_eq!(p.modified(), p2.modified());
}

// Octal escape \377 decodes to 0xFF, which is not valid UTF-8.
// When parsing into `Patch<'_, str>`, this returns a parse error
// instead of panicking.
#[test]
fn non_utf8_escaped_filename_returns_error_on_str_parse() {
    let s = r#"\
--- "a/foo\377"
+++ "b/foo\377"
@@ -1 +1 @@
-x
+y
"#;
    assert_eq!(
        parse(s).unwrap_err().kind,
        ParsePatchErrorKind::InvalidUtf8Path,
    );
}

mod error_display {
    use alloc::string::ToString;

    use crate::patch::error::ParsePatchErrorKind;
    use crate::Patch;
    use snapbox::assert_data_eq;
    use snapbox::str;

    #[test]
    fn invalid_hunk_header() {
        let content = "\
--- a/file.rs
+++ b/file.rs
@@ invalid @@
-old
+new
";
        let err = Patch::from_str(content).unwrap_err();
        assert_data_eq!(
            err.to_string(),
            str!["error parsing patch at byte 28: unable to parse hunk header"]
        );
    }

    #[test]
    fn hunk_mismatch() {
        let content = "\
--- a/file.rs
+++ b/file.rs
@@ -1,2 +1,2 @@
-only one line
+only one line
";
        let err = Patch::from_str(content).unwrap_err();
        assert_data_eq!(
            err.to_string(),
            str!["error parsing patch at byte 28: hunk header does not match hunk"]
        );
    }

    #[test]
    fn kind_preserved() {
        let content = "\
--- a/file.rs
+++ b/file.rs
@@ invalid @@
-old
+new
";
        let err = Patch::from_str(content).unwrap_err();
        assert_eq!(err.kind, ParsePatchErrorKind::InvalidHunkHeader);
    }
}
