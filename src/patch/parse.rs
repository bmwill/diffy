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

/// Options that control parsing behavior.
///
/// Defaults match the [`parse`]/[`parse_bytes`] behavior.
#[derive(Clone, Copy)]
pub(crate) struct ParseOpts {
    skip_preamble: bool,
    reject_orphaned_hunks: bool,
}

impl Default for ParseOpts {
    fn default() -> Self {
        Self {
            skip_preamble: true,
            reject_orphaned_hunks: false,
        }
    }
}

impl ParseOpts {
    /// Don't skip preamble lines before `---`/`+++`/`@@`.
    ///
    /// Useful when the caller has already positioned the input
    /// at the start of the patch content.
    #[allow(dead_code)] // will be used by patch_set parser
    pub(crate) fn no_skip_preamble(mut self) -> Self {
        self.skip_preamble = false;
        self
    }

    /// Reject orphaned `@@ ` hunk headers after parsed hunks,
    /// matching `git apply` behavior.
    pub(crate) fn reject_orphaned_hunks(mut self) -> Self {
        self.reject_orphaned_hunks = true;
        self
    }
}

struct Parser<'a, T: Text + ?Sized> {
    lines: std::iter::Peekable<LineIter<'a, T>>,
    offset: usize,
}

impl<'a, T: Text + ?Sized> Parser<'a, T> {
    fn new(input: &'a T) -> Self {
        Self {
            lines: LineIter::new(input).peekable(),
            offset: 0,
        }
    }

    fn peek(&mut self) -> Option<&&'a T> {
        self.lines.peek()
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn next(&mut self) -> Result<&'a T> {
        let line = self
            .lines
            .next()
            .ok_or_else(|| self.error(ParsePatchErrorKind::UnexpectedEof))?;
        self.offset += line.len();
        Ok(line)
    }

    /// Creates an error with the current offset as span.
    fn error(&self, kind: ParsePatchErrorKind) -> ParsePatchError {
        ParsePatchError::new(kind, self.offset..self.offset)
    }

    /// Creates an error with a specific offset as span.
    fn error_at(&self, kind: ParsePatchErrorKind, offset: usize) -> ParsePatchError {
        ParsePatchError::new(kind, offset..offset)
    }
}

pub fn parse(input: &str) -> Result<Patch<'_, str>> {
    let (result, _consumed) = parse_one(input, ParseOpts::default());
    result
}

pub fn parse_strict(input: &str) -> Result<Patch<'_, str>> {
    let (result, _consumed) = parse_one(input, ParseOpts::default().reject_orphaned_hunks());
    result
}

pub fn parse_bytes(input: &[u8]) -> Result<Patch<'_, [u8]>> {
    let mut parser = Parser::new(input);
    let header = patch_header(&mut parser, &ParseOpts::default())?;
    let hunks = hunks(&mut parser)?;

    Ok(Patch::new(header.0, header.1, hunks))
}

pub fn parse_bytes_strict(input: &[u8]) -> Result<Patch<'_, [u8]>> {
    let mut parser = Parser::new(input);
    let header = patch_header(&mut parser, &ParseOpts::default())?;
    let hunks = hunks(&mut parser)?;
    reject_orphaned_hunk_headers(&mut parser)?;

    Ok(Patch::new(header.0, header.1, hunks))
}

/// Parses one patch from input.
///
/// Always returns consumed bytes alongside the result
/// so callers can advance past the parsed or partially parsed content.
pub(crate) fn parse_one(input: &str, opts: ParseOpts) -> (Result<Patch<'_, str>>, usize) {
    let mut parser = Parser::new(input);

    let header = match patch_header(&mut parser, &opts) {
        Ok(h) => h,
        Err(e) => return (Err(e), parser.offset()),
    };
    let hunks = match hunks(&mut parser) {
        Ok(h) => h,
        Err(e) => return (Err(e), parser.offset()),
    };
    if opts.reject_orphaned_hunks {
        if let Err(e) = reject_orphaned_hunk_headers(&mut parser) {
            return (Err(e), parser.offset());
        }
    }

    let original = match header.0.map(convert_cow_to_str).transpose() {
        Ok(o) => o,
        Err(e) => return (Err(e), parser.offset()),
    };
    let modified = match header.1.map(convert_cow_to_str).transpose() {
        Ok(m) => m,
        Err(e) => return (Err(e), parser.offset()),
    };

    (Ok(Patch::new(original, modified, hunks)), parser.offset())
}

// This is only used when the type originated as a utf8 string
fn convert_cow_to_str(cow: Cow<'_, [u8]>) -> Result<Cow<'_, str>> {
    match cow {
        Cow::Borrowed(b) => std::str::from_utf8(b)
            .map(Cow::Borrowed)
            .map_err(|_| ParsePatchErrorKind::InvalidUtf8Path.into()),
        Cow::Owned(o) => String::from_utf8(o)
            .map(Cow::Owned)
            .map_err(|_| ParsePatchErrorKind::InvalidUtf8Path.into()),
    }
}

#[allow(clippy::type_complexity)]
fn patch_header<'a, T: Text + ToOwned + ?Sized>(
    parser: &mut Parser<'a, T>,
    opts: &ParseOpts,
) -> Result<(Option<Cow<'a, [u8]>>, Option<Cow<'a, [u8]>>)> {
    if opts.skip_preamble {
        skip_header_preamble(parser)?;
    }

    let mut filename1 = None;
    let mut filename2 = None;

    while let Some(line) = parser.peek() {
        if line.starts_with("--- ") {
            if filename1.is_some() {
                return Err(parser.error(ParsePatchErrorKind::MultipleOriginalHeaders));
            }
            filename1 = Some(parse_filename("--- ", parser.next()?)?);
        } else if line.starts_with("+++ ") {
            if filename2.is_some() {
                return Err(parser.error(ParsePatchErrorKind::MultipleModifiedHeaders));
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

/// Scans remaining lines for orphaned `@@ ` hunk headers.
///
/// In strict mode (git-apply behavior), trailing junk is allowed but
/// an `@@ ` line hiding behind that junk indicates a lost hunk.
fn reject_orphaned_hunk_headers<T: Text + ?Sized>(parser: &mut Parser<'_, T>) -> Result<()> {
    while let Some(line) = parser.peek() {
        if line.starts_with("@@ ") {
            return Err(parser.error(ParsePatchErrorKind::OrphanedHunkHeader));
        }
        parser.next()?;
    }
    Ok(())
}

fn hunks<'a, T: Text + ?Sized>(parser: &mut Parser<'a, T>) -> Result<Vec<Hunk<'a, T>>> {
    let mut hunks = Vec::new();

    // Parse hunks while we see @@ headers.
    //
    // Following GNU patch behavior: stop at non-@@ content.
    // Any trailing content (including hidden @@ headers) is silently ignored.
    // This is more permissive than git apply, which errors on junk between hunks.
    while parser.peek().is_some_and(|line| line.starts_with("@@ ")) {
        hunks.push(hunk(parser)?);
    }

    // check and verify that the Hunks are in sorted order and don't overlap
    if !verify_hunks_in_order(&hunks) {
        return Err(parser.error(ParsePatchErrorKind::HunksOutOfOrder));
    }

    Ok(hunks)
}

fn hunk<'a, T: Text + ?Sized>(parser: &mut Parser<'a, T>) -> Result<Hunk<'a, T>> {
    let hunk_start = parser.offset();
    let header_line = parser.next()?;
    let (range1, range2, function_context) =
        hunk_header(header_line).map_err(|e| parser.error_at(e.kind, hunk_start))?;
    let lines = hunk_lines(parser, range1.len, range2.len, hunk_start)?;

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

fn hunk_lines<'a, T: Text + ?Sized>(
    parser: &mut Parser<'a, T>,
    expected_old: usize,
    expected_new: usize,
    hunk_start: usize,
) -> Result<Vec<Line<'a, T>>> {
    let mut lines: Vec<Line<'a, T>> = Vec::new();
    let mut no_newline_context = false;
    let mut no_newline_delete = false;
    let mut no_newline_insert = false;

    // Track current line counts (old = context + delete, new = context + insert)
    let mut old_count = 0;
    let mut new_count = 0;

    while let Some(line) = parser.peek() {
        // Check if hunk is complete
        let hunk_complete = old_count >= expected_old && new_count >= expected_new;

        let line = if line.starts_with("@") {
            break;
        } else if no_newline_context {
            // After `\ No newline at end of file` on a context line,
            // only a new hunk header is valid. Any other line means
            // the hunk should be complete, or it's an error.
            if hunk_complete {
                break;
            }
            return Err(parser.error(ParsePatchErrorKind::ExpectedEndOfHunk));
        } else if let Some(line) = line.strip_prefix(" ") {
            if hunk_complete {
                break;
            }
            Line::Context(line)
        } else if line.starts_with("\n") {
            if hunk_complete {
                break;
            }
            Line::Context(*line)
        } else if let Some(line) = line.strip_prefix("-") {
            if no_newline_delete {
                return Err(parser.error(ParsePatchErrorKind::TooManyDeletedLines));
            }
            if hunk_complete {
                break;
            }
            Line::Delete(line)
        } else if let Some(line) = line.strip_prefix("+") {
            if no_newline_insert {
                return Err(parser.error(ParsePatchErrorKind::TooManyInsertedLines));
            }
            if hunk_complete {
                break;
            }
            Line::Insert(line)
        } else if line.starts_with(NO_NEWLINE_AT_EOF) {
            // The `\ No newline at end of file` marker indicates
            // the previous line doesn't end with a newline.
            // It's not a content line itself.
            // Therefore, we
            //
            // * strip the newline character of the previous line
            // * don't increment line counts and continue to next directly
            let last_line = lines
                .pop()
                .ok_or_else(|| parser.error(ParsePatchErrorKind::UnexpectedNoNewlineMarker))?;
            let modified = match last_line {
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
            };
            lines.push(modified);
            parser.next()?;
            continue;
        } else {
            // Non-hunk line encountered
            if hunk_complete {
                // Hunk is complete, treat remaining content as garbage
                break;
            }
            return Err(parser.error(ParsePatchErrorKind::UnexpectedHunkLine));
        };

        match &line {
            Line::Context(_) => {
                old_count += 1;
                new_count += 1;
            }
            Line::Delete(_) => {
                old_count += 1;
            }
            Line::Insert(_) => {
                new_count += 1;
            }
        }

        lines.push(line);
        parser.next()?;
    }

    // Final check: ensure we got the expected number of lines
    if old_count != expected_old || new_count != expected_new {
        return Err(parser.error_at(ParsePatchErrorKind::HunkMismatch, hunk_start));
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
