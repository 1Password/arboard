use std::io::Read;

use wl_clipboard_rs::{
	copy::{Options, Source},
	paste::{get_contents, ClipboardType, Error as PasteError, Seat},
	utils::is_primary_selection_supported,
};

use crate::{common::Error, common_linux::into_unknown};
#[cfg(feature = "image-data")]
use crate::{common::ImageData, common_linux::encode_as_png};

#[cfg(feature = "image-data")]
const MIME_PNG: &str = "image/png";

pub struct WaylandDataControlClipboardContext {}

impl WaylandDataControlClipboardContext {
	#[allow(clippy::unnecessary_wraps)]
	pub(crate) fn new() -> Result<Self, Error> {
		// Check if it's possible to communicate with the wayland compositor
		if let Err(e) = is_primary_selection_supported() {
			return Err(into_unknown(e));
		}
		Ok(Self {})
	}

	pub fn get_text(&mut self) -> Result<String, Error> {
		use wl_clipboard_rs::paste::MimeType;
		let result = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text);
		match result {
			Ok((mut pipe, _)) => {
				let mut contents = vec![];
				pipe.read_to_end(&mut contents).map_err(into_unknown)?;
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

	#[cfg(feature = "image-data")]
	pub fn get_image(&mut self) -> Result<ImageData, Error> {
		use std::io::Cursor;

		use wl_clipboard_rs::paste::MimeType;
		let result =
			get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Specific(MIME_PNG));
		match result {
			Ok((mut pipe, _mime_type)) => {
				let mut buffer = vec![];
				pipe.read_to_end(&mut buffer).map_err(into_unknown)?;
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

	#[cfg(feature = "image-data")]
	pub fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		use wl_clipboard_rs::copy::MimeType;

		let image = encode_as_png(&image)?;
		let opts = Options::new();
		let source = Source::Bytes(image.into());
		opts.copy(source, MimeType::Specific(MIME_PNG.into())).map_err(into_unknown)?;
		Ok(())
	}
}
