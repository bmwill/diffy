//! Tools for finding and manipulating differences between files
//!
//! ## Unified Format
//! `Patch`es can be outputed in the [Unified
//! Format](https://en.wikipedia.org/wiki/Diff#Unified_format) either by using its `Display` impl
//! or by using a `PatchFormatter` to output the diff with color.

mod apply;
mod diff;
mod patch;
mod range;

pub use diff::{create_patch, DiffOptions};
pub use patch::{Hunk, HunkRange, Line, Patch, PatchFormatter};
