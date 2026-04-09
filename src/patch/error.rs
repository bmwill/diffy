//! Error types for patch parsing.

use std::fmt;

/// An error returned when parsing a `Patch` using [`Patch::from_str`] fails.
///
/// [`Patch::from_str`]: struct.Patch.html#method.from_str
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePatchError {
    pub(crate) kind: ParsePatchErrorKind,
}

impl fmt::Display for ParsePatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error parsing patch: {}", self.kind)
    }
}

impl std::error::Error for ParsePatchError {}

impl From<ParsePatchErrorKind> for ParsePatchError {
    fn from(kind: ParsePatchErrorKind) -> Self {
        Self { kind }
    }
}

/// The kind of error that occurred when parsing a patch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum ParsePatchErrorKind {
    /// Unexpected end of input.
    UnexpectedEof,

    /// Multiple `---` lines found where only one expected.
    MultipleOriginalHeaders,

    /// Multiple `+++` lines found where only one expected.
    MultipleModifiedHeaders,

    /// Unable to parse filename from header line.
    InvalidFilename,

    /// Filename line missing newline or tab terminator.
    FilenameUnterminated,

    /// Invalid character in unquoted filename.
    InvalidCharInUnquotedFilename,

    /// Expected escaped character in quoted filename.
    ExpectedEscapedChar,

    /// Invalid escaped character in quoted filename.
    InvalidEscapedChar,

    /// Invalid unescaped character in quoted filename.
    InvalidUnescapedChar,

    /// Unable to parse hunk header (`@@ ... @@`).
    InvalidHunkHeader,

    /// Hunk header missing closing `@@`.
    HunkHeaderUnterminated,

    /// Unable to parse range in hunk header.
    InvalidRange,

    /// Hunks are not in order or overlap.
    HunksOutOfOrder,

    /// Hunk header line counts don't match actual content.
    HunkMismatch,

    /// Expected end of hunk after `\ No newline at end of file`.
    ExpectedEndOfHunk,

    /// Expected no more deleted lines in hunk.
    TooManyDeletedLines,

    /// Expected no more inserted lines in hunk.
    TooManyInsertedLines,

    /// Unexpected `\ No newline at end of file` marker.
    UnexpectedNoNewlineMarker,

    /// Unexpected line in hunk body.
    UnexpectedHunkLine,

    /// Missing newline at end of line.
    MissingNewline,
}

impl fmt::Display for ParsePatchErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::UnexpectedEof => "unexpected EOF",
            Self::MultipleOriginalHeaders => "multiple '---' lines",
            Self::MultipleModifiedHeaders => "multiple '+++' lines",
            Self::InvalidFilename => "unable to parse filename",
            Self::FilenameUnterminated => "filename unterminated",
            Self::InvalidCharInUnquotedFilename => "invalid char in unquoted filename",
            Self::ExpectedEscapedChar => "expected escaped character",
            Self::InvalidEscapedChar => "invalid escaped character",
            Self::InvalidUnescapedChar => "invalid unescaped character",
            Self::InvalidHunkHeader => "unable to parse hunk header",
            Self::HunkHeaderUnterminated => "hunk header unterminated",
            Self::InvalidRange => "can't parse range",
            Self::HunksOutOfOrder => "hunks not in order or overlap",
            Self::HunkMismatch => "hunk header does not match hunk",
            Self::ExpectedEndOfHunk => "expected end of hunk",
            Self::TooManyDeletedLines => "expected no more deleted lines",
            Self::TooManyInsertedLines => "expected no more inserted lines",
            Self::UnexpectedNoNewlineMarker => "unexpected 'No newline at end of file' line",
            Self::UnexpectedHunkLine => "unexpected line in hunk body",
            Self::MissingNewline => "missing newline",
        };
        write!(f, "{msg}")
    }
}
