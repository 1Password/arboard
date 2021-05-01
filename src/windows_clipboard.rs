/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use std::{
	borrow::Cow,
	collections::HashSet,
	ffi::{OsStr, OsString},
	os::windows::ffi::OsStringExt,
	ptr::{null, null_mut},
};

use log::{debug, trace, warn};

use clipboard_win::Clipboard as SystemClipboard;
use scopeguard::defer;
use winapi::{
	shared::{minwindef::UINT, ntdef::HANDLE, windef::HBITMAP},
	um::{
		shellapi::{DragQueryFileW, HDROP},
		stringapiset::WideCharToMultiByte,
		winbase::{GlobalLock, GlobalSize, GlobalUnlock},
		winnls::CP_UTF8,
		winuser::{
			EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW, CF_BITMAP, CF_DIB,
			CF_DIBV5, CF_DIF, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT, CF_DSPTEXT,
			CF_ENHMETAFILE, CF_GDIOBJFIRST, CF_GDIOBJLAST, CF_HDROP, CF_LOCALE, CF_METAFILEPICT,
			CF_OEMTEXT, CF_OWNERDISPLAY, CF_PALETTE, CF_PENDATA, CF_PRIVATEFIRST, CF_PRIVATELAST,
			CF_RIFF, CF_SYLK, CF_TEXT, CF_TIFF, CF_UNICODETEXT, CF_WAVE,
		},
	},
};

use crate::common::{
	line_endings_to_crlf, text_from_unknown_encoding, CustomItem, Error, ImageData,
};

mod bitmap;
use bitmap::{add_cf_bitmap, convert_clipboard_bitmap, get_image_from_dib, image_to_dib};

const MAX_OPEN_ATTEMPTS: usize = 5;

pub fn get_string() -> Result<String, Error> {
	use std::mem;

	// This pointer must not be free'd.
	let ptr = unsafe { GetClipboardData(CF_UNICODETEXT) };
	if ptr.is_null() {
		return Err(Error::ContentNotAvailable);
	}

	unsafe {
		let data_ptr = GlobalLock(ptr);
		if data_ptr.is_null() {
			return Err(Error::Unknown {
				description: "GlobalLock on clipboard data returned null.".into(),
			});
		}
		defer!( GlobalUnlock(ptr); );

		let char_count = GlobalSize(ptr) as usize / mem::size_of::<u16>();
		let storage_req_size = WideCharToMultiByte(
			CP_UTF8,
			0,
			data_ptr as _,
			char_count as _,
			null_mut(),
			0,
			null(),
			null_mut(),
		);
		if storage_req_size == 0 {
			return Err(Error::ConversionFailure);
		}

		let mut utf8: Vec<u8> = Vec::with_capacity(storage_req_size as usize);
		let output_size = WideCharToMultiByte(
			CP_UTF8,
			0,
			data_ptr as _,
			char_count as _,
			utf8.as_mut_ptr() as *mut i8,
			storage_req_size,
			null(),
			null_mut(),
		);
		if output_size == 0 {
			return Err(Error::ConversionFailure);
		}
		utf8.set_len(storage_req_size as usize);

		// WideCharToMultiByte appends a terminating null character,
		// if the original string included one or if the length of the original
		// was set to -1
		if let Some(last_byte) = utf8.last() {
			if *last_byte == 0 {
				utf8.set_len(utf8.len() - 1);
			}
		}
		Ok(String::from_utf8_unchecked(utf8))
	}
}

pub struct WindowsClipboardContext;

impl WindowsClipboardContext {
	pub(crate) fn new() -> Result<Self, Error> {
		Ok(WindowsClipboardContext)
	}
	pub(crate) fn get_text(&mut self) -> Result<String, Error> {
		// Using this nifty RAII object to open and close the clipboard.
		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;
		get_string()
	}
	pub(crate) fn set_text(&mut self, data: String) -> Result<(), Error> {
		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;
		clipboard_win::set(clipboard_win::formats::Unicode, data).map_err(|_| Error::Unknown {
			description: "Could not place the specified text to the clipboard".into(),
		})
	}
	pub(crate) fn get_image(&mut self) -> Result<ImageData, Error> {
		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;
		get_image_from_dib()
	}
	pub(crate) fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		//let clipboard = SystemClipboard::new()?;
		let dib = image_to_dib(&image)?;
		let mut result: Result<(), Error> = Ok(());
		//let mut success = false;
		clipboard_win::with_clipboard(|| {
			let success = clipboard_win::raw::set(dib.format(), dib.dib_bytes()).is_ok();
			let bitmap_result = unsafe { add_cf_bitmap(&image) };
			if bitmap_result.is_err() && !success {
				result = Err(Error::Unknown {
					description: "Could not set the image for the clipboard in neither of `CF_DIB` and `CG_BITMAP` formats.".into(),
				});
			}
		})
		.map_err(|_| Error::ClipboardOccupied)?;

		result
	}

	pub(crate) fn set_custom(&mut self, _items: &[CustomItem]) -> Result<(), Error> {
		todo!()
	}

	pub(crate) fn get_all(&mut self) -> Result<Vec<CustomItem>, Error> {
		let raw_img_mime =
			CustomItem::RawImage(ImageData { width: 0, height: 0, bytes: (&[] as &[u8]).into() })
				.media_type();

		// Using this to open, and automatically close the clipboard on return
		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;

		let mut items = Vec::new();
		let mut item_mime_types = HashSet::new();
		let mut has_raw_image = false;
		let mut format = 0;
		loop {
			// `EnumClipboardFormats` not only enumerates the forats that the owner placed onto the clipboard,
			// but it also enumerates all formats that the system can automatically convert to.
			// (Also known as "synthesized formats")
			format = unsafe { EnumClipboardFormats(format) };
			if format == 0 {
				break;
			}
			trace!("Clipboard format: {}", format);
			let allow_raw_image = !has_raw_image;
			if let Some(item) = convert_native_cb_data(format, allow_raw_image) {
				let mime_type = item.media_type();
				if !item_mime_types.contains(mime_type) {
					item_mime_types.insert(mime_type);
					items.push(item);
					if mime_type == raw_img_mime {
						has_raw_image = true;
					}
				}
			}
		}
		Ok(items)
	}
}

/// This function requires that the clipboard is open when it's called.
fn convert_native_cb_data(format: UINT, allow_raw_image: bool) -> Option<CustomItem<'static>> {
	match format {
		// A bitmap may contain PNG or JPG encoded data
		// TODO HANDLE THIS LATER
		CF_BITMAP => {
			if allow_raw_image {
				let hbitmap = unsafe { GetClipboardData(format) };
				convert_clipboard_bitmap(hbitmap as HBITMAP)
			} else {
				None
			}
		}
		CF_DIB => {
			if allow_raw_image {
				match get_image_from_dib() {
					Ok(img) => Some(CustomItem::RawImage(img)),
					Err(e) => {
						warn!("Failed to process CF_DIB image: {}", e);
						None
					}
				}
			} else {
				None
			}
		}
		CF_DIBV5 => None,

		CF_DIF => None,
		CF_DSPBITMAP => None,
		CF_DSPENHMETAFILE => None,
		CF_DSPMETAFILEPICT => None,
		CF_DSPTEXT => None,
		CF_ENHMETAFILE => None,
		CF_GDIOBJFIRST..=CF_GDIOBJLAST => None,

		// A handle to a list of files
		CF_HDROP => {
			let hdrop = unsafe { GetClipboardData(format) };
			convert_clipboard_hdrop(hdrop)
		}

		CF_LOCALE => None,
		CF_METAFILEPICT => None,
		CF_OEMTEXT => None,
		CF_OWNERDISPLAY => None,
		CF_PALETTE => None,
		CF_PENDATA => None,
		CF_PRIVATEFIRST..=CF_PRIVATELAST => None,
		CF_RIFF => None,
		CF_SYLK => None,

		// We don't handle `CF_TEXT` because the system always provides
		// `CF_UNICODETEXT` if a `CF_TEXT` is on the clipboard
		CF_TEXT => None,

		CF_TIFF => None,
		CF_UNICODETEXT => {
			match get_string() {
				Ok(string) => Some(CustomItem::Text(string.into())),
				Err(e) => {
					warn!("Failed to get the contents of a CF_UNICODETEXT clipboard item. Error was: {}", e);
					None
				}
			}
		}
		CF_WAVE => None,

		_ => {
			let mut wstr = [0u16; 32];
			let num_chars =
				unsafe { GetClipboardFormatNameW(format, wstr.as_mut_ptr(), wstr.len() as i32) };
			if num_chars == 0 {
				debug!("Could not get the name of the clipboard format {}", format);
				return None;
			} else {
				let os_str = OsString::from_wide(&wstr[0..num_chars as usize]);
				debug!("The clipboard format name is {:?}", os_str);
				convert_non_system_clipboard_data(format, &os_str)
			}
		}
	}
}

fn convert_non_system_clipboard_data(
	format: UINT,
	format_name: &OsStr,
) -> Option<CustomItem<'static>> {
	if format_name == "HTML Format" {
		// This is the official HTML format on Windows
		// See: https://docs.microsoft.com/en-us/previous-versions/windows/internet-explorer/ie-developer/platform-apis/aa767917(v=vs.85)?redirectedfrom=MSDN
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, convert_clipboard_html)
	} else if format_name == "text/html" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_text(data, "text/html", |s| CustomItem::TextHtml(s.into()))
		})
	} else if format_name == "text/csv" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_text(data, "text/csv", |s| CustomItem::TextCsv(s.into()))
		})
	} else if format_name == "text/css" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_text(data, "text/css", |s| CustomItem::TextCss(s.into()))
		})
	} else if format_name == "application/xhtml+xml" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_app_text(data, "application/xhtml+xml", |s| {
				CustomItem::ApplicationXhtml(s.to_string().into())
			})
		})
	} else if format_name == "application/xml" || format_name == "text/xml" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_app_text(data, "text/csv", |s| {
				CustomItem::TextXml(line_endings_to_crlf(s.as_ref()).to_string().into())
			})
		})
	} else if format_name == "SVG Image" || format_name == "image/svg+xml" {
		// "SVG Image" is the name used on windows according to: https://www.iana.org/assignments/media-types/image/svg+xml
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_app_text(data, "image/svg+xml", |s| {
				CustomItem::ImageSvg(s.to_string().into())
			})
		})
	} else if format_name == "application/javascript" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_app_text(data, "application/javascript", |s| {
				CustomItem::ApplicationJavascript(s.to_string().into())
			})
		})
	} else if format_name == "application/json" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			convert_clipboard_app_text(data, "application/json", |s| {
				CustomItem::ApplicationJson(s.to_string().into())
			})
		})
	} else if format_name == "application/octet-stream" {
		let handle = unsafe { GetClipboardData(format) };
		with_clipboard_data(handle, |data| {
			let data = match data {
				Ok(d) => d,
				Err(e) => {
					warn!("Failed to get the clipboard data for the format 'application/octet-stream'. Error was: {}", e);
					return None;
				}
			};
			Some(CustomItem::ApplicationOctetStream(data.to_owned().into()))
		})
	} else {
		None
	}
}

fn with_clipboard_data<F, T>(data_handle: HANDLE, fun: F) -> T
where
	F: FnOnce(Result<&[u8], &'static str>) -> T,
{
	if data_handle.is_null() {
		return fun(Err("The clipboard data was NULL"));
	}
	let data = unsafe { GlobalLock(data_handle) as *const u8 };
	if data.is_null() {
		return fun(Err("`GlobalLock` returned NULL"));
	}
	defer!(unsafe {
		GlobalUnlock(data_handle);
	});
	let data_len = unsafe { GlobalSize(data_handle) };
	let data_slice = unsafe { std::slice::from_raw_parts(data, data_len) };
	fun(Ok(data_slice))
}

fn read_html_int_field(line: &str, name_w_colon: &str) -> Option<i32> {
	if line.starts_with(name_w_colon) {
		let val_str = &line[name_w_colon.len()..];
		match val_str.parse::<i32>() {
			Ok(v) => Some(v),
			Err(e) => {
				warn!(
					"Found CF_HTML field '{}', but failed to parse its value: {}",
					name_w_colon, e
				);
				None
			}
		}
	} else {
		None
	}
}
// Converts a clipboard item with the format CF_HTML to HTML text
fn convert_clipboard_html(html_data: Result<&[u8], &str>) -> Option<CustomItem<'static>> {
	let html_data = match html_data {
		Ok(d) => d,
		Err(e) => {
			warn!("Failed to read an HTML clipboard item: {}", e);
			return None;
		}
	};
	let data_str = match std::str::from_utf8(html_data) {
		Ok(s) => s,
		Err(e) => {
			warn!("Could not get the HTML data as a utf8 text. Error was: {}", e);
			return None;
		}
	};

	// debug!("Got HTML Format data:\n{}", data_str);

	let mut end_fragment = None;
	let mut start_fragment = None;
	// Using `split()` instead of `lines()` because `lines()` only
	// splits at LF or CRLF, but the CF_HTML header may represent line breaks with CR
	for line in data_str.split(&['\r', '\n'][..]) {
		if let Some(v) = read_html_int_field(line, "EndFragment:") {
			end_fragment = Some(v);
		} else if let Some(v) = read_html_int_field(line, "StartFragment:") {
			start_fragment = Some(v);
		}
		if end_fragment.is_some() && start_fragment.is_some() {
			// Stop parsing the header if we have the information we need from the header.
			break;
		}
	}
	if let (Some(start_fragment), Some(end_fragment)) = (start_fragment, end_fragment) {
		if start_fragment <= 0 {
			warn!("The StartFragment field in a CF_HTML clipboard item was not positive.");
			return None;
		}
		let start_fragment = start_fragment as usize;
		if start_fragment >= data_str.len() {
			warn!("The StartFragment field in a CF_HTML clipboard item had a larger value than the length of the entire clipboard data.");
		}
		if end_fragment <= 0 {
			warn!("The EndFragment field in a CF_HTML clipboard item was not positive.");
			return None;
		}
		let end_fragment = end_fragment as usize;
		if end_fragment > data_str.len() {
			warn!("The EndFragment field in a CF_HTML clipboard item had a larger value than the length of the entire clipboard data.");
			return None;
		}
		let html_text = &data_str[start_fragment..end_fragment];

		// For some reason the compiler is only happy if there's this immediate step
		// where the object is a String
		let owned_text: String = line_endings_to_crlf(html_text).into_owned();
		Some(CustomItem::TextHtml(owned_text.into()))
	} else {
		warn!("Couldn't find either the `StartHTML` or the `StartFragment` field in the CF_HTML clipboard item");
		None
	}
}

fn convert_clipboard_text<F>(
	data: Result<&[u8], &str>,
	data_type: &str,
	mapper: F,
) -> Option<CustomItem<'static>>
where
	F: FnOnce(String) -> CustomItem<'static>,
{
	let data = match data {
		Ok(d) => d,
		Err(e) => {
			warn!("Failed to read a {} clipboard item: {}", data_type, e);
			return None;
		}
	};
	match std::str::from_utf8(data) {
		Ok(s) => Some(mapper(line_endings_to_crlf(s).into_owned())),
		Err(e) => {
			warn!("Failed to convert a {} clipboard item to utf8: {}", data_type, e);
			None
		}
	}
}

/// Converts any text based format that belongs to
/// the "application/" mime type family. (Instead of "text/")
fn convert_clipboard_app_text<F>(
	data: Result<&[u8], &str>,
	data_type: &str,
	mapper: F,
) -> Option<CustomItem<'static>>
where
	F: FnOnce(Cow<'_, str>) -> CustomItem<'static>,
{
	let data = match data {
		Ok(d) => d,
		Err(e) => {
			warn!("Failed to read a {} clipboard item: {}", data_type, e);
			return None;
		}
	};
	let string = match text_from_unknown_encoding(data) {
		// The wording is not entirely clear but it seems that RFC 3023
		// recommends to keep line break in whatever format provided,
		// so we don't convert to CRLF, as we would with "text/" media types.
		Ok(s) => s,
		Err(e) => {
			warn!("Failed to extract text from the data. Error was: {}", e);
			debug!("Failed to extract text from the data. Error was: '{}' Data was: {:?}", e, data);
			return None;
		}
	};
	Some(mapper(string))
}

fn convert_clipboard_hdrop(clipboard_data: HANDLE) -> Option<CustomItem<'static>> {
	if clipboard_data.is_null() {
		warn!("Failed to convert a CF_HDROP item, because the data was NULL");
		return None;
	}
	let hdrop = unsafe { GlobalLock(clipboard_data) as HDROP };
	if hdrop.is_null() {
		warn!("Failed to convert a CF_HDROP item, because `GlobalLock` returned NULL");
		return None;
	}
	defer!(unsafe {
		GlobalUnlock(clipboard_data);
	});

	let file_count = unsafe { DragQueryFileW(hdrop, 0xFFFFFFFF, null_mut(), 0) };
	let last_id = file_count - 1;
	let mut result = String::new();
	for i in 0..file_count {
		let wchar_cnt = unsafe { DragQueryFileW(hdrop, i, null_mut(), 0) };
		if wchar_cnt == 0 {
			warn!("The HDROP item at index {} had a size of zero characters.", i);
			continue;
		}
		let mut wstr: Vec<u16> = Vec::new();
		// Adding one, to allow space for the terminating null
		// (which we don't need but the DragQueryFileW function cuts off the last character if this is not there)
		wstr.resize((wchar_cnt + 1) as usize, 0);

		// Ignoring the return value here because the documentation doesn't say
		// anything about the return value in this case.
		unsafe { DragQueryFileW(hdrop, i, wstr.as_mut_ptr(), wstr.len() as u32) };

		let os_string = OsString::from_wide(&wstr[0..wchar_cnt as usize]);
		let string = match os_string.into_string() {
			Ok(s) => s,
			Err(s) => {
				warn!("Failed to convert the OsString to String when constructing a `TextUriList` from an HDROP. String was: {:?}", s);
				continue;
			}
		};

		let string = string.trim();
		// Remove the "\\?\" prefix if it's present
		let prefix = "\\\\?\\";
		let string = if string.starts_with(prefix) { &string[prefix.len()..] } else { string };
		// Make all slashes forward slashes
		let string = string.replace("\\", "/");
		// Prepend the scheme identifier. Ever wondered why does does the file
		// scheme have three forwards slashes, but all other schemes have only
		// two? Because the file scheme is defined like this:
		// file://<host>/<path>
		// But the host may be empty if the file is on the localhost (this computer).
		result.push_str("file:///");
		result.push_str(&string);
		if last_id != i {
			// All "text/" media types use CRLF line endings
			result.push_str("\r\n");
		}
	}
	Some(CustomItem::TextUriList(result.into()))
}
