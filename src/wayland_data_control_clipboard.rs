use super::common::{Error, ImageData};

use std::{
	cell::RefCell,
	io::{Cursor, Read},
	rc::Rc,
};

use wl_clipboard_rs::copy::{Options, Source};
use wl_clipboard_rs::paste::{get_contents, ClipboardType, Error as PasteError, Seat};

pub struct WaylandDataControlClipboardContext {}

impl WaylandDataControlClipboardContext {
	#[allow(clippy::unnecessary_wraps)]
	pub(crate) fn new() -> Result<Self, Error> {
		Ok(Self {})
	}

	pub fn get_text(&mut self) -> Result<String, Error> {
		use wl_clipboard_rs::paste::MimeType;
		let result = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text);
		match result {
			Ok((mut pipe, _)) => {
				let mut contents = vec![];
				pipe.read_to_end(&mut contents)
					.map_err(|e| Error::Unknown { description: format!("{}", e) })?;
				String::from_utf8(contents).map_err(|_| Error::ConversionFailure)
			}

			Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => {
				Err(Error::ContentNotAvailable)
			}

			Err(err) => return Err(Error::Unknown { description: format!("{}", err) }),
		}
	}

	pub fn set_text(&mut self, text: String) -> Result<(), Error> {
		use wl_clipboard_rs::copy::MimeType;
		let opts = Options::new();
		let source = Source::Bytes(text.as_bytes().into());
		opts.copy(source, MimeType::Autodetect).map_err(map_copy_error)?;
		Ok(())
	}

	pub fn get_image(&mut self) -> Result<ImageData, Error> {
		use wl_clipboard_rs::paste::MimeType;
		let result = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Any);
		match result {
			//TODO: Use mime_type to select Reader format
			Ok((mut pipe, _mime_type)) => {
				let mut buffer = vec![];
				pipe.read_to_end(&mut buffer)
					.map_err(|e| Error::Unknown { description: format!("{}", e) })?;
				dbg!(&buffer);
				let image = image::io::Reader::new(Cursor::new(buffer))
					.with_guessed_format()
					.map_err(|_| Error::ConversionFailure)?
					.decode()
					.map_err(|e| {
						dbg!(e);
						Error::ConversionFailure
					})?;
				let image = image.into_rgba8();

				Ok(ImageData {
					width: image.width() as usize,
					height: image.height() as usize,
					bytes: image.into_raw().into(),
				})
			}

			Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => {
				Err(Error::ContentNotAvailable)
			}

			Err(err) => return Err(Error::Unknown { description: format!("{}", err) }),
		}
	}

	pub fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		use wl_clipboard_rs::copy::MimeType;

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

		let buffer = RcBuffer { inner: Rc::new(RefCell::new(Vec::new())) };
		let encoding_result;
		{
			let encoder = image::png::PngEncoder::new(buffer.clone());
			encoding_result = encoder.encode(
				image.bytes.as_ref(),
				image.width as u32,
				image.height as u32,
				image::ColorType::Rgba8,
			);
		}
		// Rust impl: The encoder must be destroyed so that it lets go of its reference to the
		// `output` before we `try_unwrap()`
		if encoding_result.is_ok() {
			let opts = Options::new();
			let source = Source::Bytes(Rc::try_unwrap(buffer.inner).unwrap().into_inner().into());
			opts.copy(source, MimeType::Autodetect).map_err(map_copy_error)?;
		}

		Ok(())
	}
}

fn map_copy_error(error: wl_clipboard_rs::copy::Error) -> Error {
	Error::Unknown { description: format!("{}", error) }
}
