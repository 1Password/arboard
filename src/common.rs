/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use std::borrow::Cow;
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
/// This reflects the list of "mandatory data types" specified by
/// The W3C Clipboard APIs document.
/// https://www.w3.org/TR/2021/WD-clipboard-apis-20210203/#mandatory-data-types
///
#[derive(Debug, Clone)]
pub enum CustomItem {
	/// "text/plain"
	TextPlain(String),
	/// "text/uri-list"
	TextUriList(String),
	/// "text/csv"
	TextCsv(String),
	/// "text/css"
	TextCss(String),
	/// "text/html"
	TextHtml(String),
	/// "application/xhtml+xml"
	ApplicationXhtml(String),
	/// "image/png"
	ImagePng(Vec<u8>),
	/// "image/jpg", "image/jpeg"
	ImageJpg(Vec<u8>),
	/// "image/gif"
	ImageGif(Vec<u8>),
	/// "image/svg+xml"
	ImageSvg(String),
	/// "application/xml", "text/xml"
	ApplicationXml(String),
	/// "application/javascript"
	ApplicationJavascript(String),
	/// "application/json"
	ApplicationJson(String),
	/// "application/octet-stream"
	ApplicationOctetStream(Vec<u8>),
}
impl CustomItem {
	/// The MIME type of this item
	pub fn media_type(&self) -> &'static str {
		match self {
			CustomItem::TextPlain(_) => "text/plain",
			CustomItem::TextUriList(_) => "text/uri-list",
			CustomItem::TextCsv(_) => "text/csv",
			CustomItem::TextCss(_) => "text/css",
			CustomItem::TextHtml(_) => "text/html",
			CustomItem::ApplicationXhtml(_) => "application/xhtml+xml",
			CustomItem::ImagePng(_) => "image/png",
			CustomItem::ImageJpg(_) => "image/jpg",
			CustomItem::ImageGif(_) => "image/gif",
			CustomItem::ImageSvg(_) => "image/svg+xml",
			CustomItem::ApplicationXml(_) => "application/xml",
			CustomItem::ApplicationJavascript(_) => "application/javascript",
			CustomItem::ApplicationJson(_) => "application/json",
			CustomItem::ApplicationOctetStream(_) => "application/octet-stream",
		}
	}

	pub fn is_supported_text_type(media_type: &str) -> bool {
		Self::from_text_media_type(String::new(), media_type).is_some()
	}

	pub fn is_supported_octet_type(media_type: &str) -> bool {
		Self::from_octet_media_type(Vec::new(), media_type).is_some()
	}

	/// Return None if the `media_type` is not a supported text format, returns Some otherwise.
	pub fn from_text_media_type(data: String, media_type: &str) -> Option<CustomItem> {
		match media_type {
			"text/plain" => Some(CustomItem::TextPlain(data)),
			"text/uri-list" => Some(CustomItem::TextUriList(data)),
			"text/csv" => Some(CustomItem::TextCsv(data)),
			"text/css" => Some(CustomItem::TextCss(data)),
			"text/html" => Some(CustomItem::TextHtml(data)),
			"application/xhtml+xml" => Some(CustomItem::ApplicationXhtml(data)),
			"image/svg+xml" => Some(CustomItem::ImageSvg(data)),
			"application/xml" => Some(CustomItem::ApplicationXml(data)),
			"text/xml" => Some(CustomItem::ApplicationXml(data)),
			"application/javascript" => Some(CustomItem::ApplicationJavascript(data)),
			"application/json" => Some(CustomItem::ApplicationJson(data)),
			_ => None,
		}
	}

	/// Return None if the `media_type` is not a supported binary format, returns Some otherwise.
	pub fn from_octet_media_type(data: Vec<u8>, media_type: &str) -> Option<CustomItem> {
		match media_type {
			"image/png" => Some(CustomItem::ImagePng(data)),
			"image/jpg" => Some(CustomItem::ImageJpg(data)),
			"image/jpeg" => Some(CustomItem::ImageJpg(data)),
			"image/gif" => Some(CustomItem::ImageGif(data)),
			"application/octet-stream" => Some(CustomItem::ApplicationOctetStream(data)),
			_ => None,
		}
	}
}
