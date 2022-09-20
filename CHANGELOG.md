# Changelog

## 3.0 on 2022-19-09

### Added
- Support for clearing the clipboard.
- Spport for excluding Windows clipboard data from cliboard history and OneDrive.
- Support waiting for another process to read clipboard data before returning from
a `write` call to a X11 and Wayland or clipboard

### Changed
- Updated `wl-clipboard-rs` to the version `0.6`.
- Updated `x11rb` to the version `0.10`.
- Cleaned up spelling in documentation
- (Breaking) Functions that used to accept `String` now take `Into<Cow<'a>, str>` instead. 
This avoids cloning the string more times then necessary on platforms that can.
- (Breaking) `Error` is now marked as `#[non_exhaustive]`.
- (Breaking) Removed all platform specific modules and clipboard structures from the public API.
If you were using these directly, the recommended replacement is using `arboard::Clipboard` and 
the new platform-specific extension traits instead.

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
