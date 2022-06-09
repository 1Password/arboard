/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2022 The Arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

#![crate_name = "arboard"]
#![crate_type = "lib"]

mod common;
pub use common::Error;
#[cfg(feature = "image-data")]
pub use common::ImageData;

mod platform;

#[cfg(all(
	unix,
	not(any(target_os = "macos", target_os = "android", target_os = "emscripten")),
))]
pub use platform::{GetExtLinux, LinuxClipboardKind, SetExtLinux};

/// The OS independent struct for accessing the clipboard.
///
/// Any number of `Clipboard` instances are allowed to exist at a single point in time. Note however
/// that all `Clipboard`s must be 'dropped' before the program exits. In most scenarios this happens
/// automatically but there are frameworks (for example `winit`) that take over the execution
/// and where the objects don't get dropped when the application exits. In these cases you have to
/// make sure the object is dropped by taking ownership of it in a confined scope when detecting
/// that your application is about to quit.
///
/// It is also valid to have multiple `Clipboards` on separate threads at once but note that
/// executing multiple clipboard operations in parallel might fail with a `ClipboardOccupied` error.
pub struct Clipboard {
	pub(crate) platform: platform::Clipboard,
}

impl Clipboard {
	/// Creates an instance of the clipboard
	pub fn new() -> Result<Self, Error> {
		Ok(Clipboard { platform: platform::Clipboard::new()? })
	}

	/// Fetches utf-8 text from the clipboard and returns it.
	pub fn get_text(&mut self) -> Result<String, Error> {
		self.get().text()
	}

	/// Places the text onto the clipboard. Any valid utf-8 string is accepted.
	pub fn set_text(&mut self, text: String) -> Result<(), Error> {
		self.set().text(text)
	}

	/// Fetches image data from the clipboard, and returns the decoded pixels.
	///
	/// Any image data placed on the clipboard with `set_image` will be possible read back, using
	/// this function. However it's of not guaranteed that an image placed on the clipboard by any
	/// other application will be of a supported format.
	#[cfg(feature = "image-data")]
	pub fn get_image(&mut self) -> Result<ImageData<'static>, Error> {
		self.get().image()
	}

	/// Places an image to the clipboard.
	///
	/// The chosen output format, depending on the platform is the following:
	///
	/// - On macOS: `NSImage` object
	/// - On Linux: PNG, under the atom `image/png`
	/// - On Windows: In order of priority `CF_DIB` and `CF_BITMAP`
	#[cfg(feature = "image-data")]
	pub fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		self.set().image(image)
	}

	/// Begins a "get" operation to retrieve data from the clipboard.
	pub fn get(&mut self) -> Get<'_> {
		Get { platform: platform::Get::new(&mut self.platform) }
	}

	/// Begins a "set" operation to set the clipboard's contents.
	pub fn set(&mut self) -> Set<'_> {
		Set { platform: platform::Set::new(&mut self.platform) }
	}
}

/// A builder for an operation that gets a value from the clipboard.
#[must_use]
pub struct Get<'clipboard> {
	pub(crate) platform: platform::Get<'clipboard>,
}

impl Get<'_> {
	/// Completes the "get" operation by fetching UTF-8 text from the clipboard.
	pub fn text(self) -> Result<String, Error> {
		self.platform.text()
	}

	/// Completes the "get" operation by fetching image data from the clipboard and returning the
	/// decoded pixels.
	///
	/// Any image data placed on the clipboard with `set_image` will be possible read back, using
	/// this function. However it's of not guaranteed that an image placed on the clipboard by any
	/// other application will be of a supported format.
	#[cfg(feature = "image-data")]
	pub fn image(self) -> Result<ImageData<'static>, Error> {
		self.platform.image()
	}
}

/// A builder for an operation that sets a value to the clipboard.
#[must_use]
pub struct Set<'clipboard> {
	pub(crate) platform: platform::Set<'clipboard>,
}

impl Set<'_> {
	/// Completes the "set" operation by placing text onto the clipboard. Any valid UTF-8 string
	/// is accepted.
	pub fn text(self, text: String) -> Result<(), Error> {
		self.platform.text(text)
	}

	/// Completes the "set" operation by placing an image onto the clipboard.
	///
	/// The chosen output format, depending on the platform is the following:
	///
	/// - On macOS: `NSImage` object
	/// - On Linux: PNG, under the atom `image/png`
	/// - On Windows: In order of priority `CF_DIB` and `CF_BITMAP`
	#[cfg(feature = "image-data")]
	pub fn image(self, image: ImageData) -> Result<(), Error> {
		self.platform.image(image)
	}
}

/// All tests grouped in one because the windows clipboard cannot be open on
/// multiple threads at once.
#[cfg(test)]
#[test]
fn all_tests() {
	use std::{thread, time::Duration};

	let _ = env_logger::builder().is_test(true).try_init();
	{
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
		thread::sleep(Duration::from_millis(300));

		let mut ctx = Clipboard::new().unwrap();
		assert_eq!(ctx.get_text().unwrap(), text);
	}
	{
		let mut ctx = Clipboard::new().unwrap();
		let text = "Some utf8: 🤓 ∑φ(n)<ε 🐔";
		ctx.set_text(text.to_owned()).unwrap();
		assert_eq!(ctx.get_text().unwrap(), text);
	}
	#[cfg(feature = "image-data")]
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

		// Make sure that setting one format overwrites the other.
		ctx.set_image(img_data.clone()).unwrap();
		assert!(matches!(ctx.get_text(), Err(Error::ContentNotAvailable)));

		ctx.set_text("clipboard test".into()).unwrap();
		assert!(matches!(ctx.get_image(), Err(Error::ContentNotAvailable)));

		// Test if we get the same image that we put onto the clibboard
		ctx.set_image(img_data.clone()).unwrap();
		let got = ctx.get_image().unwrap();
		assert_eq!(img_data.bytes, got.bytes);

		#[rustfmt::skip]
		let big_bytes = vec![
			255, 100, 100, 255,
			100, 255, 100, 100,
			100, 100, 255, 100,

			0, 1, 2, 255,
			0, 1, 2, 255,
			0, 1, 2, 255,
		];
		let bytes_cloned = big_bytes.clone();
		let big_img_data = ImageData { width: 3, height: 2, bytes: big_bytes.into() };
		ctx.set_image(big_img_data).unwrap();
		let got = ctx.get_image().unwrap();
		assert_eq!(bytes_cloned.as_slice(), got.bytes.as_ref());
	}
	#[cfg(all(
		unix,
		not(any(target_os = "macos", target_os = "android", target_os = "emscripten")),
	))]
	{
		use crate::{LinuxClipboardKind, SetExtLinux};
		use std::sync::{
			atomic::{self, AtomicBool},
			Arc,
		};

		let mut ctx = Clipboard::new().unwrap();

		const TEXT1: &str = "I'm a little teapot,";
		const TEXT2: &str = "short and stout,";
		const TEXT3: &str = "here is my handle";

		ctx.set().clipboard(LinuxClipboardKind::Clipboard).text(TEXT1.to_string()).unwrap();

		ctx.set().clipboard(LinuxClipboardKind::Primary).text(TEXT2.to_string()).unwrap();

		// The secondary clipboard is not available under wayland
		if !cfg!(feature = "wayland-data-control") || std::env::var_os("WAYLAND_DISPLAY").is_none()
		{
			ctx.set().clipboard(LinuxClipboardKind::Secondary).text(TEXT3.to_string()).unwrap();
		}

		assert_eq!(TEXT1, &ctx.get().clipboard(LinuxClipboardKind::Clipboard).text().unwrap());

		assert_eq!(TEXT2, &ctx.get().clipboard(LinuxClipboardKind::Primary).text().unwrap());

		// The secondary clipboard is not available under wayland
		if !cfg!(feature = "wayland-data-control") || std::env::var_os("WAYLAND_DISPLAY").is_none()
		{
			assert_eq!(TEXT3, &ctx.get().clipboard(LinuxClipboardKind::Secondary).text().unwrap());
		}

		let was_replaced = Arc::new(AtomicBool::new(false));

		let setter = thread::spawn({
			let was_replaced = was_replaced.clone();
			move || {
				thread::sleep(Duration::from_millis(100));
				let mut ctx = Clipboard::new().unwrap();
				ctx.set_text("replacement text".to_owned()).unwrap();
				was_replaced.store(true, atomic::Ordering::Release);
			}
		});

		ctx.set().wait().text("initial text".to_owned()).unwrap();

		assert!(was_replaced.load(atomic::Ordering::Acquire));

		setter.join().unwrap();
	}
}
