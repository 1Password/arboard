
## v1.2.1 on 2021-05-04

### Changed
- Fixed a bug that caused the `set_image` function on Windows to distort the image colors.

## v1.2.0 on 2021-04-06

### Added

- Optional native wayland support through the `wl-clipboard-rs` crate.

## v1.1.0 on 2020-12-29

### Changed
- The `set_image` function on Windows now also provides the image in `CF_BITMAP` format.

## v1.0.2 on 2020-10-29

### Changed
- Fixed the clipboard contents sometimes not being preserved after the program exited.
