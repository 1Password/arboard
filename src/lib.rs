/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

#![crate_name = "arboard"]
#![crate_type = "lib"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
extern crate image;
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
extern crate libc;
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
extern crate xcb;
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
#[macro_use]
extern crate lazy_static;

#[cfg(windows)]
extern crate byteorder;
#[cfg(windows)]
extern crate clipboard_win;
#[cfg(windows)]
extern crate image;

// #[cfg(target_os = "macos")]
// #[macro_use]
// extern crate objc;
// #[cfg(target_os = "macos")]
// extern crate core_graphics;
// #[cfg(target_os = "macos")]
// extern crate objc_foundation;
// #[cfg(target_os = "macos")]
// extern crate objc_id;

mod common;
pub use common::{Error, ImageData};

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
pub mod x11_clipboard;

#[cfg(windows)]
pub mod windows_clipboard;

#[cfg(target_os = "macos")]
pub mod osx_clipboard;

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
type PlatformClipboard = x11_clipboard::X11ClipboardContext;
#[cfg(windows)]
type PlatformClipboard = windows_clipboard::WindowsClipboardContext;
#[cfg(target_os = "macos")]
type PlatformClipboard = osx_clipboard::OSXClipboardContext;

pub struct Clipboard {
	platform: PlatformClipboard,
}

impl Clipboard {
	pub fn new() -> Result<Self, Error> {
		Ok(Clipboard { platform: PlatformClipboard::new()? })
	}

	pub fn get_text(&mut self) -> Result<String, Error> {
		self.platform.get_text()
	}

	pub fn set_text(&mut self, text: String) -> Result<(), Error> {
		self.platform.set_text(text)
	}

	pub fn get_image(&mut self) -> Result<ImageData, Error> {
		self.platform.get_image()
	}

	pub fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		self.platform.set_image(image)
	}
}

/// All tests grouped in one because the windows clipboard cannot be open on
/// multiple threads at once.
#[test]
fn all_tests() {
	{
		let mut ctx = Clipboard::new().unwrap();
		let text = "some string";
		ctx.set_text(text.to_owned()).unwrap();
		assert_eq!(ctx.get_text().unwrap(), text);
	}
	{
		let mut ctx = Clipboard::new().unwrap();
		let text = "Some utf8: 🤓 ∑φ(n)<ε 🐔";
		ctx.set_text(text.to_owned()).unwrap();
		assert_eq!(ctx.get_text().unwrap(), text);
	}
	{
		let mut ctx = Clipboard::new().unwrap();
		#[rustfmt::skip]
		let bytes = [
			255, 100, 100, 255,
			100, 255, 100, 100,
			100, 100, 255, 100,
			0, 0, 0, 255,
		];
		let img_data = ImageData { width: 2, height: 2, bytes: bytes.as_ref().into() };
		ctx.set_image(img_data.clone()).unwrap();
		let got = ctx.get_image().unwrap();
		assert_eq!(img_data.bytes, got.bytes);
	}
}
