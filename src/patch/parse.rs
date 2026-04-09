//! Parse a Patch

use super::{Hunk, HunkRange, Line, NO_NEWLINE_AT_EOF};
use crate::{
    patch::Patch,
    utils::{escaped_filename, LineIter, Text},
};
use std::{borrow::Cow, fmt};

type Result<T, E = ParsePatchError> = std::result::Result<T, E>;

/// An error returned when parsing a `Patch` using [`Patch::from_str`] fails
///
/// [`Patch::from_str`]: struct.Patch.html#method.from_str
// TODO use a custom error type instead of a Cow
#[derive(Debug)]
pub struct ParsePatchError(Cow<'static, str>);

impl ParsePatchError {
    pub(crate) fn new<E: Into<Cow<'static, str>>>(e: E) -> Self {
        Self(e.into())
    }
}

impl fmt::Display for ParsePatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error parsing patch: {}", self.0)
    }
}

impl std::error::Error for ParsePatchError {}

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
            .ok_or_else(|| ParsePatchError::new("unexpected EOF"))?;
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
                return Err(ParsePatchError::new("multiple '---' lines"));
            }
            filename1 = Some(parse_filename("--- ", parser.next()?)?);
        } else if line.starts_with("+++ ") {
            if filename2.is_some() {
                return Err(ParsePatchError::new("multiple '+++' lines"));
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
        .ok_or_else(|| ParsePatchError::new("unable to parse filename"))?;

    let filename = if let Some((filename, _)) = line.split_at_exclusive("\t") {
        filename
    } else if let Some((filename, _)) = line.split_at_exclusive("\n") {
        filename
    } else {
        return Err(ParsePatchError::new("filename unterminated"));
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
        return Err(ParsePatchError::new("Hunks not in order or overlap"));
    }

    Ok(hunks)
}

fn hunk<'a, T: Text + ?Sized>(parser: &mut Parser<'a, T>) -> Result<Hunk<'a, T>> {
    let (range1, range2, function_context) = hunk_header(parser.next()?)?;
    let lines = hunk_lines(parser)?;

    // check counts of lines to see if they match the ranges in the hunk header
    let (len1, len2) = super::hunk_lines_count(&lines);
    if len1 != range1.len || len2 != range2.len {
        return Err(ParsePatchError::new("Hunk header does not match hunk"));
    }

    Ok(Hunk::new(range1, range2, function_context, lines))
}

fn hunk_header<T: Text + ?Sized>(input: &T) -> Result<(HunkRange, HunkRange, Option<&T>)> {
    let input = input
        .strip_prefix("@@ ")
        .ok_or_else(|| ParsePatchError::new("unable to parse hunk header"))?;

    let (ranges, function_context) = input
        .split_at_exclusive(" @@")
        .ok_or_else(|| ParsePatchError::new("hunk header unterminated"))?;
    let function_context = function_context.strip_prefix(" ");

    let (range1, range2) = ranges
        .split_at_exclusive(" ")
        .ok_or_else(|| ParsePatchError::new("unable to parse hunk header"))?;
    let range1 = range(
        range1
            .strip_prefix("-")
            .ok_or_else(|| ParsePatchError::new("unable to parse hunk header"))?,
    )?;
    let range2 = range(
        range2
            .strip_prefix("+")
            .ok_or_else(|| ParsePatchError::new("unable to parse hunk header"))?,
    )?;
    Ok((range1, range2, function_context))
}

fn range<T: Text + ?Sized>(s: &T) -> Result<HunkRange> {
    let (start, len) = if let Some((start, len)) = s.split_at_exclusive(",") {
        (
            start
                .parse()
                .ok_or_else(|| ParsePatchError::new("can't parse range"))?,
            len.parse()
                .ok_or_else(|| ParsePatchError::new("can't parse range"))?,
        )
    } else {
        (
            s.parse()
                .ok_or_else(|| ParsePatchError::new("can't parse range"))?,
            1,
        )
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
            return Err(ParsePatchError::new("expected end of hunk"));
        } else if let Some(line) = line.strip_prefix(" ") {
            Line::Context(line)
        } else if line.starts_with("\n") {
            Line::Context(*line)
        } else if let Some(line) = line.strip_prefix("-") {
            if no_newline_delete {
                return Err(ParsePatchError::new("expected no more deleted lines"));
            }
            Line::Delete(line)
        } else if let Some(line) = line.strip_prefix("+") {
            if no_newline_insert {
                return Err(ParsePatchError::new("expected no more inserted lines"));
            }
            Line::Insert(line)
        } else if line.starts_with(NO_NEWLINE_AT_EOF) {
            let last_line = lines.pop().ok_or_else(|| {
                ParsePatchError::new("unexpected 'No newline at end of file' line")
            })?;
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
            return Err(ParsePatchError::new("unexpected line in hunk body"));
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
        Err(ParsePatchError::new("missing newline"))
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
+++ "mo\0\t\r\n\\dified"
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
    // diffy misparsed \0 as standalone NUL instead of 3-digit octal start,
    // so \033 becomes NUL + literal "33".
    //
    // Found via llvm/llvm-project full-history replay
    // (commits 17af06ba..229c95ab, 6c031780..0683a1e5).
    #[test]
    fn escaped_filename_octal_misparsed() {
        let s = r#"\
--- "orig\033"
+++ "mod\033"
@@ -1,0 +1,1 @@
+content
"#;
        // Currently parses "successfully" but produces wrong result:
        // \0 is consumed as NUL, "33" remains literal.
        let p = parse(s).unwrap();
        // BUG: should be "orig\x1b" (ESC) but is "orig\x0033" (NUL + "33")
        assert_eq!(p.original(), Some("orig\x0033"));
        assert_eq!(p.modified(), Some("mod\x0033"));
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
