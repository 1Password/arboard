/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use std::{borrow::Cow, cell::RefCell, rc::Rc};
use thiserror::Error;

/// An error that might happen during a clipboard operation.
///
/// Note that both the `Display` and the `Debug` trait is implemented for this type in such a way
/// that they give a short human-readable description of the error; however the documentation
/// gives a more detailed explanation for each error kind.
#[derive(Error)]
pub enum Error {
	/// The clipboard contents were not available in the requested format.
	/// This could either be due to the clipboard being empty or the clipboard contents having
	/// an incompatible format to the requested one (eg when calling `get_image` on text)
	#[error("The clipboard contents were not available in the requested format or the clipboard is empty.")]
	ContentNotAvailable,

	/// The native clipboard is not accessible due to being held by an other party.
	///
	/// This "other party" could be a different process or it could be within
	/// the same program. So for example you may get this error when trying
	/// to interact with the clipboard from multiple threads at once.
	///
	/// Note that it's OK to have multiple `Clipboard` instances. The underlying
	/// implementation will make sure that the native clipboard is only
	/// opened for transferring data and then closed as soon as possible.
	#[error("The native clipboard is not accessible due to being held by an other party.")]
	ClipboardOccupied,

	/// This can happen in either of the following cases.
	///
	/// - When returned from `set_image`: the image going to the clipboard cannot be converted to the appropriate format.
	/// - When returned from `get_image`: the image coming from the clipboard could not be converted into the `ImageData` struct.
	/// - When returned from `get_text`: the text coming from the clipboard is not valid utf-8 or cannot be converted to utf-8.
	#[error("The image or the text that was about the be transferred to/from the clipboard could not be converted to the appropriate format.")]
	ConversionFailure,

	/// Any error that doesn't fit the other error types.
	///
	/// The `description` field is only meant to help the developer and should not be relied on as a
	/// means to identify an error case during runtime.
	#[error("Unknown error while interacting with the clipboard: {description}")]
	Unknown { description: String },
}

impl std::fmt::Debug for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		use Error::*;
		macro_rules! kind_to_str {
			($( $e: pat ),*) => {
				match self {
					$(
						$e => stringify!($e),
					)*
				}
			}
		}
		let name =
			kind_to_str!(ContentNotAvailable, ClipboardOccupied, ConversionFailure, Unknown { .. });
		f.write_fmt(format_args!("{} - \"{}\"", name, self))
	}
}

/// Stores pixel data of an image.
///
/// Each element in `bytes` stores the value of a channel of a single pixel.
/// This struct stores four channels (red, green, blue, alpha) so
/// a `3*3` image is going to be stored on `3*3*4 = 36` bytes of data.
///
/// The pixels are in row-major order meaning that the second pixel
/// in `bytes` (starting at the fifth byte) corresponds to the pixel that's
/// sitting to the right side of the top-left pixel (x=1, y=0)
///
/// Assigning a `2*1` image would for example look like this
/// ```
/// use arboard::ImageData;
/// use std::borrow::Cow;
/// let bytes = [
///     // A red pixel
///     255, 0, 0, 255,
///
///     // A green pixel
///     0, 255, 0, 255,
/// ];
/// let img = ImageData {
///     width: 2,
///     height: 1,
///     bytes: Cow::from(bytes.as_ref())
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ImageData<'a> {
	pub width: usize,
	pub height: usize,
	pub bytes: Cow<'a, [u8]>,
}

impl<'a> ImageData<'a> {
	/// Returns a the bytes field in a way that it's guaranteed to be owned.
	/// It moves the bytes if they are already owned and clones them if they are borrowed.
	pub fn into_owned_bytes(self) -> std::borrow::Cow<'static, [u8]> {
		self.bytes.into_owned().into()
	}

	/// Returns an image data that is guaranteed to own its bytes.
	/// It moves the bytes if they are already owned and clones them if they are borrowed.
	pub fn to_owned_img(&self) -> ImageData<'static> {
		ImageData {
			width: self.width,
			height: self.height,
			bytes: self.bytes.clone().into_owned().into(),
		}
	}
}

pub(crate) fn convert_to_png<'a>(image: ImageData<'a>) -> Result<ImageData<'a>, Error> {
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

	let mut image = image.to_owned_img();
	let buffer = RcBuffer { inner: Rc::new(RefCell::new(Vec::new())) };
	let encoding_result;
	let encoder = image::png::PngEncoder::new(buffer.clone());
	encoding_result = encoder.encode(
		image.bytes.as_ref(),
		image.width as u32,
		image.height as u32,
		image::ColorType::Rgba8,
	);

	if encoding_result.is_ok() {
		image.bytes = Rc::try_unwrap(buffer.inner).unwrap().into_inner().into();
		Ok(image)
	} else {
		Err(Error::ConversionFailure)
	}
}
