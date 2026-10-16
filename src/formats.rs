use std::{borrow::Cow, path::PathBuf};

use crate::Error;

/// An opaque native clipboard format.
///
/// Linux uses X11 target names or Wayland MIME types. macOS uses pasteboard type names.
/// Windows uses numeric identifiers, not MIME types. Identifiers are local to the OS session.
/// Equality compares native identities. Do not persist identifiers between sessions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ClipboardFormat {
	#[cfg(windows)]
	pub(crate) native: u32,
	#[cfg(not(windows))]
	pub(crate) native: String,
}

impl ClipboardFormat {
	/// Registers a native format name, or returns its existing identity.
	///
	/// Use a MIME type on Linux, a pasteboard type on macOS, and a registered name on Windows.
	/// Windows registers the name with the OS. Other platforms use the name directly.
	/// Empty names and names with NUL bytes return [`Error::InvalidFormat`].
	/// Windows compares registered names without regard to case.
	///
	/// See `cargo run --example formats` for a complete example.
	pub fn register(name: &str) -> Result<Self, Error> {
		if name.is_empty() || name.contains('\0') {
			return Err(Error::InvalidFormat);
		}
		crate::platform::register_format(name)
	}

	/// Returns the native name, if the format has one.
	///
	/// Windows standard numeric formats have no registered name.
	pub fn name(&self) -> Result<Option<String>, Error> {
		crate::platform::format_name(self)
	}

	/// Returns the Windows format identifier, including standard numeric identifiers.
	#[cfg(windows)]
	pub fn windows_id(&self) -> u32 {
		self.native
	}

	/// Creates a Windows format identity from a standard or registered identifier.
	///
	/// Zero and identifiers above `0xffff` return [`Error::InvalidFormat`].
	/// This does not make handle-backed formats available for raw access.
	#[cfg(windows)]
	pub fn from_windows_id(id: u32) -> Result<Self, Error> {
		if id == 0 || id > 0xffff {
			return Err(Error::InvalidFormat);
		}
		Ok(Self { native: id })
	}

	pub(crate) fn rtf() -> Result<Self, Error> {
		#[cfg(windows)]
		let name = "Rich Text Format";
		#[cfg(target_os = "macos")]
		let name = "public.rtf";
		#[cfg(all(unix, not(target_os = "macos")))]
		let name = "text/rtf";
		Self::register(name)
	}
}

/// One representation in a single clipboard write.
///
/// Typed variants use native encodings. Raw data passes through without conversion.
/// Use [`crate::Set::formats`] to publish several representations together.
/// Images and files use the same native formats as their typed setters.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum ClipboardContent<'a> {
	/// UTF-8 text, converted to the native text representation.
	Text(Cow<'a, str>),
	/// An HTML fragment. The backend adds native HTML headers where necessary.
	Html(Cow<'a, str>),
	/// RTF bytes. These need not be UTF-8.
	Rtf(Cow<'a, [u8]>),
	/// RGBA pixels, encoded in the native image formats.
	#[cfg(feature = "image-data")]
	Image(crate::ImageData<'a>),
	/// File paths. Every path must resolve before the clipboard changes.
	///
	/// Linux offers URI, GNOME, and Nautilus representations. Windows offers `CF_HDROP`.
	/// macOS offers one file URL per item, with other representations on the first item.
	Files(Cow<'a, [PathBuf]>),
	/// Native bytes with their format identity.
	Raw(ClipboardFormat, Cow<'a, [u8]>),
}

#[cfg(feature = "image-data")]
pub(crate) fn validate_image(image: &crate::ImageData<'_>) -> Result<(), Error> {
	let length = image.width.checked_mul(image.height).and_then(|pixels| pixels.checked_mul(4));
	if image.width == 0
		|| image.height == 0
		|| image.width > u32::MAX as usize
		|| image.height > u32::MAX as usize
		|| length != Some(image.bytes.len())
	{
		return Err(Error::ConversionFailure);
	}
	Ok(())
}

pub(crate) fn push_unique(
	data: &mut Vec<(ClipboardFormat, Vec<u8>)>,
	format: ClipboardFormat,
	bytes: Vec<u8>,
) -> Result<(), Error> {
	if data.iter().any(|(existing, _)| existing == &format) {
		return Err(Error::DuplicateFormat);
	}
	data.push((format, bytes));
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rejects_invalid_names_and_duplicates() {
		assert!(matches!(ClipboardFormat::register(""), Err(Error::InvalidFormat)));
		assert!(matches!(ClipboardFormat::register("a\0b"), Err(Error::InvalidFormat)));
		let format = ClipboardFormat::register("application/x-arboard-example").unwrap();
		let mut data = Vec::new();
		push_unique(&mut data, format.clone(), vec![0, 255]).unwrap();
		assert!(matches!(push_unique(&mut data, format, vec![]), Err(Error::DuplicateFormat)));
		assert_eq!(data[0].1, [0, 255]);
	}
}
