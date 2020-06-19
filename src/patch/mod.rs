mod format;
mod parse;

pub use format::PatchFormatter;

use std::{borrow::Cow, fmt, ops};

const NO_NEWLINE_AT_EOF: &str = "\\ No newline at end of file";

/// Representation of all the differences between two files
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch<'a> {
    original: Filename<'a>,
    modified: Filename<'a>,
    hunks: Vec<Hunk<'a>>,
}

impl<'a> Patch<'a> {
    pub(crate) fn new<O, M>(original: O, modified: M, hunks: Vec<Hunk<'a>>) -> Self
    where
        O: Into<Cow<'a, str>>,
        M: Into<Cow<'a, str>>,
    {
        Self {
            original: Filename(original.into()),
            modified: Filename(modified.into()),
            hunks,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_str(s: &'a str) -> Result<Patch<'a>, parse::PatchParseError> {
        parse::parse(s)
    }

    /// Return the name of the old file
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Return the name of the new file
    pub fn modified(&self) -> &str {
        &self.modified
    }

    /// Returns the hunks in the patch
    pub fn hunks(&self) -> &[Hunk<'_>] {
        &self.hunks
    }
}

impl fmt::Display for Patch<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", PatchFormatter::new().fmt_patch(self))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Filename<'a>(Cow<'a, str>);

impl Filename<'_> {
    const ESCAPED_CHARS: &'static [char] = &['\n', '\t', '\0', '\r', '\"', '\\'];

    fn needs_to_be_escaped(&self) -> bool {
        self.0.contains(Self::ESCAPED_CHARS)
    }
}

impl AsRef<str> for Filename<'_> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl ops::Deref for Filename<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for Filename<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        if self.needs_to_be_escaped() {
            f.write_char('\"')?;
            for c in self.0.chars() {
                if Self::ESCAPED_CHARS.contains(&c) {
                    f.write_char('\\')?;
                }
                f.write_char(c)?;
            }
            f.write_char('\"')?;
        } else {
            f.write_str(&self.0)?;
        }

        Ok(())
    }
}

/// Represents a group of differing lines between two files
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk<'a> {
    old_range: HunkRange,
    new_range: HunkRange,

    function_context: Option<&'a str>,

    lines: Vec<Line<'a>>,
}

fn hunk_lines_count(lines: &[Line<'_>]) -> (usize, usize) {
    lines.iter().fold((0, 0), |count, line| match line {
        Line::Context(_) => (count.0 + 1, count.1 + 1),
        Line::Delete(_) => (count.0 + 1, count.1),
        Line::Insert(_) => (count.0, count.1 + 1),
    })
}

impl<'a> Hunk<'a> {
    pub(crate) fn new(
        old_range: HunkRange,
        new_range: HunkRange,
        function_context: Option<&'a str>,
        lines: Vec<Line<'a>>,
    ) -> Self {
        let (old_count, new_count) = hunk_lines_count(&lines);

        assert_eq!(old_range.len, old_count);
        assert_eq!(new_range.len, new_count);

        Self {
            old_range,
            new_range,
            function_context,
            lines,
        }
    }

    /// Returns the corresponding range for the old file in the hunk
    pub fn old_range(&self) -> HunkRange {
        self.old_range
    }

    /// Returns the corresponding range for the new file in the hunk
    pub fn new_range(&self) -> HunkRange {
        self.new_range
    }

    /// Returns the function context (if any) for the hunk
    pub fn function_context(&self) -> Option<&str> {
        self.function_context.as_deref()
    }

    /// Returns the lines in the hunk
    pub fn lines(&self) -> &[Line<'a>] {
        &self.lines
    }
}

/// The range of lines in a file for a particular `Hunk`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HunkRange {
    /// The starting line number of a hunk
    start: usize,
    /// The hunk size (number of lines)
    len: usize,
}

impl HunkRange {
    pub(crate) fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }

    /// Returns the range as a `ops::Range`
    pub fn range(&self) -> ops::Range<usize> {
        self.start..self.end()
    }

    /// Returns the starting line number of the range (inclusive)
    pub fn start(&self) -> usize {
        self.start
    }

    /// Returns the ending line number of the range (exclusive)
    pub fn end(&self) -> usize {
        self.start + self.len
    }

    /// Returns the number of lines in the range
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the range is empty (has a length of `0`)
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Display for HunkRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.start)?;
        if self.len != 1 {
            write!(f, ",{}", self.len)?;
        }
        Ok(())
    }
}

/// A line in either the old file, new file, or both.
///
/// A `Line` contains the terminating newline character `\n` unless it is the final
/// line in the file and the file does not end with a newline character.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Line<'a> {
    /// A line providing context in the diff which is present in both the old and new file
    Context(&'a str),
    /// A line deleted from the old file
    Delete(&'a str),
    /// A line inserted to the new file
    Insert(&'a str),
}
