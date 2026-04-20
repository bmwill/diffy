//! Parse multiple file patches from a unified diff.

use super::error::PatchSetParseErrorKind;
use super::FileMode;
use super::FileOperation;
use super::FilePatch;
use super::Format;
use super::ParseOptions;
use super::PatchSetParseError;
use crate::binary::parse_binary_patch;
use crate::binary::BinaryPatch;
use crate::patch::parse::parse_one;
use crate::utils::escaped_filename;
use crate::utils::Text;
use crate::Patch;

use alloc::borrow::Cow;
use alloc::string::String;

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
/// Created by [`PatchSet::parse`] or [`PatchSet::parse_bytes`].
///
/// # Example
///
/// ```
/// use diffy::patch_set::ParseOptions;
/// use diffy::patch_set::PatchSet;
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
pub struct PatchSet<'a, T: ?Sized> {
    input: &'a T,
    offset: usize,
    opts: ParseOptions,
    finished: bool,
    found_any: bool,
}

impl<'a> PatchSet<'a, str> {
    /// Creates a streaming parser for multiple file patches from a string.
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

impl<'a> PatchSet<'a, [u8]> {
    /// Creates a streaming parser for multiple file patches from raw bytes.
    ///
    /// This is useful when the diff output may contain non-UTF-8 content,
    /// such as patches produced by `git diff --binary` on files that git
    /// misdetects as text.
    pub fn parse_bytes(input: &'a [u8], opts: ParseOptions) -> Self {
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

impl<'a> Iterator for PatchSet<'a, str> {
    type Item = Result<FilePatch<'a, str>, PatchSetParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        next_patch(self)
    }
}

impl<'a> Iterator for PatchSet<'a, [u8]> {
    type Item = Result<FilePatch<'a, [u8]>, PatchSetParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        next_patch(self)
    }
}

fn next_patch<'a, T: Text + ?Sized>(
    ps: &mut PatchSet<'a, T>,
) -> Option<Result<FilePatch<'a, T>, PatchSetParseError>> {
    if ps.finished {
        return None;
    }

    let result = match ps.opts.format {
        Format::UniDiff => next_unidiff_patch(ps),
        Format::GitDiff => next_gitdiff_patch(ps),
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

fn next_unidiff_patch<'a, T: Text + ?Sized>(
    ps: &mut PatchSet<'a, T>,
) -> Option<Result<FilePatch<'a, T>, PatchSetParseError>> {
    let remaining = remaining(ps);
    if remaining.is_empty() {
        return None;
    }

    let patch_start = find_patch_start(remaining)?;
    ps.found_any = true;

    let (_, patch_input) = remaining.split_at(patch_start);

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

fn remaining<'a, T: Text + ?Sized>(ps: &PatchSet<'a, T>) -> &'a T {
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

fn next_gitdiff_patch<'a, T: Text + ?Sized>(
    ps: &mut PatchSet<'a, T>,
) -> Option<Result<FilePatch<'a, T>, PatchSetParseError>> {
    let patch_start = find_gitdiff_start(remaining(ps))?;
    ps.offset += patch_start;
    ps.found_any = true;

    let abs_patch_start = ps.offset;

    // Parse extended headers incrementally and stops at first unrecognized line
    let (header, header_consumed) = GitHeader::parse(remaining(ps));
    ps.offset += header_consumed;

    // Handle "Binary files ... differ" (no patch data)
    if header.is_binary_marker {
        // FIXME: error spans point at `diff --git` line, not the specific offending line
        let operation = match extract_file_op_binary(&header, abs_patch_start) {
            Ok(op) => op,
            Err(e) => return Some(Err(e)),
        };
        let (old_mode, new_mode) = match parse_file_modes(&header) {
            Ok(modes) => modes,
            Err(mut e) => {
                e.set_span(abs_patch_start..abs_patch_start);
                return Some(Err(e));
            }
        };
        return Some(Ok(FilePatch::new_binary(
            operation,
            BinaryPatch::Marker,
            old_mode,
            new_mode,
        )));
    }

    // Handle "GIT binary patch" (has patch data)
    if let Some(binary_patch_start) = header.binary_patch_offset {
        // GitHeader::parse consumed the marker line but not the payload.
        // Use the recorded offset to pass input from the marker onward.
        let (_, binary_input) = ps.input.split_at(abs_patch_start + binary_patch_start);
        let (binary_patch, consumed) = match parse_binary_patch(binary_input.as_bytes()) {
            Ok(result) => result,
            Err(e) => return Some(Err(e.into())),
        };
        ps.offset = abs_patch_start + binary_patch_start + consumed;

        // FIXME: error spans point at `diff --git` line, not the specific offending line
        let operation = match extract_file_op_binary(&header, abs_patch_start) {
            Ok(op) => op,
            Err(e) => return Some(Err(e)),
        };
        let (old_mode, new_mode) = match parse_file_modes(&header) {
            Ok(modes) => modes,
            Err(mut e) => {
                e.set_span(abs_patch_start..abs_patch_start);
                return Some(Err(e));
            }
        };
        return Some(Ok(FilePatch::new_binary(
            operation,
            binary_patch,
            old_mode,
            new_mode,
        )));
    }

    // `git diff` output format is stricter.
    // There is no preamble between Git headers and unidiff patch portion,
    // so we safely don't perform the preamble skipping.
    //
    // If we did, it would fail the pure rename/mode-change operation
    // since those ops have no unidiff patch portion
    // and is directly followed by the next `diff --git` header.
    let opts = crate::patch::parse::ParseOpts::default().no_skip_preamble();
    let (result, consumed) = parse_one(remaining(ps), opts);
    ps.offset += consumed;
    let patch = match result {
        Ok(patch) => patch,
        Err(e) => return Some(Err(e.into())),
    };

    // FIXME: error spans point at `diff --git` line, not the specific offending line
    let operation = match extract_file_op_gitdiff(&header, &patch) {
        Ok(op) => op,
        Err(mut e) => {
            e.set_span(abs_patch_start..abs_patch_start);
            return Some(Err(e));
        }
    };

    // FIXME: error spans point at `diff --git` line, not the specific offending line
    let (old_mode, new_mode) = match parse_file_modes(&header) {
        Ok(modes) => modes,
        Err(mut e) => {
            e.set_span(abs_patch_start..abs_patch_start);
            return Some(Err(e));
        }
    };

    Some(Ok(FilePatch::new(operation, patch, old_mode, new_mode)))
}

/// Finds the byte offset of the first `diff --git` line in `input`.
fn find_gitdiff_start<T: Text + ?Sized>(input: &T) -> Option<usize> {
    let mut offset = 0;
    for line in input.lines() {
        if line.starts_with("diff --git ") {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Git extended header metadata.
///
/// Extracted from lines between `diff --git` and `---` (or end of patch).
/// See [git-diff format documentation](https://git-scm.com/docs/diff-format).
#[derive(Debug)]
struct GitHeader<'a, T: ?Sized> {
    /// Raw content after "diff --git " prefix.
    ///
    /// Only parsed in fallback when `---`/`+++` is absent (mode-only, binary, empty file).
    diff_git_line: Option<&'a T>,
    /// Source path from `rename from <path>`.
    rename_from: Option<&'a T>,
    /// Destination path from `rename to <path>`.
    rename_to: Option<&'a T>,
    /// Source path from `copy from <path>`.
    copy_from: Option<&'a T>,
    /// Destination path from `copy to <path>`.
    copy_to: Option<&'a T>,
    /// File mode from `old mode <mode>`.
    old_mode: Option<&'a T>,
    /// File mode from `new mode <mode>`.
    new_mode: Option<&'a T>,
    /// File mode from `new file mode <mode>`.
    new_file_mode: Option<&'a T>,
    /// File mode from `deleted file mode <mode>`.
    deleted_file_mode: Option<&'a T>,
    /// Whether this is a binary diff with no actual patch content.
    ///
    /// Observed `git diff` output (without `--binary`):
    ///
    /// ```text
    /// diff --git a/image.png b/image.png
    /// new file mode 100644
    /// index 0000000..7c4530c
    /// Binary files /dev/null and b/image.png differ
    /// ```
    is_binary_marker: bool,
    /// Byte offset of `"GIT binary patch"` line relative to header input,
    /// or `None` if no binary patch content was found.
    ///
    /// Observed `git diff --binary` output:
    ///
    /// ```text
    /// diff --git a/image.png b/image.png
    /// new file mode 100644
    /// index 0000000..7c4530c
    /// GIT binary patch
    /// literal 67
    /// zcmV-J0KET+...
    ///
    /// literal 0
    /// KcmV+b0RR6000031
    /// ```
    binary_patch_offset: Option<usize>,
}

impl<T: ?Sized> Default for GitHeader<'_, T> {
    fn default() -> Self {
        Self {
            diff_git_line: None,
            rename_from: None,
            rename_to: None,
            copy_from: None,
            copy_to: None,
            old_mode: None,
            new_mode: None,
            new_file_mode: None,
            deleted_file_mode: None,
            is_binary_marker: false,
            binary_patch_offset: None,
        }
    }
}

impl<'a, T: Text + ?Sized> GitHeader<'a, T> {
    /// Parses git extended headers incrementally from the current position.
    ///
    /// Consumes the `diff --git` line and all recognized extended header lines,
    /// stopping at the first unrecognized line (typically `---`/`+++`/`@@`
    /// or the next `diff --git`).
    ///
    /// Returns the parsed header and the number of bytes consumed.
    fn parse(input: &'a T) -> (Self, usize) {
        let mut header = GitHeader::default();
        let mut consumed = 0;

        for line in input.lines() {
            let trimmed = strip_line_ending(line);

            if let Some(rest) = trimmed.strip_prefix("diff --git ") {
                // Only accept the first `diff --git` line.
                // A second one means we've reached the next patch.
                if header.diff_git_line.is_some() {
                    break;
                }
                header.diff_git_line = Some(rest);
            } else if let Some(path) = trimmed.strip_prefix("rename from ") {
                header.rename_from = Some(path);
            } else if let Some(path) = trimmed.strip_prefix("rename to ") {
                header.rename_to = Some(path);
            } else if let Some(path) = trimmed.strip_prefix("copy from ") {
                header.copy_from = Some(path);
            } else if let Some(path) = trimmed.strip_prefix("copy to ") {
                header.copy_to = Some(path);
            } else if let Some(mode) = trimmed.strip_prefix("old mode ") {
                header.old_mode = Some(mode);
            } else if let Some(mode) = trimmed.strip_prefix("new mode ") {
                header.new_mode = Some(mode);
            } else if let Some(mode) = trimmed.strip_prefix("new file mode ") {
                header.new_file_mode = Some(mode);
            } else if let Some(mode) = trimmed.strip_prefix("deleted file mode ") {
                header.deleted_file_mode = Some(mode);
            } else if trimmed.starts_with("index ")
                || trimmed.starts_with("similarity index ")
                || trimmed.starts_with("dissimilarity index ")
            {
                // Recognized but nothing to extract.
            } else if trimmed.starts_with("Binary files ") {
                header.is_binary_marker = true;
            } else if trimmed.starts_with("GIT binary patch") {
                header.binary_patch_offset = Some(consumed);
            } else {
                // Unrecognized line: End of extended headers
                // (typically `---`/`+++`/`@@` or trailing content).
                break;
            }

            consumed += line.len();
        }

        (header, consumed)
    }
}

/// Determines the file operation from git headers and patch paths.
fn extract_file_op_gitdiff<'a, T: Text + ?Sized>(
    header: &GitHeader<'a, T>,
    patch: &Patch<'a, T>,
) -> Result<FileOperation<'a, T>, PatchSetParseError> {
    // Git headers are authoritative for rename/copy.
    // Paths may be quoted (e.g., `rename from "foo\tbar.txt"`).
    if let (Some(from), Some(to)) = (header.rename_from, header.rename_to) {
        return Ok(FileOperation::Rename {
            from: escaped_filename(from)?,
            to: escaped_filename(to)?,
        });
    }
    if let (Some(from), Some(to)) = (header.copy_from, header.copy_to) {
        return Ok(FileOperation::Copy {
            from: escaped_filename(from)?,
            to: escaped_filename(to)?,
        });
    }

    // Try ---/+++ paths first
    if patch.original().is_some() || patch.modified().is_some() {
        return extract_file_op_unidiff(patch.original_path(), patch.modified_path());
    }

    // Fall back to `diff --git <old> <new>` for mode-only and empty file changes
    let Some((original, modified)) = header.diff_git_line.and_then(parse_diff_git_path) else {
        return Err(PatchSetParseErrorKind::InvalidDiffGitPath.into());
    };

    if header.new_file_mode.is_some() {
        Ok(FileOperation::Create(modified))
    } else if header.deleted_file_mode.is_some() {
        Ok(FileOperation::Delete(original))
    } else {
        Ok(FileOperation::Modify { original, modified })
    }
}

/// Parses file modes from git extended headers.
fn parse_file_modes<T: Text + ?Sized>(
    header: &GitHeader<'_, T>,
) -> Result<(Option<FileMode>, Option<FileMode>), PatchSetParseError> {
    let parse_mode = |mode: &T| -> Result<FileMode, PatchSetParseError> {
        mode.as_str()
            .ok_or_else(|| {
                let s = String::from_utf8_lossy(mode.as_bytes()).into_owned();
                PatchSetParseErrorKind::InvalidFileMode(s)
            })?
            .parse::<FileMode>()
    };
    let old_mode = header
        .old_mode
        .or(header.deleted_file_mode)
        .map(parse_mode)
        .transpose()?;
    let new_mode = header
        .new_mode
        .or(header.new_file_mode)
        .map(parse_mode)
        .transpose()?;
    Ok((old_mode, new_mode))
}

/// Extracts both old and new paths from `diff --git` line content.
///
/// ## Assumption #1: old and new paths are the same
///
/// This extraction has one strong assumption:
/// Beside their prefixes, old and new paths are the same.
///
/// From [git-diff format documentation]:
///
/// > The `a/` and `b/` filenames are the same unless rename/copy is involved.
/// > Especially, even for a creation or a deletion, `/dev/null` is not used
/// > in place of the `a/` or `b/` filenames.
/// >
/// > When a rename/copy is involved, file1 and file2 show the name of the
/// > source file of the rename/copy and the name of the file that the
/// > rename/copy produces, respectively.
///
/// Since rename/copy operations use `rename from/to` and `copy from/to` headers
/// we have handled earlier in [`extract_file_op_gitdiff`],
/// (which have no `a/`/`b/` prefix per git spec),
///
/// this extraction is only used
/// * when unified diff headers (`---`/`+++`) are absent
/// * Only for mode-only and empty file cases
///
/// [git-diff format documentation]: https://git-scm.com/docs/diff-format
///
/// ## Assumption #2: the longest common path suffix is the shared path
///
/// When custom prefixes contain spaces,
/// multiple splits may produce valid path suffixes.
///
/// Example: `src/foo.rs src/foo.rs src/foo.rs src/foo.rs`
///
/// Three splits all produce valid path suffixes (contain `/`):
///
/// * Position 10
///   * old path: `src/foo.rs`
///   * new path: `src/foo.rs src/foo.rs src/foo.rs`
///   * common suffix: `foo.rs`
/// * Position 21
///   * old path: `src/foo.rs src/foo.rs`
///   * new path: `src/foo.rs src/foo.rs`
///   * common suffix: `foo.rs src/foo.rs`
/// * Position 32
///   * old path: `src/foo.rs src/foo.rs src/foo.rs`
///   * new path: `src/foo.rs`
///   * common suffix: `foo.rs`
///
/// We observed that `git apply` would pick position 21,
/// which has the longest path suffix,
/// hence this heuristic.
///
/// ## Supported formats
///
/// * `a/<path> b/<path>` (default prefix)
/// * `<path> <path>` (`git diff --no-prefix`)
/// * `<src-prefix><path> <dst-prefix><path>` (custom prefix)
/// * `"<prefix><path>" "<prefix><path>"` (quoted, with escapes)
/// * Mixed quoted/unquoted
fn parse_diff_git_path<'a, T: Text + ?Sized>(line: &'a T) -> Option<(Cow<'a, T>, Cow<'a, T>)> {
    if line.starts_with("\"") || line.ends_with("\"") {
        parse_quoted_diff_git_path(line)
    } else {
        parse_unquoted_diff_git_path(line)
    }
}

/// See [`parse_diff_git_path`].
fn parse_unquoted_diff_git_path<'a, T: Text + ?Sized>(
    line: &'a T,
) -> Option<(Cow<'a, T>, Cow<'a, T>)> {
    let bytes = line.as_bytes();
    let mut best_match = None;
    let mut longest_path_len = 0;

    for (i, _) in bytes.iter().enumerate().filter(|(_, &b)| b == b' ') {
        let (left, right_with_space) = line.split_at(i);
        // skip the space
        let (_, right) = right_with_space.split_at(1);
        if left.is_empty() || right.is_empty() {
            continue;
        }
        // Select split with longest common path suffix.
        // On ties (`>` not `>=`), the first (leftmost) split wins.
        //
        // Observed: `git apply` rejects ambiguous splits:
        //
        // > git diff header lacks filename information
        // > when removing N leading pathname component(s)"
        //
        // Also in <https://git-scm.com/docs/diff-format#generate_patch_text_with_p>:
        //
        // > The a/ and b/ filenames are the same unless rename/copy is involved.
        //
        // This kinda tells git-apply's path resolution is strip-level-aware,
        // unlike ours.
        //
        // See `fail_ambiguous_suffix_tie` compat test.
        if let Some(path) = longest_common_path_suffix(left, right) {
            if path.len() > longest_path_len {
                longest_path_len = path.len();
                best_match = Some((left, right));
            }
        }
    }

    best_match.map(|(l, r)| (Cow::Borrowed(l), Cow::Borrowed(r)))
}

/// See [`parse_diff_git_path`].
fn parse_quoted_diff_git_path<'a, T: Text + ?Sized>(
    line: &'a T,
) -> Option<(Cow<'a, T>, Cow<'a, T>)> {
    let (left_raw, right_raw) = if line.starts_with("\"") {
        // First token is quoted.
        let bytes = line.as_bytes();
        let mut i = 1; // skip starting `"`

        // Find the closing `"`.
        // The only escape where literal `"` appears right after `\` is `\"`,
        // an octal double quote `\042` has 3 digits.
        // So, `i += 2` correctly skips past `"` and octal digits.
        let end = loop {
            match bytes.get(i)? {
                b'"' => break i + 1,
                b'\\' => i += 2,
                _ => i += 1,
            }
        };
        let (first, rest) = line.split_at(end);
        let rest = rest.strip_prefix(" ")?;
        (first, rest)
    } else if let Some(pos) = line.find(" \"") {
        // First token is unquoted. The second must be quoted.
        let (left, rest) = line.split_at(pos);
        let (_, right) = rest.split_at(1); // skip the space
        (left, right)
    } else {
        // Malformed: ends with `"` but no valid quoted path found
        return None;
    };

    let left = escaped_filename(left_raw).ok()?;
    let right = escaped_filename(right_raw).ok()?;

    // Verify both sides share the same path.
    longest_common_path_suffix(left.as_ref(), right.as_ref())?;
    Some((left, right))
}

/// Extracts the longest common path suffix shared by `a` and `b`.
///
/// Returns `None` if no valid common path exists.
///
/// * If both strings are identical, returns the whole string
///   (e.g., `file.rs` vs `file.rs` → `file.rs`).
/// * Otherwise, returns the portion after the first `/` in the common suffix
///   (e.g., `foo/bar.rs` vs `fooo/bar.rs` → `bar.rs`).
fn longest_common_path_suffix<'a, T: Text + ?Sized>(a: &'a T, b: &T) -> Option<&'a T> {
    if a.is_empty() || b.is_empty() {
        return None;
    }

    let mut last_slash = None;
    let mut matched = 0;

    for (i, (x, y)) in a
        .as_bytes()
        .iter()
        .rev()
        .zip(b.as_bytes().iter().rev())
        .enumerate()
    {
        if x != y {
            break;
        }
        // `/` is ASCII,
        // so this index is always a valid split point UTF-8 strings.
        // No char boundary check needed.
        if *x == b'/' {
            last_slash = Some(i + 1);
        }
        matched = i + 1;
    }

    if matched == 0 {
        return None;
    }

    // Identical strings
    if matched == a.len() && a.len() == b.len() {
        return Some(a);
    }

    // Return the path after the outermost `/` in the common suffix.
    let suffix_len = last_slash?;
    let start = a.len() - suffix_len + 1; // skip the '/'
    let (_, path) = a.split_at(start);
    (!path.is_empty()).then_some(path)
}

/// Extracts the file operation for a binary patch from git headers.
///
/// Binary patches have no `---`/`+++` headers, so paths come from the
/// `diff --git` line or rename/copy headers.
fn extract_file_op_binary<'a, T: Text + ?Sized>(
    header: &GitHeader<'a, T>,
    abs_patch_start: usize,
) -> Result<FileOperation<'a, T>, PatchSetParseError> {
    // Git headers are authoritative for rename/copy.
    // Paths may be quoted (e.g., `rename from "foo\tbar.txt"`).
    if let (Some(from), Some(to)) = (header.rename_from, header.rename_to) {
        return Ok(FileOperation::Rename {
            from: escaped_filename(from)?,
            to: escaped_filename(to)?,
        });
    }
    if let (Some(from), Some(to)) = (header.copy_from, header.copy_to) {
        return Ok(FileOperation::Copy {
            from: escaped_filename(from)?,
            to: escaped_filename(to)?,
        });
    }

    let Some((original, modified)) = header.diff_git_line.and_then(parse_diff_git_path) else {
        return Err(PatchSetParseError::new(
            PatchSetParseErrorKind::InvalidDiffGitPath,
            abs_patch_start..abs_patch_start,
        ));
    };

    if header.new_file_mode.is_some() {
        Ok(FileOperation::Create(modified))
    } else if header.deleted_file_mode.is_some() {
        Ok(FileOperation::Delete(original))
    } else {
        Ok(FileOperation::Modify { original, modified })
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

/// Strips the trailing `\n` from a line yielded by [`Text::lines`].
///
/// [`Text::lines`] includes line endings; strip for matching.
fn strip_line_ending<T: Text + ?Sized>(line: &T) -> &T {
    // TODO: GNU patch strips trailing CRs from CRLF patches automatically.
    // We should consider adding compat tests for GNU patch.
    // And `git apply` seems to reject. Worth adding tests as well.
    line.strip_suffix("\n").unwrap_or(line)
}
