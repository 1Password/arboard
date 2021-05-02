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

#[cfg(windows)]
extern crate clipboard_win;
#[cfg(windows)]
extern crate image;

mod common;
pub use common::{CustomItem, Error, ImageData};

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten")),))]
pub(crate) mod common_linux;

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten")),))]
pub mod x11_clipboard;

#[cfg(all(
	unix,
	not(any(target_os = "macos", target_os = "android", target_os = "emscripten")),
	feature = "wayland-data-control"
))]
pub mod wayland_data_control_clipboard;

#[cfg(windows)]
pub mod windows_clipboard;

#[cfg(target_os = "macos")]
mod osx_clipboard;

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten")),))]
type PlatformClipboard = common_linux::LinuxClipboard;
#[cfg(windows)]
type PlatformClipboard = windows_clipboard::WindowsClipboardContext;
#[cfg(target_os = "macos")]
type PlatformClipboard = osx_clipboard::OSXClipboardContext;

/// The OS independent struct for accessing the clipboard.
///
/// Any number of `Clipboard` instances are allowed to exist at a single point in time. Note however
/// that all `Clipboard`s must be 'dropped' before the program exits. In most scenarios this happens
/// automatically but there are frameworks (for example `winit`) that take over the execution
/// and where some objects don't get dropped when the application exits. In these cases you have to
/// make sure the object is dropped by moving it into a confined scope when detecting
/// that your application is about to quit.
///
/// It is also valid to have multiple `Clipboards` on separate threads at once but note that
/// executing multiple clipboard operations in paralell might fail with a `ClipboardOccupied` error.
pub struct Clipboard {
	platform: PlatformClipboard,
}

impl Clipboard {
	/// Creates an instance of the clipboard
	pub fn new() -> Result<Self, Error> {
		Ok(Clipboard { platform: PlatformClipboard::new()? })
	}

	/// Fetches utf-8 text from the clipboard and returns it.
	pub fn get_text(&mut self) -> Result<String, Error> {
		self.platform.get_text()
	}

	/// Places the text onto the clipboard. Any valid utf-8 string is accepted.
	pub fn set_text(&mut self, text: String) -> Result<(), Error> {
		self.platform.set_text(text)
	}

	/// Fetches image data from the clipboard, and returns the decoded pixels.
	///
	/// Any image data placed on the clipboard with `set_image` will be possible read back, using
	/// this function. However it's of not guaranteed that an image placed on the clipboard by any
	/// other application will be of a supported format.
	pub fn get_image(&mut self) -> Result<ImageData, Error> {
		self.platform.get_image()
	}

	/// Places an image to the clipboard.
	///
	/// The chosen output format, depending on the platform is the following:
	///
	/// - On macOS: `NSImage` object
	/// - On Linux: PNG, under the atom `image/png`
	/// - On Windows: In order of priority `CF_DIB` and `CF_BITMAP`
	pub fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		self.platform.set_image(image)
	}

	/// Places a list of representations of the same object onto the clipboard.
	///
	/// Each `CustomItem` is one representation of the object.
	pub fn set_custom(&mut self, items: &[CustomItem]) -> Result<(), Error> {
		self.platform.set_custom(items)
	}

	/// Gets all available representations of the object on the clipboard.
	pub fn get_all(&mut self) -> Result<Vec<CustomItem>, Error> {
		self.platform.get_all()
	}
}

/// All tests grouped in one because we want the tests to run on a single
/// thread.
///
/// Reason: The clipboard is a global resource and we don't even expect that our
/// opreations complete successfully if they all try to manipulate the clipboard
/// at once.
#[cfg(test)]
#[test]
fn all_tests() {
	let _ = env_logger::builder().is_test(true).try_init();
	{
		// Setting and getting text, and persisting after closing.

		let mut ctx = Clipboard::new().unwrap();
		let text = "some string";
		ctx.set_text(text.to_owned()).unwrap();
		assert_eq!(ctx.get_text().unwrap(), text);

		// We also need to check that the content persists after the drop; this is
		// especially important on X11
		drop(ctx);

		// Give any external mechanism a generous amount of time to take over
		// responsibility for the clipboard, in case that happens asynchronously
		// (it appears that this is the case on X11 plus Mutter 3.34+, see #4)
		use std::time::Duration;
		std::thread::sleep(Duration::from_millis(100));

		let mut ctx = Clipboard::new().unwrap();
		assert_eq!(ctx.get_text().unwrap(), text);
	}
	{
		// Setting and getting non-ascii text.

		let mut ctx = Clipboard::new().unwrap();
		let text = "Some utf8: 🤓 ∑φ(n)<ε 🐔";
		ctx.set_text(text.to_owned()).unwrap();
		assert_eq!(ctx.get_text().unwrap(), text);
	}
	{
		// Setting and getting an image.

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

		ctx.set_text("just overriding the clipboard".into()).unwrap();

		// Now set an image as a custom type
		ctx.set_custom(&[CustomItem::RawImage(img_data.clone())]).unwrap();
		let items = ctx.get_all().unwrap();
		match items.get(0).unwrap() {
			CustomItem::RawImage(got) => {
				assert_eq!(img_data.bytes, got.bytes);
			}
			_ => unreachable!(),
		}
	}
	{
		// Setting and getting uri list
		let uri_list_w_comment = "#comment\r\nfile:///C:/my file.txt\r\nfile:///C:/a folder/";
		let uri_list_no_comment = "file:///C:/my file.txt\r\nfile:///C:/a folder/";

		let mut ctx = Clipboard::new().unwrap();
		ctx.set_custom(&[CustomItem::TextUriList(uri_list_w_comment.into())]).unwrap();
		let items = ctx.get_all().unwrap();
		match items.get(0).unwrap() {
			CustomItem::TextUriList(l) => {
				assert_eq!(l.as_ref(), uri_list_no_comment);
			}
			_ => unreachable!(),
		}
	}
	{
		// Setting and getting HTML
		let html = "<p style='text-align:center'>Hello <b>World</b>!</p>\r\n<p>Another line</p>";

		let mut ctx = Clipboard::new().unwrap();
		ctx.set_custom(&[CustomItem::TextHtml(html.into())]).unwrap();
		let items = ctx.get_all().unwrap();
		match items.get(0).unwrap() {
			CustomItem::TextHtml(t) => {
				assert_eq!(t.as_ref(), html);
			}
			_ => unreachable!(),
		}
	}
}
