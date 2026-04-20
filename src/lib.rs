//! Tools for finding and manipulating differences between files
//!
//! ## Overview
//!
//! This library is intended to be a collection of tools used to find and
//! manipulate differences between files inspired by [LibXDiff] and [GNU
//! Diffutils]. Version control systems like [Git] and [Mercurial] generally
//! communicate differences between two versions of a file using a `diff` or
//! `patch`.
//!
//! The current diff implementation is based on the [Myers' diff algorithm].
//!
//! The documentation generally refers to "files" in many places but none of
//! the apis explicitly operate on on-disk files. Instead this library
//! requires that the text being operated on resides in-memory and as such if
//! you want to perform operations on files, it is up to the user to load the
//! contents of those files into memory before passing their contents to the
//! apis provided by this library.
//!
//! ## Cargo Feature Flags
//!
//! This crate is `no_std` by default.
//! Enable [Cargo features] as needed:
//!
//! - `std` for writer-based formatting and `std::error::Error` impls
//! - `color` for ANSI-colored patch formatting
//! - `binary` for applying parsed git binary patches
//!
//! ## UTF-8 and Non-UTF-8
//!
//! This library has support for working with both utf8 and non-utf8 texts.
//! Most of the API's have two different variants, one for working with utf8
//! `str` texts (e.g. [`create_patch`]) and one for working with bytes `[u8]`
//! which may or may not be utf8 (e.g. [`create_patch_bytes`]).
//!
//! ## Creating a Patch
//!
//! A [`Patch`] between two texts can be created by doing the following:
//!
//! ```
//! use diffy::create_patch;
//!
//! let original = "The Way of Kings\nWords of Radiance\n";
//! let modified = "The Way of Kings\nWords of Radiance\nOathbringer\n";
//!
//! let patch = create_patch(original, modified);
//! #
//! # let expected = "\
//! # --- original
//! # +++ modified
//! # @@ -1,2 +1,3 @@
//! #  The Way of Kings
//! #  Words of Radiance
//! # +Oathbringer
//! # ";
//! #
//! # assert_eq!(patch.to_string(), expected);
//! ```
//!
//! A [`Patch`] can the be output in the [Unified Format] either by using its
//! [`Display`] impl or by using a [`PatchFormatter`] to output the diff with
//! color (requires the `color` feature).
//!
//! ```
//! # use diffy::create_patch;
//! #
//! # let original = "The Way of Kings\nWords of Radiance\n";
//! # let modified = "The Way of Kings\nWords of Radiance\nOathbringer\n";
//! #
//! # let patch = create_patch(original, modified);
//! #
//! # let expected = "\
//! # --- original
//! # +++ modified
//! # @@ -1,2 +1,3 @@
//! #  The Way of Kings
//! #  Words of Radiance
//! # +Oathbringer
//! # ";
//! #
//! # assert_eq!(patch.to_string(), expected);
//! #
//! // Without color
//! print!("{}", patch);
//! ```
//!
//! With the `color` feature enabled:
//!
//! ```ignore
//! use diffy::PatchFormatter;
//! let f = PatchFormatter::new().with_color();
//! print!("{}", f.fmt_patch(&patch));
//! ```
//!
//! ```console
//! --- original
//! +++ modified
//! @@ -1,2 +1,3 @@
//!  The Way of Kings
//!  Words of Radiance
//! +Oathbringer
//! ```
//!
//! ## Applying a Patch
//!
//! Once you have a [`Patch`] you can apply it to a base image in order to
//! recover the new text. Each hunk will be applied to the base image in
//! sequence. Similarly to GNU `patch`, this implementation can detect when
//! line numbers specified in the patch are incorrect and will attempt to find
//! the correct place to apply each hunk by iterating forward and backward
//! from the given position until all context lines from a hunk match the base
//! image.
//!
//! ```
//! use diffy::apply;
//! use diffy::Patch;
//!
//! let s = "\
//! --- a/skybreaker-ideals
//! +++ b/skybreaker-ideals
//! @@ -10,6 +10,8 @@
//!  First:
//!      Life before death,
//!      strength before weakness,
//!      journey before destination.
//!  Second:
//! -    I will put the law before all else.
//! +    I swear to seek justice,
//! +    to let it guide me,
//! +    until I find a more perfect Ideal.
//! ";
//!
//! let patch = Patch::from_str(s).unwrap();
//!
//! let base_image = "\
//! First:
//!     Life before death,
//!     strength before weakness,
//!     journey before destination.
//! Second:
//!     I will put the law before all else.
//! ";
//!
//! let expected = "\
//! First:
//!     Life before death,
//!     strength before weakness,
//!     journey before destination.
//! Second:
//!     I swear to seek justice,
//!     to let it guide me,
//!     until I find a more perfect Ideal.
//! ";
//!
//! assert_eq!(apply(base_image, &patch).unwrap(), expected);
//! ```
//!
//! ## Parsing Multi-File Patches
//!
//! The [`patch_set`] module provides support for parsing unified diffs
//! that contain changes to multiple files,
//! such as `git diff` and `git format-patch` output.
//! [`PatchSet`] is a streaming iterator,
//! so callers can process file patches one at a time.
//!
//! Use [`ParseOptions::gitdiff`] for git-style diffs or
//! [`ParseOptions::unidiff`] for plain unified diffs.
//!
//! ```
//! use diffy::apply;
//! use diffy::patch_set::FileOperation;
//! use diffy::patch_set::ParseOptions;
//! use diffy::patch_set::PatchSet;
//!
//! let input = "\
//! diff --git a/alpha.txt b/alpha.txt
//! index 1111111..2222222 100644
//! --- a/alpha.txt
//! +++ b/alpha.txt
//! @@ -1 +1 @@
//! -alpha
//! +ALPHA
//! diff --git a/beta.txt b/beta.txt
//! new file mode 100644
//! --- /dev/null
//! +++ b/beta.txt
//! @@ -0,0 +1 @@
//! +beta
//! ";
//!
//! let mut patches = PatchSet::parse(input, ParseOptions::gitdiff());
//!
//! let first = patches.next().unwrap().unwrap();
//! let second = patches.next().unwrap().unwrap();
//! assert!(patches.next().is_none());
//!
//! match first.operation().strip_prefix(1) {
//!     FileOperation::Modify { original, modified } => {
//!         assert_eq!(original, "alpha.txt");
//!         assert_eq!(modified, "alpha.txt");
//!     }
//!     operation => panic!("unexpected operation: {operation:?}"),
//! }
//!
//! let text_patch = first.patch().as_text().unwrap();
//! assert_eq!(apply("alpha\n", text_patch).unwrap(), "ALPHA\n");
//!
//! match second.operation().strip_prefix(1) {
//!     FileOperation::Create(path) => assert_eq!(path, "beta.txt"),
//!     operation => panic!("unexpected operation: {operation:?}"),
//! }
//! ```
//!
//! With the `binary` Cargo feature enabled,
//! parsed multi-file patches can also contain [`BinaryPatch`] values.
//! You can apply them with [`BinaryPatch::apply`].
//!
//! ## Performing a Three-way Merge
//!
//! Two files `A` and `B` can be merged together given a common ancestor or
//! original file `O` to produce a file `C` similarly to how [diff3]
//! performs a three-way merge.
//!
//! ```console
//!     --- A ---
//!   /           \
//!  /             \
//! O               C
//!  \             /
//!   \           /
//!     --- B ---
//! ```
//!
//! If files `A` and `B` modified different regions of the original file `O`
//! (or the same region in the same way) then the files can be merged without
//! conflict.
//!
//! ```
//! use diffy::merge;
//!
//! let original = "the final empire\nThe Well of Ascension\nThe hero of ages\n";
//! let a = "The Final Empire\nThe Well of Ascension\nThe Hero of Ages\n";
//! let b = "The Final Empire\nThe Well of Ascension\nThe hero of ages\n";
//! let expected = "\
//! The Final Empire
//! The Well of Ascension
//! The Hero of Ages
//! ";
//!
//! assert_eq!(merge(original, a, b).unwrap(), expected);
//! ```
//!
//! If both files `A` and `B` modified the same region of the original file
//! `O` (and those modifications are different), it would result in a conflict
//! as it is not clear which modifications should be used in the merged
//! result.
//!
//! ```
//! use diffy::merge;
//!
//! let original = "The Final Empire\nThe Well of Ascension\nThe hero of ages\n";
//! let a = "The Final Empire\nThe Well of Ascension\nThe Hero of Ages\nSecret History\n";
//! let b = "The Final Empire\nThe Well of Ascension\nThe hero of ages\nThe Alloy of Law\n";
//! let expected = "\
//! The Final Empire
//! The Well of Ascension
//! <<<<<<< ours
//! The Hero of Ages
//! Secret History
//! ||||||| original
//! The hero of ages
//! =======
//! The hero of ages
//! The Alloy of Law
//! >>>>>>> theirs
//! ";
//!
//! assert_eq!(merge(original, a, b).unwrap_err(), expected);
//! ```
//!
//! [LibXDiff]: http://www.xmailserver.org/xdiff-lib.html
//! [Myers' diff algorithm]: http://www.xmailserver.org/diff2.pdf
//! [GNU Diffutils]: https://www.gnu.org/software/diffutils/
//! [Git]: https://git-scm.com/
//! [Mercurial]: https://www.mercurial-scm.org/
//! [Unified Format]: https://en.wikipedia.org/wiki/Diff#Unified_format
//! [diff3]: https://en.wikipedia.org/wiki/Diff3
//! [Cargo features]: https://doc.rust-lang.org/cargo/reference/features.html
//!
//! [`BinaryPatch`]: crate::binary::BinaryPatch
//! [`BinaryPatch::apply`]: crate::binary::BinaryPatch::apply
//! [`Display`]: core::fmt::Display
//! [`ParseOptions::gitdiff`]: crate::patch_set::ParseOptions::gitdiff
//! [`ParseOptions::unidiff`]: crate::patch_set::ParseOptions::unidiff
//! [`Patch`]: crate::Patch
//! [`PatchFormatter`]: crate::PatchFormatter
//! [`PatchKind::as_binary`]: crate::patch_set::PatchKind::as_binary
//! [`PatchSet`]: crate::patch_set::PatchSet
//! [`PatchSet::parse`]: crate::patch_set::PatchSet::parse
//! [`PatchSet::parse_bytes`]: crate::patch_set::PatchSet::parse_bytes
//! [`create_patch`]: crate::create_patch
//! [`create_patch_bytes`]: crate::create_patch_bytes
//! [`patch_set`]: crate::patch_set

// unconditionally define as no_std to have consistency on the prelude that is auto imported.
#![no_std]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![warn(clippy::alloc_instead_of_core)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod apply;
pub mod binary;
mod diff;
mod merge;
mod patch;
pub mod patch_set;
mod range;
mod utils;

pub use apply::apply;
pub use apply::apply_bytes;
pub use apply::ApplyError;
pub use diff::create_patch;
pub use diff::create_patch_bytes;
pub use diff::DiffOptions;
pub use merge::merge;
pub use merge::merge_bytes;
pub use merge::ConflictStyle;
pub use merge::MergeOptions;
pub use patch::Hunk;
pub use patch::HunkRange;
pub use patch::Line;
pub use patch::ParsePatchError;
pub use patch::Patch;
pub use patch::PatchFormatter;
