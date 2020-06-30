//! Parse a Patch

use super::{Filename, Hunk, HunkRange, Line, NO_NEWLINE_AT_EOF};
use crate::{patch::Patch, utils::LineIter};
use std::borrow::Cow;

type Result<T, E = PatchParseError> = std::result::Result<T, E>;

// TODO use a custom error type instead of a Cow
#[derive(Debug)]
pub struct PatchParseError(Cow<'static, str>);

impl<T> From<T> for PatchParseError
where
    T: Into<Cow<'static, str>>,
{
    fn from(e: T) -> Self {
        Self(e.into())
    }
}

struct Parser<'a> {
    lines: std::iter::Peekable<LineIter<'a>>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            lines: LineIter::new(input).peekable(),
        }
    }

    fn peek(&mut self) -> Option<&&'a str> {
        self.lines.peek()
    }

    fn next(&mut self) -> Result<&'a str> {
        let line = self.lines.next().ok_or("unexpected EOF")?;
        Ok(line)
    }
}

#[allow(dead_code)]
pub fn parse<'a>(input: &'a str) -> Result<Patch<'a>> {
    let mut parser = Parser::new(input);
    let header = patch_header(&mut parser)?;
    let hunks = hunks(&mut parser)?;

    Ok(Patch::new(header.0, header.1, hunks))
}

fn patch_header<'a>(parser: &mut Parser<'a>) -> Result<(Cow<'a, str>, Cow<'a, str>)> {
    skip_header_preamble(parser)?;
    let filename1 = parse_filename("--- ", parser.next()?)?;
    let filename2 = parse_filename("+++ ", parser.next()?)?;
    Ok((filename1, filename2))
}

// Skip to the first "--- " line, skipping any preamble lines like "diff --git", etc.
fn skip_header_preamble<'a>(parser: &mut Parser<'a>) -> Result<()> {
    while let Some(line) = parser.peek() {
        if line.starts_with("--- ") {
            break;
        }
        parser.next()?;
    }

    Ok(())
}

fn parse_filename<'a>(prefix: &str, line: &'a str) -> Result<Cow<'a, str>> {
    let line = strip_prefix(line, prefix)?;

    let filename_end = line
        .find(['\n', '\t'].as_ref())
        .ok_or("filename unterminated")?;
    let filename = &line[..filename_end];

    let filename = if is_quoted(filename) {
        escaped_filename(&filename[1..filename.len() - 1])?
    } else {
        unescaped_filename(filename)?
    };

    Ok(filename)
}

fn is_quoted(s: &str) -> bool {
    s.starts_with('\"') && s.ends_with('\"')
}

fn unescaped_filename<'a>(filename: &'a str) -> Result<Cow<'a, str>> {
    if filename.contains(Filename::ESCAPED_CHARS) {
        return Err("invalid char in unquoted filename".into());
    }

    Ok(filename.into())
}

fn escaped_filename(escaped: &str) -> Result<Cow<'_, str>> {
    let mut filename = String::new();

    let mut chars = escaped.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next().ok_or("expected escaped character")? {
                'n' | 't' | '0' | 'r' | '\"' | '\\' => filename.push(c),
                _ => return Err("invalid escaped character".into()),
            }
        } else if Filename::ESCAPED_CHARS.contains(&c) {
            return Err("invalid unescaped character".into());
        } else {
            filename.push(c);
        }
    }

    Ok(filename.into())
}

fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Result<&'a str> {
    if s.starts_with(prefix) {
        Ok(&s[prefix.len()..])
    } else {
        Err(format!("prefix doesn't match: prefix: {:?} input: {:?}", prefix, s).into())
    }
}

fn verify_hunks_in_order(hunks: &[Hunk<'_>]) -> bool {
    for hunk in hunks.windows(2) {
        if hunk[0].old_range.end() >= hunk[1].old_range.start()
            || hunk[0].new_range.end() >= hunk[1].new_range.start()
        {
            return false;
        }
    }
    true
}

fn hunks<'a>(parser: &mut Parser<'a>) -> Result<Vec<Hunk<'a>>> {
    let mut hunks = Vec::new();
    while parser.peek().is_some() {
        hunks.push(hunk(parser)?);
    }

    // check and verify that the Hunks are in sorted order and don't overlap
    if !verify_hunks_in_order(&hunks) {
        return Err("Hunks not in order/overlap".into());
    }

    Ok(hunks)
}

fn hunk<'a>(parser: &mut Parser<'a>) -> Result<Hunk<'a>> {
    let (range1, range2, function_context) = hunk_header(parser.next()?)?;
    let lines = hunk_lines(parser)?;

    // check counts of lines to see if they match the ranges in the hunk header
    let (len1, len2) = super::hunk_lines_count(&lines);
    if len1 != range1.len || len2 != range2.len {
        return Err("Hunk header does not match hunk".into());
    }

    Ok(Hunk::new(range1, range2, function_context, lines))
}

fn hunk_header<'a>(input: &'a str) -> Result<(HunkRange, HunkRange, Option<&'a str>)> {
    let input = strip_prefix(input, "@@ ")?;

    let (ranges, function_context) =
        split_at_exclusive(input, " @@").map_err(|_| "hunk header unterminated")?;
    let function_context = strip_prefix(function_context, " ").ok();

    let (range1, range2) = split_at_exclusive(ranges, " ")?;
    let range1 = range(strip_prefix(range1, "-")?)?;
    let range2 = range(strip_prefix(range2, "+")?)?;
    Ok((range1, range2, function_context))
}

fn split_at_exclusive<'a>(s: &'a str, needle: &str) -> Result<(&'a str, &'a str)> {
    if let Some(idx) = s.find(needle) {
        Ok((&s[..idx], &s[idx + needle.len()..]))
    } else {
        Err("unable to find needle".into())
    }
}

fn range(s: &str) -> Result<HunkRange> {
    let (start, len) = if let Ok((start, len)) = split_at_exclusive(s, ",") {
        (
            start.parse().map_err(|_| "can't parse range")?,
            len.parse().map_err(|_| "can't parse range")?,
        )
    } else {
        (s.parse().map_err(|_| "cant parse range")?, 1)
    };

    Ok(HunkRange::new(start, len))
}

fn hunk_lines<'a>(parser: &mut Parser<'a>) -> Result<Vec<Line<'a>>> {
    let mut lines: Vec<Line<'a>> = Vec::new();
    let mut no_newline_context = false;
    let mut no_newline_delete = false;
    let mut no_newline_insert = false;

    while let Some(line) = parser.peek() {
        let line = if line.starts_with('@') {
            break;
        } else if no_newline_context {
            return Err("expected end of hunk".into());
        } else if line.starts_with(' ') {
            Line::Context(&line[1..])
        } else if *line == "\n" {
            Line::Context(line)
        } else if line.starts_with('-') {
            if no_newline_delete {
                return Err("expected no more deleted lines".into());
            }
            Line::Delete(&line[1..])
        } else if line.starts_with('+') {
            if no_newline_insert {
                return Err("expected no more inserted lines".into());
            }
            Line::Insert(&line[1..])
        } else if line.starts_with(NO_NEWLINE_AT_EOF) {
            let last_line = lines
                .pop()
                .ok_or("unexpected 'No newline at end of file' line")?;
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
            return Err("unexpected line".into());
        };

        lines.push(line);
        parser.next()?;
    }

    Ok(lines)
}

fn strip_newline<'a>(s: &'a str) -> Result<&'a str> {
    if s.ends_with('\n') {
        Ok(&s[..s.len() - 1])
    } else {
        Err("missing newline".into())
    }
}
