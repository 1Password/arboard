#[cfg(feature = "image-data")]
use std::{cell::RefCell, rc::Rc};

#[cfg(feature = "wayland-data-control")]
use crate::wayland_data_control_clipboard::WaylandDataControlClipboardContext;
#[cfg(feature = "wayland-data-control")]
use log::{info, warn};

#[cfg(feature = "image-data")]
use crate::ImageData;
use crate::{x11_clipboard::X11ClipboardContext, Error};

pub fn into_unknown<E: std::fmt::Display>(error: E) -> Error {
	Error::Unknown { description: format!("{}", error) }
}

#[cfg(feature = "image-data")]
pub fn encode_as_png(image: &ImageData) -> Result<Vec<u8>, Error> {
	/// This is a workaround for the PNGEncoder not having a `into_inner` like function
	/// which would allow us to take back our Vec after the encoder finished encoding.
	/// So instead we create this wrapper around an Rc Vec which implements `io::Write`
	#[derive(Clone)]
	struct RcBuffer {
		inner: Rc<RefCell<Vec<u8>>>,
	}
	impl std::io::Write for RcBuffer {
		fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
			self.inner.borrow_mut().extend_from_slice(buf);
			Ok(buf.len())
		}
		fn flush(&mut self) -> std::io::Result<()> {
			// Noop
			Ok(())
		}
	}

	if image.bytes.is_empty() || image.width == 0 || image.height == 0 {
		return Err(Error::ConversionFailure);
	}

	let enc_output = RcBuffer { inner: Rc::new(RefCell::new(Vec::new())) };
	let encoder = image::png::PngEncoder::new(enc_output.clone());
	encoder
		.encode(
			image.bytes.as_ref(),
			image.width as u32,
			image.height as u32,
			image::ColorType::Rgba8,
		)
		.map_err(|_| Error::ConversionFailure)?;

	// The encoder must be destroyed by the time we get to `try_unwrap`, in order to
	// be able to take the value from the `Rc`. This code is currently relying on the fact
	// that the `encode` function consumes its `self` parameter.
	let bytes = Rc::try_unwrap(enc_output.inner).unwrap().into_inner();
	Ok(bytes)
}

/// Clipboard selection
#[derive(Copy, Clone, Debug)]
pub enum LinuxClipboardKind {
	/// Typically used selection for explicit cut/copy/paste actions (ie. windows/macos like
	/// clipboard behavior)
	Clipboard,

	/// Typically used for mouse selections and/or currently selected text. Accessible via middle
	/// mouse click.
	///
	/// *On Wayland, this may not be available for all systems (requires a compositor supporting
	/// version 2 or above) and operations using this will return an error if unsupported.*
	Primary,

	/// The secondary clipboard is rarely used but theoretically available on X11.
	///
	/// *On Wayland, this is not be available and operations using this variant will return an
	/// error.*
	Secondary,
}

/// Linux has a concept of clipboard "selections" which tend to be used in different contexts. This
/// trait extension provides a way to get/set to a specific clipboard (the default
/// [LinuxClipboardKind::Clipboard] being used for the common platform API).
///
/// See https://specifications.freedesktop.org/clipboards-spec/clipboards-0.1.txt for a better
/// description of the different clipboards.
///
/// # Examples
///
/// ```
/// use arboard::{Clipboard, ClipboardExtLinux, LinuxClipboardKind};
/// let mut ctx = Clipboard::new().unwrap();
///
/// ctx.set_text_with_clipboard(
///     "This goes in the traditional (ex. Copy & Paste) clipboard.".to_string(),
///     LinuxClipboardKind::Clipboard
/// ).unwrap();
///
/// ctx.set_text_with_clipboard(
///     "This goes in the primary keyboard. It's typically used via middle mouse click.".to_string(),
///     LinuxClipboardKind::Primary
/// ).unwrap();
/// ```
pub trait ClipboardExtLinux {
	/// Places the text onto the selected clipboard. Any valid utf-8 string is accepted. If wayland
	/// support is enabled and available, attempting to use the Secondary clipboard will return an
	/// error.
	fn set_text_with_clipboard(
		&mut self,
		text: String,
		clipboard: LinuxClipboardKind,
	) -> Result<(), Error>;

	/// Fetches utf-8 text from the selected clipboard and returns it. If wayland support is enabled
	/// and available, attempting to use the Secondary clipboard will return an error.
	fn get_text_with_clipboard(&mut self, clipboard: LinuxClipboardKind) -> Result<String, Error>;
}

impl ClipboardExtLinux for super::Clipboard {
	fn get_text_with_clipboard(&mut self, selection: LinuxClipboardKind) -> Result<String, Error> {
		match &mut self.platform {
			LinuxClipboard::X11(cb) => cb.get_text_with_clipboard(selection),

			#[cfg(feature = "wayland-data-control")]
			LinuxClipboard::WlDataControl(cb) => cb.get_text_with_clipboard(selection),
		}
	}

	fn set_text_with_clipboard(
		&mut self,
		text: String,
		selection: LinuxClipboardKind,
	) -> Result<(), Error> {
		match &mut self.platform {
			LinuxClipboard::X11(cb) => cb.set_text_with_clipboard(text, selection),

			#[cfg(feature = "wayland-data-control")]
			LinuxClipboard::WlDataControl(cb) => cb.set_text_with_clipboard(text, selection),
		}
	}
}

pub enum LinuxClipboard {
	X11(X11ClipboardContext),

	#[cfg(feature = "wayland-data-control")]
	WlDataControl(WaylandDataControlClipboardContext),
}

impl LinuxClipboard {
	pub fn new() -> Result<Self, Error> {
		#[cfg(feature = "wayland-data-control")]
		{
			if std::env::var_os("WAYLAND_DISPLAY").is_some() {
				// Wayland is available
				match WaylandDataControlClipboardContext::new() {
					Ok(clipboard) => {
						info!("Successfully initialized the Wayland data control clipboard.");
						return Ok( Self::WlDataControl(clipboard))
					},
					Err(e) => warn!(
						"Tried to initialize the wayland data control protocol clipboard, but failed. Falling back to the X11 clipboard protocol. The error was: {}",
						e
					),
				}
			}
		}
		Ok(Self::X11(X11ClipboardContext::new()?))
	}

	/// Fetches utf-8 text from the clipboard and returns it.
	pub fn get_text(&mut self) -> Result<String, Error> {
		match self {
			Self::X11(cb) => cb.get_text(),

			#[cfg(feature = "wayland-data-control")]
			Self::WlDataControl(cb) => cb.get_text(),
		}
	}

	/// Places the text onto the clipboard. Any valid utf-8 string is accepted.
	pub fn set_text(&mut self, text: String) -> Result<(), Error> {
		match self {
			Self::X11(cb) => cb.set_text(text),

			#[cfg(feature = "wayland-data-control")]
			Self::WlDataControl(cb) => cb.set_text(text),
		}
	}

	/// Fetches image data from the clipboard, and returns the decoded pixels.
	///
	/// Any image data placed on the clipboard with `set_image` will be possible read back, using
	/// this function. However it's of not guaranteed that an image placed on the clipboard by any
	/// other application will be of a supported format.
	#[cfg(feature = "image-data")]
	pub fn get_image(&mut self) -> Result<ImageData<'static>, Error> {
		match self {
			Self::X11(cb) => cb.get_image(),

			#[cfg(feature = "wayland-data-control")]
			Self::WlDataControl(cb) => cb.get_image(),
		}
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
		match self {
			Self::X11(cb) => cb.set_image(image),

			#[cfg(feature = "wayland-data-control")]
			Self::WlDataControl(cb) => cb.set_image(image),
		}
	}
}
