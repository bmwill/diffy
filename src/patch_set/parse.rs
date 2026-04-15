//! Parse multiple file patches from a unified diff.

use super::{
    error::PatchSetParseErrorKind, FileOperation, FilePatch, Format, ParseOptions,
    PatchSetParseError,
};
use crate::patch::parse::parse_one;
use crate::utils::Text;

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
}

impl<'a> Iterator for PatchSet<'a> {
    type Item = Result<FilePatch<'a, str>, PatchSetParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        next_patch(self)
    }
}

fn next_patch<'a>(ps: &mut PatchSet<'a>) -> Option<Result<FilePatch<'a, str>, PatchSetParseError>> {
    if ps.finished {
        return None;
    }

    let result = match ps.opts.format {
        Format::UniDiff => next_unidiff_patch(ps),
    };

    if result.is_none() {
        ps.finished = true;
        if !ps.found_any {
            let err = PatchSetParseError::new(
                PatchSetParseErrorKind::NoPatchesFound,
                ps.offset..ps.offset,
            );
            return Some(Err(err));
        }
    }

    result
}

fn next_unidiff_patch<'a>(
    ps: &mut PatchSet<'a>,
) -> Option<Result<FilePatch<'a, str>, PatchSetParseError>> {
    let remaining = remaining(ps);
    if remaining.is_empty() {
        return None;
    }

    let patch_start = find_patch_start(remaining)?;
    ps.found_any = true;

    let patch_input = &remaining[patch_start..];

    let opts = crate::patch::parse::ParseOpts::default();
    let (result, consumed) = parse_one(patch_input, opts);
    // Always advance so the iterator makes progress even on error.
    let abs_patch_start = ps.offset + patch_start;
    ps.offset += patch_start + consumed;

    let patch = match result {
        Ok(patch) => patch,
        Err(e) => return Some(Err(e.into())),
    };
    let operation = match extract_file_op_unidiff(patch.original_path(), patch.modified_path()) {
        Ok(op) => op,
        Err(mut e) => {
            e.set_span(abs_patch_start..abs_patch_start);
            return Some(Err(e));
        }
    };

    Some(Ok(FilePatch::new(operation, patch, None, None)))
}

fn remaining<'a>(ps: &PatchSet<'a>) -> &'a str {
    let (_, rest) = ps.input.split_at(ps.offset);
    rest
}

/// Finds the byte offset of the first patch header in the input.
///
/// A patch header starts with `--- ` or `+++ ` (the file path lines).
/// Returns `None` if no header is found.
fn find_patch_start<T: Text + ?Sized>(input: &T) -> Option<usize> {
    let mut offset = 0;
    for line in input.lines() {
        if line.starts_with(ORIGINAL_PREFIX) || line.starts_with(MODIFIED_PREFIX) {
            return Some(offset);
        }
        offset += line.len();
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
fn strip_email_preamble<T: Text + ?Sized>(input: &T) -> &T {
    // only strip preamble for mbox-formatted input
    if !input.starts_with("From ") {
        return input;
    }

    match input.find(EMAIL_PREAMBLE_SEPARATOR) {
        Some(pos) => {
            let (_, rest) = input.split_at(pos + EMAIL_PREAMBLE_SEPARATOR.len());
            rest
        }
        None => input,
    }
}

/// Extracts the file operation from a patch based on its header paths.
fn extract_file_op_unidiff<'a, T: Text + ?Sized>(
    original: Option<&Cow<'a, T>>,
    modified: Option<&Cow<'a, T>>,
) -> Result<FileOperation<'a, T>, PatchSetParseError> {
    let is_dev_null = |cow: &Cow<'_, T>| cow.as_ref().as_bytes() == DEV_NULL.as_bytes();

    let is_create = original.is_some_and(is_dev_null);
    let is_delete = modified.is_some_and(is_dev_null);

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
