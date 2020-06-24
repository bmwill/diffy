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
//! color.
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
//!
//! // With color
//! # use diffy::PatchFormatter;
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
//! [LibXDiff]: http://www.xmailserver.org/xdiff-lib.html
//! [Myers' diff algorithm]: http://www.xmailserver.org/diff2.pdf
//! [GNU Diffutils]: https://www.gnu.org/software/diffutils/
//! [Git]: https://git-scm.com/
//! [Mercurial]: https://www.mercurial-scm.org/
//! [Unified Format]: https://en.wikipedia.org/wiki/Diff#Unified_format
//!
//! [`Display`]: https://doc.rust-lang.org/stable/std/fmt/trait.Display.html
//! [`Patch`]: struct.Patch.html
//! [`PatchFormatter`]: struct.PatchFormatter.html

mod apply;
mod diff;
mod merge;
mod patch;
mod range;
mod utils;

pub use diff::{create_patch, DiffOptions};
pub use patch::{Hunk, HunkRange, Line, Patch, PatchFormatter};
