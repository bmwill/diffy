//! Compatibility tests against reference implementations.
//!
//! These tests verify diffy produces results compatible with established tools
//! Focus is on edge cases and ambiguous behavior,
//! not basic functionality which is covered by unit tests in `src/patches/tests.rs`.
//!
//! ## Test structure
//!
//! Each test case has:
//!
//! - `in/` directory with original file(s) and `foo.patch`
//! - `out/` directory with expected patched file(s) (for success cases)
//!
//! For failure test cases:
//!
//! - Only `in/` directory is needed (no `out/`)
//! - Both diffy and reference tool should fail to apply
//!
//! ## Running tests
//!
//! ```sh
//! # Run all compat tests
//! cargo test --test compat -F binary
//!
//! # Run with reference tool comparison (CI mode)
//! CI=1 cargo test --test compat -F binary
//!
//! # For Nix users, run this to ensure you have GNU patch
//! CI=1 nix shell nixpkgs#gnupatch -c cargo test --test compat -F binary
//! ```
//!
//! ## Regenerating snapshots
//!
//! ```sh
//! SNAPSHOTS=overwrite cargo test --test compat -F binary
//! ```
//!
//! ## Adding new test cases
//!
//! 1. Create `case_name/in/` with input file(s) and `foo.patch`
//! 2. Run `SNAPSHOTS=overwrite cargo test --test compat -F binary` to generate `out/`
//! 3. Add `#[test] fn case_name() { Case::{gnu_patch,git}(...).run(); }` in the module
//!
//! For failure tests, use `.expect_success(false)` and skip step 2.
//! For intentional compat divergence, use `.expect_compat(false)`.

mod common;
mod git;
mod gnu_patch;
