//! Error types for patches parsing.

use std::fmt;
use std::ops::Range;

use crate::binary::BinaryPatchParseError;
use crate::patch::ParsePatchError;

/// An error returned when parsing patches fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchSetParseError {
    pub(crate) kind: PatchSetParseErrorKind,
    span: Option<Range<usize>>,
}

impl PatchSetParseError {
    /// Creates a new error with the given kind and span.
    pub(crate) fn new(kind: PatchSetParseErrorKind, span: Range<usize>) -> Self {
        Self {
            kind,
            span: Some(span),
        }
    }

    /// Sets the byte range span for this error.
    pub(crate) fn set_span(&mut self, span: Range<usize>) {
        self.span = Some(span);
    }
}

impl fmt::Display for PatchSetParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(span) = &self.span {
            write!(
                f,
                "error parsing patches at byte {}: {}",
                span.start, self.kind
            )
        } else {
            write!(f, "error parsing patches: {}", self.kind)
        }
    }
}

impl std::error::Error for PatchSetParseError {}

impl From<PatchSetParseErrorKind> for PatchSetParseError {
    fn from(kind: PatchSetParseErrorKind) -> Self {
        Self { kind, span: None }
    }
}

/// The kind of error that occurred when parsing patches.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum PatchSetParseErrorKind {
    /// Single patch parsing failed.
    Patch(ParsePatchError),

    /// No valid patches found in input.
    NoPatchesFound,

    /// Patch has no file path.
    NoFilePath,

    /// Patch has both original and modified as /dev/null.
    BothDevNull,

    /// Delete patch missing original path.
    DeleteMissingOriginalPath,

    /// Create patch missing modified path.
    CreateMissingModifiedPath,

    /// Invalid file mode string.
    InvalidFileMode(String),

    /// Invalid `diff --git` path.
    InvalidDiffGitPath,

    /// File path contains invalid UTF-8.
    InvalidUtf8Path,

    /// Binary diff not supported in current configuration.
    BinaryNotSupported { path: String },

    /// Binary patch parsing failed.
    BinaryParse(BinaryPatchParseError),
}

impl fmt::Display for PatchSetParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Patch(e) => write!(f, "{e}"),
            Self::NoPatchesFound => write!(f, "no valid patches found"),
            Self::NoFilePath => write!(f, "patch has no file path"),
            Self::BothDevNull => write!(f, "patch has both original and modified as /dev/null"),
            Self::DeleteMissingOriginalPath => write!(f, "delete patch has no original path"),
            Self::CreateMissingModifiedPath => write!(f, "create patch has no modified path"),
            Self::InvalidFileMode(mode) => write!(f, "invalid file mode: {mode}"),
            Self::InvalidDiffGitPath => write!(f, "invalid diff --git path"),
            Self::InvalidUtf8Path => write!(f, "file path is not valid UTF-8"),
            Self::BinaryNotSupported { path } => {
                write!(f, "binary diff not supported: {path}")
            }
            Self::BinaryParse(e) => write!(f, "{e}"),
        }
    }
}

impl From<ParsePatchError> for PatchSetParseError {
    fn from(e: ParsePatchError) -> Self {
        PatchSetParseErrorKind::Patch(e).into()
    }
}

impl From<BinaryPatchParseError> for PatchSetParseError {
    fn from(e: BinaryPatchParseError) -> Self {
        PatchSetParseErrorKind::BinaryParse(e).into()
    }
}
