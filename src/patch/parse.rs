//! Parse a Patch

use super::{
    error::{ParsePatchError, ParsePatchErrorKind},
    Hunk, HunkRange, Line, NO_NEWLINE_AT_EOF,
};
use crate::{
    patch::Patch,
    utils::{escaped_filename, LineIter, Text},
};
use std::borrow::Cow;

type Result<T, E = ParsePatchError> = std::result::Result<T, E>;

struct Parser<'a, T: Text + ?Sized> {
    lines: std::iter::Peekable<LineIter<'a, T>>,
}

impl<'a, T: Text + ?Sized> Parser<'a, T> {
    fn new(input: &'a T) -> Self {
        Self {
            lines: LineIter::new(input).peekable(),
        }
    }

    fn peek(&mut self) -> Option<&&'a T> {
        self.lines.peek()
    }

    fn next(&mut self) -> Result<&'a T> {
        let line = self
            .lines
            .next()
            .ok_or(ParsePatchErrorKind::UnexpectedEof)?;
        Ok(line)
    }
}

pub fn parse(input: &str) -> Result<Patch<'_, str>> {
    let mut parser = Parser::new(input);
    let header = patch_header(&mut parser)?;
    let hunks = hunks(&mut parser)?;

    Ok(Patch::new(
        header.0.map(convert_cow_to_str),
        header.1.map(convert_cow_to_str),
        hunks,
    ))
}

pub fn parse_bytes(input: &[u8]) -> Result<Patch<'_, [u8]>> {
    let mut parser = Parser::new(input);
    let header = patch_header(&mut parser)?;
    let hunks = hunks(&mut parser)?;

    Ok(Patch::new(header.0, header.1, hunks))
}

// This is only used when the type originated as a utf8 string
fn convert_cow_to_str(cow: Cow<'_, [u8]>) -> Cow<'_, str> {
    match cow {
        Cow::Borrowed(b) => std::str::from_utf8(b).unwrap().into(),
        Cow::Owned(o) => String::from_utf8(o).unwrap().into(),
    }
}

#[allow(clippy::type_complexity)]
fn patch_header<'a, T: Text + ToOwned + ?Sized>(
    parser: &mut Parser<'a, T>,
) -> Result<(Option<Cow<'a, [u8]>>, Option<Cow<'a, [u8]>>)> {
    skip_header_preamble(parser)?;

    let mut filename1 = None;
    let mut filename2 = None;

    while let Some(line) = parser.peek() {
        if line.starts_with("--- ") {
            if filename1.is_some() {
                return Err(ParsePatchErrorKind::MultipleOriginalHeaders.into());
            }
            filename1 = Some(parse_filename("--- ", parser.next()?)?);
        } else if line.starts_with("+++ ") {
            if filename2.is_some() {
                return Err(ParsePatchErrorKind::MultipleModifiedHeaders.into());
            }
            filename2 = Some(parse_filename("+++ ", parser.next()?)?);
        } else {
            break;
        }
    }

    Ok((filename1, filename2))
}

// Skip to the first filename header ("--- " or "+++ ") or hunk line,
// skipping any preamble lines like "diff --git", etc.
fn skip_header_preamble<T: Text + ?Sized>(parser: &mut Parser<'_, T>) -> Result<()> {
    while let Some(line) = parser.peek() {
        if line.starts_with("--- ") | line.starts_with("+++ ") | line.starts_with("@@ ") {
            break;
        }
        parser.next()?;
    }

    Ok(())
}

fn parse_filename<'a, T: Text + ToOwned + ?Sized>(
    prefix: &str,
    line: &'a T,
) -> Result<Cow<'a, [u8]>> {
    let line = line
        .strip_prefix(prefix)
        .ok_or(ParsePatchErrorKind::InvalidFilename)?;

    let filename = if let Some((filename, _)) = line.split_at_exclusive("\t") {
        filename
    } else if let Some((filename, _)) = line.split_at_exclusive("\n") {
        filename
    } else {
        return Err(ParsePatchErrorKind::FilenameUnterminated.into());
    };

    let filename = escaped_filename(filename)?;

    Ok(filename)
}

fn verify_hunks_in_order<T: ?Sized>(hunks: &[Hunk<'_, T>]) -> bool {
    for hunk in hunks.windows(2) {
        if hunk[0].old_range.end() > hunk[1].old_range.start()
            || hunk[0].new_range.end() > hunk[1].new_range.start()
        {
            return false;
        }
    }
    true
}

fn hunks<'a, T: Text + ?Sized>(parser: &mut Parser<'a, T>) -> Result<Vec<Hunk<'a, T>>> {
    let mut hunks = Vec::new();
    while parser.peek().is_some() {
        hunks.push(hunk(parser)?);
    }

    // check and verify that the Hunks are in sorted order and don't overlap
    if !verify_hunks_in_order(&hunks) {
        return Err(ParsePatchErrorKind::HunksOutOfOrder.into());
    }

    Ok(hunks)
}

fn hunk<'a, T: Text + ?Sized>(parser: &mut Parser<'a, T>) -> Result<Hunk<'a, T>> {
    let (range1, range2, function_context) = hunk_header(parser.next()?)?;
    let lines = hunk_lines(parser)?;

    // check counts of lines to see if they match the ranges in the hunk header
    let (len1, len2) = super::hunk_lines_count(&lines);
    if len1 != range1.len || len2 != range2.len {
        return Err(ParsePatchErrorKind::HunkMismatch.into());
    }

    Ok(Hunk::new(range1, range2, function_context, lines))
}

fn hunk_header<T: Text + ?Sized>(input: &T) -> Result<(HunkRange, HunkRange, Option<&T>)> {
    let input = input
        .strip_prefix("@@ ")
        .ok_or(ParsePatchErrorKind::InvalidHunkHeader)?;

    let (ranges, function_context) = input
        .split_at_exclusive(" @@")
        .ok_or(ParsePatchErrorKind::HunkHeaderUnterminated)?;
    let function_context = function_context.strip_prefix(" ");

    let (range1, range2) = ranges
        .split_at_exclusive(" ")
        .ok_or(ParsePatchErrorKind::InvalidHunkHeader)?;
    let range1 = range(
        range1
            .strip_prefix("-")
            .ok_or(ParsePatchErrorKind::InvalidHunkHeader)?,
    )?;
    let range2 = range(
        range2
            .strip_prefix("+")
            .ok_or(ParsePatchErrorKind::InvalidHunkHeader)?,
    )?;
    Ok((range1, range2, function_context))
}

fn range<T: Text + ?Sized>(s: &T) -> Result<HunkRange> {
    let (start, len) = if let Some((start, len)) = s.split_at_exclusive(",") {
        (
            start.parse().ok_or(ParsePatchErrorKind::InvalidRange)?,
            len.parse().ok_or(ParsePatchErrorKind::InvalidRange)?,
        )
    } else {
        (s.parse().ok_or(ParsePatchErrorKind::InvalidRange)?, 1)
    };

    Ok(HunkRange::new(start, len))
}

fn hunk_lines<'a, T: Text + ?Sized>(parser: &mut Parser<'a, T>) -> Result<Vec<Line<'a, T>>> {
    let mut lines: Vec<Line<'a, T>> = Vec::new();
    let mut no_newline_context = false;
    let mut no_newline_delete = false;
    let mut no_newline_insert = false;

    while let Some(line) = parser.peek() {
        let line = if line.starts_with("@") {
            break;
        } else if no_newline_context {
            return Err(ParsePatchErrorKind::ExpectedEndOfHunk.into());
        } else if let Some(line) = line.strip_prefix(" ") {
            Line::Context(line)
        } else if line.starts_with("\n") {
            Line::Context(*line)
        } else if let Some(line) = line.strip_prefix("-") {
            if no_newline_delete {
                return Err(ParsePatchErrorKind::TooManyDeletedLines.into());
            }
            Line::Delete(line)
        } else if let Some(line) = line.strip_prefix("+") {
            if no_newline_insert {
                return Err(ParsePatchErrorKind::TooManyInsertedLines.into());
            }
            Line::Insert(line)
        } else if line.starts_with(NO_NEWLINE_AT_EOF) {
            let last_line = lines
                .pop()
                .ok_or(ParsePatchErrorKind::UnexpectedNoNewlineMarker)?;
            match last_line {
                Line::Context(line) => {
                    no_newline_context = true;
                    Line::Context(strip_newline(line)?)
                }
                Line::Delete(line) => {
                    no_newline_delete = true;
                    Line::Delete(strip_newline(line)?)
                }
                Line::Insert(line) => {
                    no_newline_insert = true;
                    Line::Insert(strip_newline(line)?)
                }
            }
        } else {
            return Err(ParsePatchErrorKind::UnexpectedHunkLine.into());
        };

        lines.push(line);
        parser.next()?;
    }

    Ok(lines)
}

fn strip_newline<T: Text + ?Sized>(s: &T) -> Result<&T> {
    if let Some(stripped) = s.strip_suffix("\n") {
        Ok(stripped)
    } else {
        Err(ParsePatchErrorKind::MissingNewline.into())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, parse_bytes};

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
        parse(s).unwrap_err();
        parse_bytes(s.as_ref()).unwrap_err();

        // quoted with invalid escaped characters
        let s = "\
--- \"ori\\\"g\rinal\"
+++ modified
@@ -1,0 +1,1 @@
+Oathbringer
";
        parse(s).unwrap_err();
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

        // Truncated octal (only 2 digits) → error
        let s = r#"\
--- "orig\03"
+++ "mod\03"
@@ -1,0 +1,1 @@
+content
"#;
        parse(s).unwrap_err();

        // Non-octal digit in second position → error
        let s = r#"\
--- "orig\08x"
+++ "mod\08x"
@@ -1,0 +1,1 @@
+content
"#;
        parse(s).unwrap_err();

        // First octal digit > 3 → error (would overflow a byte)
        let s = r#"\
--- "orig\477"
+++ "mod\477"
@@ -1,0 +1,1 @@
+content
"#;
        parse(s).unwrap_err();

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
        parse(s).unwrap_err();
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
}
