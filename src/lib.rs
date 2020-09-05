/*
Copyright 2016 Avraham Weinstock

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
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

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;
#[cfg(target_os = "macos")]
extern crate core_graphics;
#[cfg(target_os = "macos")]
extern crate objc_foundation;
#[cfg(target_os = "macos")]
extern crate objc_id;

mod common;
pub use common::ImageData;

use std::error::Error;

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
	platform: PlatformClipboard
}

impl Clipboard {
	pub fn new() -> Result<Self, Box<dyn Error>> {
		Ok(Clipboard {
			platform: PlatformClipboard::new()?
		})
	}

	pub fn get_text(&mut self) -> Result<String, Box<dyn Error>> {
		self.platform.get_text()
	}

	pub fn set_text(&mut self, text: String) -> Result<(), Box<dyn Error>> {
		self.platform.set_text(text)
	}

	pub fn get_image(&mut self) -> Result<ImageData, Box<dyn Error>> {
		self.platform.get_image()
	}

	pub fn set_image(&mut self, image: ImageData) -> Result<(), Box<dyn Error>> {
		self.platform.set_image(image)
	}
}

/// All tests grouped in one because the windows clipboard cannot be open on
/// multiple threads at once.
/// TODO this could be resolved by using a global mutex similar to the one the
/// Linux implementation uses.
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
