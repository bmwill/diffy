# Changelog

## [0.5.0] - Unreleased

This is a major release introducing multi-file patch support,
git binary diff handling, and `no_std` compatibility.

### Breaking Changes

- [#73](https://github.com/bmwill/diffy/pull/73)
  The crate is now `no_std` by default.
  `Patch::to_bytes()` and `PatchFormatter::write_patch_into()` now
  require the `std` feature. Add `features = ["std"]` to restore them.
- [#46](https://github.com/bmwill/diffy/pull/46)
  Color support is now behind the `color` feature flag.
  Previously color was always available. Now `PatchFormatter::with_color()`
  requires enabling the `color` feature. The underlying implementation
  switched from `nu-ansi-term` to `anstyle`.

### Added

- [#55](https://github.com/bmwill/diffy/pull/55)
  [#59](https://github.com/bmwill/diffy/pull/59)
  [#61](https://github.com/bmwill/diffy/pull/61)
  [#66](https://github.com/bmwill/diffy/pull/66)
  [#74](https://github.com/bmwill/diffy/pull/74)
  [#76](https://github.com/bmwill/diffy/pull/76)
  Multi-file patch support.
  Parse and apply unified diff and `git diff` output containing
  multiple files, including create, delete, modify, rename, and copy
  operations. Git binary patches (`literal` and `delta`) are supported
  behind the `binary` feature flag.
- [#80](https://github.com/bmwill/diffy/pull/80)
  New `apply` example demonstrating multi-file patch application.

### Fixed

- [#51](https://github.com/bmwill/diffy/pull/51)
  [#82](https://github.com/bmwill/diffy/pull/82)
  `Patch::from_str` / `from_bytes` no longer error on trailing
  non-patch content after a complete hunk,
  matching GNU patch and `git apply` behavior.
- [#65](https://github.com/bmwill/diffy/pull/65)
  Return an error instead of panicking on non-UTF-8 escaped filenames
  when parsing as `str`.
- [#47](https://github.com/bmwill/diffy/pull/47)
  Fix quoted filename escaping: handle `\a`, `\b`, `\f`, `\v`,
  3-digit octal escapes (`\0xx`–`\3xx`), and quote all control characters.
- [#83](https://github.com/bmwill/diffy/pull/83)
  Fix arithmetic overflow panic when parsing hunk headers
  with extremely large range values.

### Changed

- [#79](https://github.com/bmwill/diffy/pull/79)
  Bump MSRV to 1.85 (Rust 2024 edition).
- [#48](https://github.com/bmwill/diffy/pull/48)
  [#50](https://github.com/bmwill/diffy/pull/50)
  Parse error messages now show the byte offset where parsing failed.

## [0.4.2] - 2025-01-29

### Added
- [#37](https://github.com/bmwill/diffy/pull/37) Allow configuring the "No
  newline at end of file" message from being printed when formatting a patch.
- [#38](https://github.com/bmwill/diffy/pull/38) Add support for configuring
  `suppress_blank_empty`.

## [0.4.1] - 2025-01-29

### Added
- [#36](https://github.com/bmwill/diffy/pull/36) Add ability to configure
  filenames when creating a patch with `DiffOptions`.

## [0.4.0] - 2024-06-14

### Fixed
- [#28](https://github.com/bmwill/diffy/issues/28) Fixed an issue where
  conflicts were being omitted from merges.

### Added
- [#26](https://github.com/bmwill/diffy/pull/26) Add ability to reverse a
  patch.

### Changed
- [#29](https://github.com/bmwill/diffy/pull/29) Bump minimum supported rust
  version (msrv) to 1.62.1.
- [#22](https://github.com/bmwill/diffy/pull/22) update nu-ansi-term dependency
  to 0.50.

## [0.3.0] - 2022-08-29

### Fixed
- [#17](https://github.com/bmwill/diffy/issues/17) Fix an issue which resulted
  in a large slowdown when applying a patch with incorrect hunk headers.
- [#18](https://github.com/bmwill/diffy/pull/18) Replace unmaintained ansi_term
  dependency with nu_ansi_term in order to address
  [RUSTSEC-2021-0139](https://rustsec.org/advisories/RUSTSEC-2021-0139).

### Changed
- [#19](https://github.com/bmwill/diffy/pull/19) Bump minimum supported rust
  version (msrv) to 1.51.0.

## [0.2.2] - 2022-01-31

### Fixed
- [#16](https://github.com/bmwill/diffy/issues/16) Fix an issue where patch
  files failed to parse when they contained hunks which were adjacent to one
  another.

## [0.2.1] - 2021-01-27

### Fixed
- [#9](https://github.com/bmwill/diffy/issues/9) Fix an issue where the incorrect
  range was being used to index an array when calculating a merge resulting in a
  panic in some cases.

## [0.2.0] - 2020-07-07
### Added
- Support for working with potentially non-utf8 data with the addition of
  various `*_bytes` functions.
- Support for writing both utf8 and non-utf8 patches into a writer `W: io::write`
- Support for a minimum supported rust version (msrv) of 1.36.0.

### Changed
- The `Patch` type is now generic across the text type, either `str` for utf8
  text and `[u8]` for potentially non-utf8 texts.
- The filenames for the original and modified files of a patch are now
  optional. This means that patches which don't include filename headers
  (only include hunks) can now properly be parsed.

### Fixed
- Quoted filenames which include escaped characters are now properly parsed.

## [0.1.1] - 2020-07-01
### Added
- `Patch`es can now be parsed from strings with `Patch::from_str`
- A `Patch` can now be applied to a base image with `apply`

## [0.1.0] - 2020-06-30
- Initial release.

[0.5.0]: https://github.com/bmwill/diffy/releases/tag/0.5.0
[0.4.2]: https://github.com/bmwill/diffy/releases/tag/0.4.2
[0.4.1]: https://github.com/bmwill/diffy/releases/tag/0.4.1
[0.4.0]: https://github.com/bmwill/diffy/releases/tag/0.4.0
[0.3.0]: https://github.com/bmwill/diffy/releases/tag/0.3.0
[0.2.2]: https://github.com/bmwill/diffy/releases/tag/0.2.2
[0.2.1]: https://github.com/bmwill/diffy/releases/tag/0.2.1
[0.2.0]: https://github.com/bmwill/diffy/releases/tag/0.2.0
[0.1.1]: https://github.com/bmwill/diffy/releases/tag/0.1.1
[0.1.0]: https://github.com/bmwill/diffy/releases/tag/0.1.0
