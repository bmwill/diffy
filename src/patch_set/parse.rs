//! Parse multiple file patches from a unified diff.

use super::{
    error::PatchSetParseErrorKind, FileOperation, FilePatch, Format, ParseOptions,
    PatchSetParseError,
};
use crate::patch::parse::parse_one;

use std::borrow::Cow;

/// Prefix for the original file path (e.g., `--- a/file.rs`).
const ORIGINAL_PREFIX: &str = "--- ";
/// Prefix for the modified file path (e.g., `+++ b/file.rs`).
const MODIFIED_PREFIX: &str = "+++ ";
/// Path used to indicate file creation or deletion.
const DEV_NULL: &str = "/dev/null";

/// Separator between commit message and patch in git format-patch output.
const EMAIL_PREAMBLE_SEPARATOR: &str = "\n---\n";

/// Streaming iterator for parsing patches one at a time.
///
/// Created by [`PatchSet::parse`].
///
/// # Example
///
/// ```
/// use diffy::patch_set::{PatchSet, ParseOptions};
///
/// let s = "\
/// --- original
/// +++ modified
/// @@ -1 +1 @@
/// -old
/// +new
/// --- original2
/// +++ modified2
/// @@ -1 +1 @@
/// -foo
/// +bar
/// ";
///
/// for patch in PatchSet::parse(s, ParseOptions::unidiff()) {
///     let patch = patch.unwrap();
///     println!("{:?}", patch.operation());
/// }
/// ```
pub struct PatchSet<'a> {
    input: &'a str,
    offset: usize,
    opts: ParseOptions,
    finished: bool,
    found_any: bool,
}

impl<'a> PatchSet<'a> {
    /// Creates a streaming parser for multiple file patches.
    pub fn parse(input: &'a str, opts: ParseOptions) -> Self {
        // Strip email preamble once at construction
        let input = strip_email_preamble(input);
        Self {
            input,
            offset: 0,
            opts,
            finished: false,
            found_any: false,
        }
    }

    /// Creates an error with the current offset as span.
    fn error(&self, kind: PatchSetParseErrorKind) -> PatchSetParseError {
        PatchSetParseError::new(kind, self.offset..self.offset)
    }

    fn next_unidiff_patch(&mut self) -> Option<Result<FilePatch<'a, str>, PatchSetParseError>> {
        let remaining = &self.input[self.offset..];
        if remaining.is_empty() {
            return None;
        }

        let patch_start = find_patch_start(remaining)?;
        self.found_any = true;

        let patch_input = &remaining[patch_start..];

        let opts = crate::patch::parse::ParseOpts::default();
        let (result, consumed) = parse_one(patch_input, opts);
        // Always advance so the iterator makes progress even on error.
        let abs_patch_start = self.offset + patch_start;
        self.offset += patch_start + consumed;

        let patch = match result {
            Ok(patch) => patch,
            Err(e) => return Some(Err(e.into())),
        };
        let operation = match extract_file_op_unidiff(patch.original_path(), patch.modified_path())
        {
            Ok(op) => op,
            Err(mut e) => {
                e.set_span(abs_patch_start..abs_patch_start);
                return Some(Err(e));
            }
        };

        Some(Ok(FilePatch::new(operation, patch, None, None)))
    }
}

impl<'a> Iterator for PatchSet<'a> {
    type Item = Result<FilePatch<'a, str>, PatchSetParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        let result = match self.opts.format {
            Format::UniDiff => {
                let result = self.next_unidiff_patch();
                if result.is_none() {
                    self.finished = true;
                    if !self.found_any {
                        return Some(Err(self.error(PatchSetParseErrorKind::NoPatchesFound)));
                    }
                }
                result
            }
        };

        result
    }
}

/// Finds the byte offset of the first patch header in the input.
///
/// A patch header starts with `--- ` or `+++ ` (the file path lines).
/// Returns `None` if no header is found.
fn find_patch_start(input: &str) -> Option<usize> {
    let mut offset = 0;
    for line in input.lines() {
        if line.starts_with(ORIGINAL_PREFIX) || line.starts_with(MODIFIED_PREFIX) {
            return Some(offset);
        }
        offset += line.len();
        offset += line_ending_len(&input[offset..]);
    }
    None
}

/// Strips email preamble (headers and commit message) from `git format-patch` output.
///
/// Returns the content after the first `\n---\n` separator.
///
/// ## Observed git behavior
///
/// `git mailinfo` (used by `git am`) uses the first `---` line
/// as the separator between commit message and patch content.
/// It does not check if `diff --git` follows or there are more `---` lines.
///
/// From [`git format-patch`] manpage:
///
/// > The log message and the patch are separated by a line with a three-dash line.
///
/// [`git format-patch`]: https://git-scm.com/docs/git-format-patch
fn strip_email_preamble(input: &str) -> &str {
    // only strip preamble for mbox-formatted input
    if !input.starts_with("From ") {
        return input;
    }

    match input.find(EMAIL_PREAMBLE_SEPARATOR) {
        Some(pos) => &input[pos + EMAIL_PREAMBLE_SEPARATOR.len()..],
        None => input,
    }
}

/// Extracts the file operation from a patch based on its header paths.
pub(crate) fn extract_file_op_unidiff<'a>(
    original: Option<&Cow<'a, str>>,
    modified: Option<&Cow<'a, str>>,
) -> Result<FileOperation<'a>, PatchSetParseError> {
    let is_create = original.map(Cow::as_ref) == Some(DEV_NULL);
    let is_delete = modified.map(Cow::as_ref) == Some(DEV_NULL);

    if is_create && is_delete {
        return Err(PatchSetParseErrorKind::BothDevNull.into());
    }

    if is_delete {
        let path = original.ok_or(PatchSetParseErrorKind::DeleteMissingOriginalPath)?;
        Ok(FileOperation::Delete(path.clone()))
    } else if is_create {
        let path = modified.ok_or(PatchSetParseErrorKind::CreateMissingModifiedPath)?;
        Ok(FileOperation::Create(path.clone()))
    } else {
        match (original, modified) {
            (Some(original), Some(modified)) => Ok(FileOperation::Modify {
                original: original.clone(),
                modified: modified.clone(),
            }),
            (None, Some(modified)) => {
                // No original path, but has modified path.
                // Observed that GNU patch reads from the modified path in this case.
                Ok(FileOperation::Modify {
                    original: modified.clone(),
                    modified: modified.clone(),
                })
            }
            (Some(original), None) => {
                // No modified path, but has original path.
                Ok(FileOperation::Modify {
                    modified: original.clone(),
                    original: original.clone(),
                })
            }
            (None, None) => Err(PatchSetParseErrorKind::NoFilePath.into()),
        }
    }
}

/// Returns the length of the line ending at the start of `s`.
///
/// `.lines()` strips line endings, so callers tracking byte offsets need to
/// account for the `\r\n` or `\n` that was consumed.
fn line_ending_len(s: &str) -> usize {
    if s.starts_with("\r\n") {
        2
    } else if s.starts_with('\n') {
        1
    } else {
        0
    }
}
