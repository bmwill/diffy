//! Utilities for parsing unified diff patches containing multiple files.
//!
//! This module provides [`PatchSet`] for parsing patches that contain changes
//! to multiple files, like the output of `git diff` or `git format-patch`.

pub(crate) mod error;
mod parse;
#[cfg(test)]
mod tests;

use alloc::borrow::Cow;
use alloc::borrow::ToOwned;
use core::fmt;

use crate::binary::BinaryPatch;
use crate::utils::Text;
use crate::Patch;

pub use error::PatchSetParseError;
use error::PatchSetParseErrorKind;
pub use parse::PatchSet;

/// Options for parsing patch content.
///
/// Use [`ParseOptions::unidiff()`] or [`ParseOptions::gitdiff()`]
/// to create options for the desired format.
///
/// ## Binary Files
///
/// When parsing git diffs, binary file changes are detected by:
///
/// * `Binary files a/path and b/path differ` (`git diff` without `--binary` flag)
/// * `GIT binary patch` (from `git diff --binary`)
///
/// Note that this is not a documented Git behavior,
/// so the implementation here is subject to change if Git changes.
///
/// ## Example
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
/// ";
///
/// let patches: Vec<_> = PatchSet::parse(s, ParseOptions::unidiff())
///     .collect::<Result<_, _>>()
///     .unwrap();
/// assert_eq!(patches.len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub(crate) format: Format,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Format {
    /// Standard unified diff format.
    UniDiff,
    /// Git extended diff format.
    GitDiff,
}

impl ParseOptions {
    /// Parse as standard [unified diff] format.
    ///
    /// Supported:
    ///
    /// * `---`/`+++` file headers
    /// * `@@ ... @@` hunks
    /// * modify and rename files
    /// * create files (`--- /dev/null`)
    /// * delete files (`+++ /dev/null`)
    /// * Skip preamble, headers, and email signature trailer
    ///
    /// [unified diff]: https://www.gnu.org/software/diffutils/manual/html_node/Unified-Format.html
    pub fn unidiff() -> Self {
        Self {
            format: Format::UniDiff,
        }
    }

    /// Parse as [git extended diff format][git-diff-format].
    ///
    /// Supports all features of [`unidiff()`](Self::unidiff) plus:
    ///
    /// * `diff --git` headers
    /// * Extended headers (`new file mode`, `deleted file mode`, etc.)
    /// * Rename/copy detection (`rename from`/`rename to`, `copy from`/`copy to`)
    /// * Binary file detection
    ///
    /// [git-diff-format]: https://git-scm.com/docs/diff-format
    pub fn gitdiff() -> Self {
        Self {
            format: Format::GitDiff,
        }
    }
}

/// File mode extracted from git extended headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    /// `100644` regular file
    Regular,
    /// `100755` executable file
    Executable,
    /// `120000` symlink
    Symlink,
    /// `160000` gitlink (submodule)
    Gitlink,
}

impl core::str::FromStr for FileMode {
    type Err = PatchSetParseError;

    fn from_str(mode: &str) -> Result<Self, Self::Err> {
        match mode {
            "100644" => Ok(Self::Regular),
            "100755" => Ok(Self::Executable),
            "120000" => Ok(Self::Symlink),
            "160000" => Ok(Self::Gitlink),
            _ => Err(PatchSetParseErrorKind::InvalidFileMode(mode.to_owned()).into()),
        }
    }
}

/// The kind of patch content in a [`FilePatch`].
#[derive(Clone, PartialEq, Eq)]
pub enum PatchKind<'a, T: ToOwned + ?Sized> {
    /// Text patch with hunks.
    Text(Patch<'a, T>),
    /// Binary patch (literal or delta encoded, or marker-only).
    Binary(BinaryPatch<'a>),
}

impl<T: ?Sized, O> fmt::Debug for PatchKind<'_, T>
where
    T: ToOwned<Owned = O> + fmt::Debug,
    O: core::borrow::Borrow<T> + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatchKind::Text(patch) => f.debug_tuple("Text").field(patch).finish(),
            PatchKind::Binary(patch) => f.debug_tuple("Binary").field(patch).finish(),
        }
    }
}

impl<'a, T: ToOwned + ?Sized> PatchKind<'a, T> {
    /// Returns the text patch, or `None` if this is a binary patch.
    pub fn as_text(&self) -> Option<&Patch<'a, T>> {
        match self {
            PatchKind::Text(patch) => Some(patch),
            PatchKind::Binary(_) => None,
        }
    }

    /// Returns the binary patch, or `None` if this is a text patch.
    pub fn as_binary(&self) -> Option<&BinaryPatch<'a>> {
        match self {
            PatchKind::Binary(patch) => Some(patch),
            PatchKind::Text(_) => None,
        }
    }

    /// Returns `true` if this is a binary diff.
    pub fn is_binary(&self) -> bool {
        matches!(self, PatchKind::Binary(_))
    }
}

/// A single file's patch with operation metadata.
///
/// This combines a [`PatchKind`] with a [`FileOperation`]
/// that indicates what kind of file operation this patch represents
/// (create, delete, modify, or rename).
#[derive(Clone, PartialEq, Eq)]
pub struct FilePatch<'a, T: ToOwned + ?Sized> {
    operation: FileOperation<'a, T>,
    kind: PatchKind<'a, T>,
    old_mode: Option<FileMode>,
    new_mode: Option<FileMode>,
}

impl<T: ?Sized, O> fmt::Debug for FilePatch<'_, T>
where
    T: ToOwned<Owned = O> + fmt::Debug,
    O: core::borrow::Borrow<T> + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FilePatch")
            .field("operation", &self.operation)
            .field("kind", &self.kind)
            .field("old_mode", &self.old_mode)
            .field("new_mode", &self.new_mode)
            .finish()
    }
}

impl<'a, T: ToOwned + ?Sized> FilePatch<'a, T> {
    fn new(
        operation: FileOperation<'a, T>,
        patch: Patch<'a, T>,
        old_mode: Option<FileMode>,
        new_mode: Option<FileMode>,
    ) -> Self {
        Self {
            operation,
            kind: PatchKind::Text(patch),
            old_mode,
            new_mode,
        }
    }

    fn new_binary(
        operation: FileOperation<'a, T>,
        patch: BinaryPatch<'a>,
        old_mode: Option<FileMode>,
        new_mode: Option<FileMode>,
    ) -> Self {
        Self {
            operation,
            kind: PatchKind::Binary(patch),
            old_mode,
            new_mode,
        }
    }

    /// Returns the file operation for this patch.
    pub fn operation(&self) -> &FileOperation<'a, T> {
        &self.operation
    }

    /// Returns the patch content.
    pub fn patch(&self) -> &PatchKind<'a, T> {
        &self.kind
    }

    /// Consumes the [`FilePatch`] and returns the underlying [`PatchKind`].
    pub fn into_patch(self) -> PatchKind<'a, T> {
        self.kind
    }

    /// Returns the file mode before applying this patch (when known).
    ///
    /// This is typically populated for
    ///
    /// * mode changes (`old mode <mode>` header)
    /// * deletions (`deleted file mode <mode>` header)
    pub fn old_mode(&self) -> Option<&FileMode> {
        self.old_mode.as_ref()
    }

    /// Returns the file mode **after** applying this patch (when known).
    ///
    /// This is typically populated for
    ///
    /// * mode changes (the `new mode <mode>` header)
    /// * creations (the `new file mode <mode>` header)
    pub fn new_mode(&self) -> Option<&FileMode> {
        self.new_mode.as_ref()
    }
}

/// The operation to perform based on a patch.
///
/// This is determined by examining the `---` and `+++` header lines
/// of a unified diff patch, and git extended headers when available.
#[derive(PartialEq, Eq)]
pub enum FileOperation<'a, T: ToOwned + ?Sized> {
    /// Delete a file (`+++ /dev/null`).
    Delete(Cow<'a, T>),
    /// Create a new file (`--- /dev/null`).
    Create(Cow<'a, T>),
    /// Modify a file.
    ///
    /// * If `original == modified`, this is an in-place modification.
    /// * If they differ, the caller decides how to handle, e.g., treat as rename or error.
    ///
    /// Usually, the caller needs to strip the prefix from the paths to determine.
    Modify {
        original: Cow<'a, T>,
        modified: Cow<'a, T>,
    },
    /// Rename a file (move from `from` to `to`, delete `from`).
    ///
    /// Only produced when git extended headers explicitly indicate a rename.
    Rename { from: Cow<'a, T>, to: Cow<'a, T> },
    /// Copy a file (copy from `from` to `to`, keep `from`).
    ///
    /// Only produced when git extended headers explicitly indicate a copy.
    Copy { from: Cow<'a, T>, to: Cow<'a, T> },
}

impl<T: ToOwned + ?Sized> Clone for FileOperation<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Delete(p) => Self::Delete(p.clone()),
            Self::Create(p) => Self::Create(p.clone()),
            Self::Modify { original, modified } => Self::Modify {
                original: original.clone(),
                modified: modified.clone(),
            },
            Self::Rename { from, to } => Self::Rename {
                from: from.clone(),
                to: to.clone(),
            },
            Self::Copy { from, to } => Self::Copy {
                from: from.clone(),
                to: to.clone(),
            },
        }
    }
}

impl<T: ?Sized, O> fmt::Debug for FileOperation<'_, T>
where
    T: ToOwned<Owned = O> + fmt::Debug,
    O: core::borrow::Borrow<T> + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Delete(p) => f.debug_tuple("Delete").field(p).finish(),
            Self::Create(p) => f.debug_tuple("Create").field(p).finish(),
            Self::Modify { original, modified } => f
                .debug_struct("Modify")
                .field("original", original)
                .field("modified", modified)
                .finish(),
            Self::Rename { from, to } => f
                .debug_struct("Rename")
                .field("from", from)
                .field("to", to)
                .finish(),
            Self::Copy { from, to } => f
                .debug_struct("Copy")
                .field("from", from)
                .field("to", to)
                .finish(),
        }
    }
}

impl<T: Text + ?Sized> FileOperation<'_, T> {
    /// Strip the first `n` path components from the paths in this operation.
    ///
    /// This is similar to the `-p` option in GNU patch. For example,
    /// `strip_prefix(1)` on a path `a/src/lib.rs` would return `src/lib.rs`.
    pub fn strip_prefix(&self, n: usize) -> FileOperation<'_, T> {
        fn strip<T: Text + ?Sized>(path: &T, n: usize) -> &T {
            let mut remaining = path;
            for _ in 0..n {
                match remaining.split_at_exclusive("/") {
                    Some((_first, rest)) => remaining = rest,
                    None => return remaining,
                }
            }
            remaining
        }

        match self {
            FileOperation::Delete(path) => FileOperation::Delete(Cow::Borrowed(strip(path, n))),
            FileOperation::Create(path) => FileOperation::Create(Cow::Borrowed(strip(path, n))),
            FileOperation::Modify { original, modified } => FileOperation::Modify {
                original: Cow::Borrowed(strip(original, n)),
                modified: Cow::Borrowed(strip(modified, n)),
            },
            FileOperation::Rename { from, to } => FileOperation::Rename {
                from: Cow::Borrowed(strip(from, n)),
                to: Cow::Borrowed(strip(to, n)),
            },
            FileOperation::Copy { from, to } => FileOperation::Copy {
                from: Cow::Borrowed(strip(from, n)),
                to: Cow::Borrowed(strip(to, n)),
            },
        }
    }

    /// Returns `true` if this is a file creation operation.
    pub fn is_create(&self) -> bool {
        matches!(self, FileOperation::Create(_))
    }

    /// Returns `true` if this is a file deletion operation.
    pub fn is_delete(&self) -> bool {
        matches!(self, FileOperation::Delete(_))
    }

    /// Returns `true` if this is a file modification.
    pub fn is_modify(&self) -> bool {
        matches!(self, FileOperation::Modify { .. })
    }

    /// Returns `true` if this is a rename operation.
    pub fn is_rename(&self) -> bool {
        matches!(self, FileOperation::Rename { .. })
    }

    /// Returns `true` if this is a copy operation.
    pub fn is_copy(&self) -> bool {
        matches!(self, FileOperation::Copy { .. })
    }
}
