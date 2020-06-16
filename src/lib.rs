//! Tools for finding and manipulating differences between files

mod apply;
mod diff;
mod patch;
mod range;

pub use diff::{create_patch, DiffOptions};
pub use patch::{Hunk, HunkRange, Line, Patch, PatchFormatter};
