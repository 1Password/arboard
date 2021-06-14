use std::{
	convert::TryInto,
	io::{Cursor, Read},
};

use wl_clipboard_rs::{
	copy::{self, Options, Source},
	paste::{self, get_contents, Error as PasteError, Seat},
	utils::is_primary_selection_supported,
};

use crate::{
	common::{Error, ImageData},
	common_linux::{encode_as_png, into_unknown, LinuxClipboardKind},
};

static MIME_PNG: &str = "image/png";

pub struct WaylandDataControlClipboardContext {}

impl TryInto<copy::ClipboardType> for LinuxClipboardKind {
	type Error = Error;

	fn try_into(self) -> Result<copy::ClipboardType, Self::Error> {
		match self {
			LinuxClipboardKind::Clipboard => Ok(copy::ClipboardType::Regular),
			LinuxClipboardKind::Primary => Ok(copy::ClipboardType::Primary),
			LinuxClipboardKind::Secondary => {
				return Err(Error::ContentNotAvailable);
			}
		}
	}
}

impl TryInto<paste::ClipboardType> for LinuxClipboardKind {
	type Error = Error;

	fn try_into(self) -> Result<paste::ClipboardType, Self::Error> {
		match self {
			LinuxClipboardKind::Clipboard => Ok(paste::ClipboardType::Regular),
			LinuxClipboardKind::Primary => Ok(paste::ClipboardType::Primary),
			LinuxClipboardKind::Secondary => {
				return Err(Error::ContentNotAvailable);
			}
		}
	}
}

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
		self.get_text_with_clipboard(LinuxClipboardKind::Clipboard)
	}

	pub(crate) fn get_text_with_clipboard(
		&mut self,
		selection: LinuxClipboardKind,
	) -> Result<String, Error> {
		use wl_clipboard_rs::paste::MimeType;

		let result = get_contents(selection.try_into()?, Seat::Unspecified, MimeType::Text);
		match result {
			Ok((mut pipe, _)) => {
				let mut contents = vec![];
				pipe.read_to_end(&mut contents).map_err(into_unknown)?;
				String::from_utf8(contents).map_err(|_| Error::ConversionFailure)
			}

			Err(PasteError::ClipboardEmpty)
			| Err(PasteError::NoMimeType)
			| Err(PasteError::PrimarySelectionUnsupported) => Err(Error::ContentNotAvailable),

			Err(err) => return Err(Error::Unknown { description: format!("{}", err) }),
		}
	}

	pub fn set_text(&mut self, text: String) -> Result<(), Error> {
		self.set_text_with_clipboard(text, LinuxClipboardKind::Clipboard)
	}

	pub(crate) fn set_text_with_clipboard(
		&self,
		text: String,
		selection: LinuxClipboardKind,
	) -> Result<(), Error> {
		use wl_clipboard_rs::copy::MimeType;
		let mut opts = Options::new();
		opts.clipboard(selection.try_into()?);
		let source = Source::Bytes(text.as_bytes().into());
		opts.copy(source, MimeType::Text).map_err(into_unknown)?;
		Ok(())
	}

	pub fn get_image(&mut self) -> Result<ImageData, Error> {
		use paste::ClipboardType;
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

	pub fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		use wl_clipboard_rs::copy::MimeType;

		let image = encode_as_png(&image)?;
		let opts = Options::new();
		let source = Source::Bytes(image.into());
		opts.copy(source, MimeType::Specific(MIME_PNG.into())).map_err(into_unknown)?;
		Ok(())
	}
}
