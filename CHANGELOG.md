# Changelog

## v2.1.1 on 2022-18-05

### Changed

- Fix compilation on FreeBSD
- Internal cleanup and documentation fixes
- Remove direct dependency on the `once_cell` crate.
- Fixed crates.io repository link

## v2.1.0 on 2022-09-03

### Changed

- Updated most dependencies
- Removed crate deprecation
- Fixed soundness bug in Windows clipboard

## v2.0.1 on 2021-11-05

### Changed

- On X11, re-assert clipboard ownership every time the data changes.

## v2.0.0 on 2021-08-07

### Changed

- Update dependency on yanked crate versions
- Make the image operations an optional feature

### Added

- Support selecting which linux clipboard is used

## v1.2.1 on 2021-05-04

### Changed

- Fixed a bug that caused the `set_image` function on Windows to distort the
  image colors.

## v1.2.0 on 2021-04-06

### Added

- Optional native wayland support through the `wl-clipboard-rs` crate.

## v1.1.0 on 2020-12-29

### Changed

- The `set_image` function on Windows now also provides the image in
  `CF_BITMAP` format.

## v1.0.2 on 2020-10-29

### Changed

- Fixed the clipboard contents sometimes not being preserved after the program
  exited.
