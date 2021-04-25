/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use std::{borrow::Cow, convert::TryInto};

use log::debug;
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

	// /// The format and the data fields of a `CustomItem` don't match.
	// ///
	// /// This is returned from `set_custom`.
	// ///
	// /// For example if the format is `TextCsv` but the data is `Bytes` this error
	// /// is returned.
	// #[error("The data provided with the clipboard item, does not match its `format`")]
	// FormatDataMismatch,
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

// #[derive(Debug, Clone)]
// pub enum CustomData {
// 	Text(String),
// 	Bytes(Vec<u8>),
// }

// #[derive(Debug, Clone)]
// pub struct CustomItem {
// 	pub format: Format,
// 	pub data: CustomData,
// }

/// Possible custom clipboard formats.
///
/// This reflects the list of "mandatory data types" specified by The W3C
/// Clipboard APIs document.
/// <https://www.w3.org/TR/2021/WD-clipboard-apis-20210203/#mandatory-data-types>
///
/// ### Line breaks
/// When receiving data from the clipboard all "text/" media types terminate
/// lines with CRLF (`"\r\n"`) as per RFC 2046.
///
/// When setting the clipboard contents, all line endings in "text/" formats are
/// converted to CRLF (or the system native format), so it's valid to provide
/// text using all of CR, LF, and CRLF. Such an item may also use multiple of
/// these (mixing them).
#[derive(Debug, Clone)]
pub enum CustomItem<'a> {
	/// "text/plain"
	///
	/// WARNING: Line breaks are CRLF. See the documentation of [`CustomItem`].
	TextPlain(Cow<'a, str>),
	/// "text/uri-list"
	///
	/// WARNING: Line breaks are CRLF. See the documentation of [`CustomItem`].
	TextUriList(Cow<'a, str>),
	/// "text/csv"
	///
	/// WARNING: Line breaks are CRLF. See the documentation of [`CustomItem`].
	TextCsv(Cow<'a, str>),
	/// "text/css"
	///
	/// WARNING: Line breaks are CRLF. See the documentation of [`CustomItem`].
	TextCss(Cow<'a, str>),
	/// "text/html"
	///
	/// WARNING: Line breaks are CRLF. See the documentation of [`CustomItem`].
	TextHtml(Cow<'a, str>),
	/// "text/xml"
	///
	/// Data coming from the clipboard in "application/xml" format,
	/// is automaticaly converted to this ("text/xml").
	///
	/// WARNING: Line breaks are CRLF. See the documentation of [`CustomItem`].
	TextXml(Cow<'a, str>),
	/// "application/xhtml+xml"
	ApplicationXhtml(Cow<'a, str>),
	/// "image/png"
	ImagePng(Cow<'a, [u8]>),
	/// "image/jpg", "image/jpeg"
	ImageJpg(Cow<'a, [u8]>),
	/// "image/gif"
	ImageGif(Cow<'a, [u8]>),
	/// "image/svg+xml"
	ImageSvg(Cow<'a, str>),
	/// "application/javascript"
	ApplicationJavascript(Cow<'a, str>),
	/// "application/json"
	ApplicationJson(Cow<'a, str>),
	/// "application/octet-stream"
	ApplicationOctetStream(Cow<'a, [u8]>),
}
impl<'main> CustomItem<'main> {
	/// The MIME type of this item
	pub fn media_type(&self) -> &'static str {
		match self {
			CustomItem::TextPlain(_) => "text/plain",
			CustomItem::TextUriList(_) => "text/uri-list",
			CustomItem::TextCsv(_) => "text/csv",
			CustomItem::TextCss(_) => "text/css",
			CustomItem::TextHtml(_) => "text/html",
			CustomItem::TextXml(_) => "text/xml",
			CustomItem::ApplicationXhtml(_) => "application/xhtml+xml",
			CustomItem::ImagePng(_) => "image/png",
			CustomItem::ImageJpg(_) => "image/jpg",
			CustomItem::ImageGif(_) => "image/gif",
			CustomItem::ImageSvg(_) => "image/svg+xml",
			CustomItem::ApplicationJavascript(_) => "application/javascript",
			CustomItem::ApplicationJson(_) => "application/json",
			CustomItem::ApplicationOctetStream(_) => "application/octet-stream",
		}
	}

	pub fn is_supported_text_type(media_type: &str) -> bool {
		Self::from_text_media_type("", media_type).is_some()
	}

	pub fn is_supported_octet_type(media_type: &str) -> bool {
		Self::from_octet_media_type(&[], media_type).is_some()
	}

	/// Return None if the `media_type` is not a supported text format, returns Some otherwise.
	pub fn from_text_media_type<'a>(data: &'a str, media_type: &str) -> Option<CustomItem<'a>> {
		match media_type {
			"text/plain" => Some(CustomItem::TextPlain(data.into())),
			"text/uri-list" => Some(CustomItem::TextUriList(data.into())),
			"text/csv" => Some(CustomItem::TextCsv(data.into())),
			"text/css" => Some(CustomItem::TextCss(data.into())),
			"text/html" => Some(CustomItem::TextHtml(data.into())),
			"text/xml" => Some(CustomItem::TextXml(data.into())),
			"application/xml" => Some(CustomItem::TextXml(data.into())),
			"application/xhtml+xml" => Some(CustomItem::ApplicationXhtml(data.into())),
			"image/svg+xml" => Some(CustomItem::ImageSvg(data.into())),
			"application/javascript" => Some(CustomItem::ApplicationJavascript(data.into())),
			"application/json" => Some(CustomItem::ApplicationJson(data.into())),
			_ => None,
		}
	}

	/// Return None if the `media_type` is not a supported binary format, returns Some otherwise.
	pub fn from_octet_media_type<'a>(data: &'a [u8], media_type: &str) -> Option<CustomItem<'a>> {
		match media_type {
			"image/png" => Some(CustomItem::ImagePng(data.into())),
			"image/jpg" => Some(CustomItem::ImageJpg(data.into())),
			"image/jpeg" => Some(CustomItem::ImageJpg(data.into())),
			"image/gif" => Some(CustomItem::ImageGif(data.into())),
			"application/octet-stream" => Some(CustomItem::ApplicationOctetStream(data.into())),
			_ => None,
		}
	}
}

pub fn line_endings_to_crlf<'a>(text: &'a str) -> Cow<'a, str> {
	// TODO: The signature of this function allows the input reference to be
	// returned within the output cow. This design allow us to avoid copying the
	// text in case the original is already using CRLF line breaks.
	//
	// HOWEVER I just cannot be bothered right now to figure out how to juggle
	// with the references/slices to get this right so I'm gonna go the easy way
	// and just always build a new string.
	let mut result = Vec::<u8>::with_capacity(text.len());
	let mut prev_ch = 0;
	for &ch in text.as_bytes() {
		let mut push_current = true;
		if ch == b'\r' {
			// If this is \r, than no matter what follows the \r, we
			// definitely want to insert a line break here.
			result.push(b'\r');
			result.push(b'\n');
			push_current = false;
		}
		if prev_ch == b'\r' && ch == b'\n' {
			// If this is a \r\n then no need to do anything because
			// we already pushed a crlf at the previous character
			push_current = false;
		}
		if prev_ch != b'\r' && ch == b'\n' {
			result.push(b'\r');
			result.push(b'\n');
			push_current = false;
		}
		if push_current {
			result.push(ch);
		}
		prev_ch = ch;
	}
	// This is safe because the original is an `&str` which is utf8,
	// and we only replaced certain utf8 characters with other utf8 characters
	unsafe { String::from_utf8_unchecked(result).into() }
}

/// Takes an array of bytes and attempts to detect the encoding of text
/// in the array. For example if the array starts with a UTF16 byte order mark,
/// the text is converted to a Rust `str` accordingly (omiting the byte order mark).
///
/// If the conversion fails, an error message is returned.
pub fn text_from_unknown_encoding<'a>(text: &'a [u8]) -> Result<Cow<'a, str>, &'static str> {
	// Whoo boy... the code in this function is really error prone... but
	// luckily "unit tests" are a thing.

	// Try to detect the BOM (byte order mark)
	// let system_is_be = cfg!(target_endian = "big");
	let utf32_be = [0x00, 0x00, 0xFE, 0xFF];
	let utf32_le = [0xFF, 0xFE, 0x00, 0x00];
	let utf16_be = [0xFE, 0xFF];
	let utf16_le = [0xFF, 0xFE];
	let utf8 = [0xEF, 0xBB, 0xBF];
	if text.starts_with(&utf32_be) {
		let mut tmp_s = String::with_capacity(text.len() / 4);
		// Start at [4] because we are skipping the BOM
		for chunk in text[4..].chunks(4) {
			match chunk.try_into() {
				Ok(arr) => {
					let chr = u32::from_be_bytes(arr);
					match std::char::from_u32(chr) {
						Some(chr) => tmp_s.push(chr),
						None => {
							return Err("Failed to convert a UTF32 code point into a character")
						}
					}
				}
				Err(_) => return Err("Failed to convert a slice of bytes into an array of 4"),
			}
		}
		return Ok(tmp_s.into());
	} else if text.starts_with(&utf32_le) {
		let mut tmp_s = String::with_capacity(text.len() / 4);
		// Start at [4] because we are skipping the BOM
		for chunk in text[4..].chunks(4) {
			match chunk.try_into() {
				Ok(arr) => {
					let chr = u32::from_le_bytes(arr);
					match std::char::from_u32(chr) {
						Some(chr) => tmp_s.push(chr),
						None => {
							return Err("Failed to convert a UTF32 code point into a character")
						}
					}
				}
				Err(_) => return Err("Failed to convert a slice of bytes into an array of 4"),
			}
		}
		return Ok(tmp_s.into());
	} else if text.starts_with(&utf16_be) {
		// Start at [2] because we are skipping the BOM
		match text_from_utf16_be(&text[2..]) {
			Ok(s) => return Ok(s.into()),
			Err(e) => return Err(e),
		}
	} else if text.starts_with(&utf16_le) {
		// Start at [2] because we are skipping the BOM
		match text_from_utf16_le(&text[2..]) {
			Ok(s) => return Ok(s.into()),
			Err(e) => return Err(e),
		}
	} else if text.starts_with(&utf8) {
		match std::str::from_utf8(&text[utf8.len()..]) {
			Ok(s) => return Ok(s.into()),
			Err(_) => {
				return Err("The string started with a UTF8 BOM, but contained invalid UTF8 data.");
			}
		}
	}
	// Even if there's no BOM, the contents may still be UTF16
	// so let's check if a zero byte shows up more frequently on
	// odd indices or on even indices.
	let mut zeroes_at_even = 0;
	let mut zeroes_at_odd = 0;
	for (i, byte) in text.iter().enumerate() {
		if *byte == 0 {
			if i & 1 == 0 {
				zeroes_at_even += 1;
			} else {
				zeroes_at_odd += 1;
			}
		}
	}
	let is_little_endian = zeroes_at_odd > zeroes_at_even;
	if is_little_endian {
		// Let's attempt to convert as if it was little endian, but don't fail
		// it wasn't succesful.
		match text_from_utf16_le(text) {
			Ok(s) => return Ok(s.into()),
			Err(e) => {
				debug!("The text contained more zero bytes at odd indices, but failed to be converted to UTF-16 LE. {}", e);
			}
		}
	}
	let is_big_endian = zeroes_at_even > zeroes_at_odd;
	if is_big_endian {
		// Let's attempt to convert as if it was little endian, but don't fail
		// it wasn't succesful.
		match text_from_utf16_be(text) {
			Ok(s) => return Ok(s.into()),
			Err(e) => {
				debug!("The text contained more zero bytes at even indices, but failed to be converted to UTF-16 BE. {}", e);
			}
		}
	}
	// If both the big endian and the little endian utf-16 conversions failed, then just assume the text is utf-8
	match std::str::from_utf8(text) {
		Ok(s) => return Ok(s.into()),
		Err(_) => {
			return Err("The string is assumed to be UTF8, but contained invalid UTF8 data.");
		}
	}
}

/// `text` must NOT contain the byte order mark
pub(crate) fn text_from_utf16_le(text: &[u8]) -> Result<String, &'static str> {
	let mut chars: Vec<u16> = Vec::with_capacity(text.len() / 2);
	for chunk in text.chunks(2) {
		match chunk.try_into() {
			Ok(arr) => chars.push(u16::from_le_bytes(arr)),
			Err(_) => return Err("Failed to convert a slice of bytes into an array of 2"),
		}
	}
	match String::from_utf16(&chars) {
		Ok(s) => Ok(s.into()),
		Err(_) => Err("The string contained a UTF16 LE BOM, but contained invalid UTF16 data."),
	}
}

/// `text` must NOT contain the byte order mark
pub(crate) fn text_from_utf16_be(text: &[u8]) -> Result<String, &'static str> {
	let mut chars: Vec<u16> = Vec::with_capacity(text.len() / 2);
	for chunk in text.chunks(2) {
		match chunk.try_into() {
			Ok(arr) => chars.push(u16::from_be_bytes(arr)),
			Err(_) => return Err("Failed to convert a slice of bytes into an array of 2"),
		}
	}
	match String::from_utf16(&chars) {
		Ok(s) => Ok(s.into()),
		Err(_) => Err("The string contained data that's not valid UTF-16 BE."),
	}
}

#[cfg(test)]
mod tests {

	use super::{line_endings_to_crlf, text_from_unknown_encoding};

	#[test]
	fn test_text_from_utf8() {
		let input = "Hello utf8. æøå áéóúüűöéő";
		let output = text_from_unknown_encoding(input.as_bytes()).unwrap();
		assert_eq!(input, output);
	}

	#[test]
	fn test_text_from_utf16_le() {
		let expected = "Hello utf16 LE. æøå áéóúüűöéő";
		// Obtained with notepad + powershell Get-Content
		let input = [
			255, 254, 72, 0, 101, 0, 108, 0, 108, 0, 111, 0, 32, 0, 117, 0, 116, 0, 102, 0, 49, 0,
			54, 0, 32, 0, 76, 0, 69, 0, 46, 0, 32, 0, 230, 0, 248, 0, 229, 0, 32, 0, 225, 0, 233,
			0, 243, 0, 250, 0, 252, 0, 113, 1, 246, 0, 233, 0, 81, 1,
		];
		let output = text_from_unknown_encoding(&input).unwrap();
		assert_eq!(expected, output);
	}
	#[test]
	fn test_text_from_utf16_be() {
		let expected = "Hello utf16 BE. æøå áéóúüűöéő";
		// Obtained with notepad + powershell Get-Content
		let input = [
			254, 255, 0, 72, 0, 101, 0, 108, 0, 108, 0, 111, 0, 32, 0, 117, 0, 116, 0, 102, 0, 49,
			0, 54, 0, 32, 0, 66, 0, 69, 0, 46, 0, 32, 0, 230, 0, 248, 0, 229, 0, 32, 0, 225, 0,
			233, 0, 243, 0, 250, 0, 252, 1, 113, 0, 246, 0, 233, 1, 81,
		];
		let output = text_from_unknown_encoding(&input).unwrap();
		assert_eq!(expected, output);
	}
	#[test]
	fn test_text_from_utf16_le_without_bom() {
		let expected = "Hello utf16 LE. æøå áéóúüűöéő";
		// Obtained with notepad + powershell Get-Content
		let input = [
			72, 0, 101, 0, 108, 0, 108, 0, 111, 0, 32, 0, 117, 0, 116, 0, 102, 0, 49, 0, 54, 0, 32,
			0, 76, 0, 69, 0, 46, 0, 32, 0, 230, 0, 248, 0, 229, 0, 32, 0, 225, 0, 233, 0, 243, 0,
			250, 0, 252, 0, 113, 1, 246, 0, 233, 0, 81, 1,
		];
		let output = text_from_unknown_encoding(&input).unwrap();
		assert_eq!(expected, output);
	}
	#[test]
	fn test_text_from_utf16_be_without_bom() {
		let expected = "Hello utf16 BE. æøå áéóúüűöéő";
		// Obtained with notepad + powershell Get-Content
		let input = [
			0, 72, 0, 101, 0, 108, 0, 108, 0, 111, 0, 32, 0, 117, 0, 116, 0, 102, 0, 49, 0, 54, 0,
			32, 0, 66, 0, 69, 0, 46, 0, 32, 0, 230, 0, 248, 0, 229, 0, 32, 0, 225, 0, 233, 0, 243,
			0, 250, 0, 252, 1, 113, 0, 246, 0, 233, 1, 81,
		];
		let output = text_from_unknown_encoding(&input).unwrap();
		assert_eq!(expected, output);
	}
	// TODO add tests for UTF-32

	#[test]
	fn test_line_endings_to_crlf_begin() {
		let input = "\nabcd";
		let expected = "\r\nabcd";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);

		let input = "\rabcd";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);
	}
	#[test]
	fn test_line_endings_to_crlf_end() {
		let input = "abcd\n";
		let expected = "abcd\r\n";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);

		let input = "abcd\r";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);
	}
	#[test]
	fn test_line_endings_to_crlf_middle() {
		let input = "abcd\nqwer";
		let expected = "abcd\r\nqwer";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);

		let input = "abcd\rqwer";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);
	}
	#[test]
	fn test_line_endings_to_crlf_multiple() {
		let input = "\nabcd\n\nqwer\n";
		let expected = "\r\nabcd\r\n\r\nqwer\r\n";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);

		let input = "\rabcd\r\rqwer\r";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);
	}
	#[test]
	fn test_line_endings_to_crlf_mixed() {
		let input = "\nabcd\r\n.\r\n\nqwer\r";
		let expected = "\r\nabcd\r\n.\r\n\r\nqwer\r\n";
		let output = line_endings_to_crlf(input);
		assert_eq!(expected, output);
	}
}
