use super::common::{convert_to_png, Error, ImageData};

use std::io::{Cursor, Read};

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
		opts.copy(source, MimeType::Text).map_err(into_unknown)?;
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

		convert_to_png(image).and_then(|image| {
			let opts = Options::new();
			let source = Source::Bytes(image.bytes.into());
			opts.copy(source, MimeType::Specific("image/png".into())).map_err(into_unknown)?;
			Ok(())
		})
	}
}

fn into_unknown<E: std::fmt::Display>(error: E) -> Error {
	Error::Unknown { description: format!("{}", error) }
}
