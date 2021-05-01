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
	convert::TryInto,
	ffi::{OsStr, OsString},
	io::{self, Read, Seek},
	mem::{size_of, MaybeUninit},
	os::windows::ffi::OsStringExt,
	ptr::{null, null_mut},
};

use log::{debug, trace, warn};

use clipboard_win::Clipboard as SystemClipboard;
use image::{
	bmp::{BmpDecoder, BmpEncoder},
	ColorType, ImageDecoder,
};
use scopeguard::defer;
use winapi::{
	shared::{
		minwindef::{DWORD, UINT, WORD},
		ntdef::HANDLE,
		windef::HBITMAP,
	},
	um::{
		errhandlingapi::GetLastError,
		minwinbase::LPTR,
		shellapi::{DragQueryFileW, HDROP},
		stringapiset::WideCharToMultiByte,
		winbase::{GlobalLock, GlobalSize, GlobalUnlock, LocalAlloc, LocalFree},
		wingdi::{
			CreateDIBitmap, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
			BITMAPINFOHEADER, BITMAPV4HEADER, BI_BITFIELDS, BI_PNG, BI_RGB, CBM_INIT, CIEXYZTRIPLE,
			DIB_RGB_COLORS, PBITMAPINFO, RGBQUAD,
		},
		winnls::CP_UTF8,
		winnt::LONG,
		winuser::{
			EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW, GetDC, ReleaseDC,
			SetClipboardData, CF_BITMAP, CF_DIB, CF_DIBV5, CF_DIF, CF_DSPBITMAP, CF_DSPENHMETAFILE,
			CF_DSPMETAFILEPICT, CF_DSPTEXT, CF_ENHMETAFILE, CF_GDIOBJFIRST, CF_GDIOBJLAST,
			CF_HDROP, CF_LOCALE, CF_METAFILEPICT, CF_OEMTEXT, CF_OWNERDISPLAY, CF_PALETTE,
			CF_PENDATA, CF_PRIVATEFIRST, CF_PRIVATELAST, CF_RIFF, CF_SYLK, CF_TEXT, CF_TIFF,
			CF_UNICODETEXT, CF_WAVE,
		},
	},
};

use crate::common::{
	line_endings_to_crlf, text_from_unknown_encoding, CustomItem, Error, ImageData,
};

const MAX_OPEN_ATTEMPTS: usize = 5;

const BITMAP_FILE_HEADER_SIZE: usize = 14;
//const BITMAP_INFO_HEADER_SIZE: usize = 40;
const BITMAP_V4_INFO_HEADER_SIZE: u32 = 108;

struct FakeBitmapFile {
	file_header: [u8; BITMAP_FILE_HEADER_SIZE],
	bitmap: Vec<u8>,

	curr_pos: usize,
}

impl FakeBitmapFile {
	fn len(&self) -> usize {
		self.file_header.len() + self.bitmap.len()
	}
}

impl Seek for FakeBitmapFile {
	fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
		match pos {
			io::SeekFrom::Start(p) => self.curr_pos = p as usize,
			io::SeekFrom::End(p) => self.curr_pos = self.len() + p as usize,
			io::SeekFrom::Current(p) => self.curr_pos += p as usize,
		}
		Ok(self.curr_pos as u64)
	}
}

impl Read for FakeBitmapFile {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		let remaining = self.len() - self.curr_pos;
		let total_read_len = buf.len().min(remaining);
		let mut buf_pos = 0;

		if total_read_len == 0 {
			return Ok(0);
		}

		// Read from the header
		if self.curr_pos < self.file_header.len() {
			let copy_len = (self.file_header.len() - self.curr_pos).min(total_read_len);
			let header_end = self.curr_pos + copy_len;
			buf[0..copy_len].copy_from_slice(&self.file_header[self.curr_pos..header_end]);
			buf_pos += copy_len;
			self.curr_pos += copy_len;
		}
		// Read from the bitmap
		let remaining_read_len = total_read_len - buf_pos;
		if remaining_read_len > 0 {
			let bitmap_start = self.curr_pos - self.file_header.len();
			if bitmap_start < self.bitmap.len() {
				let copy_len = (self.bitmap.len() - bitmap_start).min(remaining_read_len);
				let bitmap_end = bitmap_start + copy_len;
				let buf_end = buf_pos + copy_len;
				buf[buf_pos..buf_end].copy_from_slice(&self.bitmap[bitmap_start..bitmap_end]);
				self.curr_pos += copy_len;
			}
		}
		Ok(total_read_len)
	}
}

unsafe fn add_cf_bitmap(image: &ImageData) -> Result<(), Error> {
	// TODO this might be incorrect.
	// the bV4RedMask, bV4GreenMask, bV4BlueMask
	// might be dependent on endianness
	let header = BITMAPV4HEADER {
		bV4Size: std::mem::size_of::<BITMAPV4HEADER>() as _,
		bV4Width: image.width as LONG,
		bV4Height: -(image.height as LONG),
		bV4Planes: 1,
		bV4BitCount: 32,
		bV4V4Compression: BI_BITFIELDS,
		bV4SizeImage: (4 * image.width * image.height) as DWORD,
		bV4XPelsPerMeter: 3000,
		bV4YPelsPerMeter: 3000,
		bV4ClrUsed: 0,
		bV4ClrImportant: 3,
		bV4RedMask: 0xff0000,
		bV4GreenMask: 0x00ff00,
		bV4BlueMask: 0x0000ff,
		bV4AlphaMask: 0x000000,
		bV4CSType: 0,
		bV4Endpoints: std::mem::MaybeUninit::<CIEXYZTRIPLE>::zeroed().assume_init(),
		bV4GammaRed: 0,
		bV4GammaGreen: 0,
		bV4GammaBlue: 0,
	};

	let hdc = GetDC(null_mut());
	let hbitmap = CreateDIBitmap(
		hdc,
		&header as *const BITMAPV4HEADER as *const _,
		CBM_INIT,
		image.bytes.as_ptr() as *const _,
		&header as *const BITMAPV4HEADER as *const _,
		DIB_RGB_COLORS,
	);
	if SetClipboardData(CF_BITMAP, hbitmap as _).is_null() {
		DeleteObject(hbitmap as _);
		return Err(Error::Unknown {
			description: String::from("Call to `SetClipboardData` returned NULL"),
		});
	}
	Ok(())
}

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
		let mut bmp_data = Vec::with_capacity(image.bytes.len());
		let mut cursor = std::io::Cursor::new(&mut bmp_data);
		let mut encoder = BmpEncoder::new(&mut cursor);
		encoder
			.encode(&image.bytes, image.width as u32, image.height as u32, ColorType::Rgba8)
			.map_err(|_| Error::ConversionFailure)?;
		let data_without_file_header = &bmp_data[BITMAP_FILE_HEADER_SIZE..];
		let header_size = u32::from_le_bytes(data_without_file_header[..4].try_into().unwrap());
		let format = if header_size > BITMAP_V4_INFO_HEADER_SIZE {
			clipboard_win::formats::CF_DIBV5
		} else {
			clipboard_win::formats::CF_DIB
		};
		let mut result: Result<(), Error> = Ok(());
		//let mut success = false;
		clipboard_win::with_clipboard(|| {
			let success = clipboard_win::raw::set(format, data_without_file_header).is_ok();
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
		let raw_img_mime = CustomItem::RawImage(ImageData{
			width: 0,
			height: 0,
			bytes: (&[] as &[u8]).into()
		}).media_type();

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
		},
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

/// It's the caller's responsibility to free the returned pointer, using `LocalFree`
///
/// Source: https://docs.microsoft.com/en-us/windows/win32/gdi/storing-an-image
fn create_bitmap_info_struct(hbmp: HBITMAP) -> Result<*mut BITMAPINFO, &'static str> {
	unsafe {
		// Retrieve the bitmap color format, width, and height.
		let mut bmp = MaybeUninit::<BITMAP>::uninit();
		if 0 == GetObjectW(hbmp as *mut _, size_of::<BITMAP>() as i32, bmp.as_mut_ptr() as *mut _) {
			return Err("GetObjectW returned 0");
		}
		let bmp = bmp.assume_init();
		// Convert the color format to a count of bits.
		let mut clr_bits = (bmp.bmPlanes * bmp.bmBitsPixel) as WORD;

		// PBITMAPINFO pbmi;
		if clr_bits == 1 {
			clr_bits = 1;
		} else if clr_bits <= 4 {
			clr_bits = 4;
		} else if clr_bits <= 8 {
			clr_bits = 8;
		} else if clr_bits <= 16 {
			clr_bits = 16;
		} else if clr_bits <= 24 {
			clr_bits = 24;
		} else {
			clr_bits = 32;
		}

		// Allocate memory for the BITMAPINFO structure. (This structure
		// contains a BITMAPINFOHEADER structure and an array of RGBQUAD
		// data structures.)
		let pbmi = if clr_bits < 24 {
			LocalAlloc(LPTR, size_of::<BITMAPINFOHEADER>() + size_of::<RGBQUAD>() * (1 << clr_bits))
				as PBITMAPINFO
		} else {
			// There is no RGBQUAD array for these formats: 24-bit-per-pixel or 32-bit-per-pixel
			LocalAlloc(LPTR, size_of::<BITMAPINFOHEADER>()) as PBITMAPINFO
		};

		// Initialize the fields in the BITMAPINFO structure.
		(*pbmi).bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
		(*pbmi).bmiHeader.biWidth = bmp.bmWidth;
		(*pbmi).bmiHeader.biHeight = bmp.bmHeight;
		(*pbmi).bmiHeader.biPlanes = bmp.bmPlanes;
		(*pbmi).bmiHeader.biBitCount = bmp.bmBitsPixel;
		if clr_bits < 24 {
			(*pbmi).bmiHeader.biClrUsed = 1 << clr_bits;
		}

		// If the bitmap is not compressed, set the BI_RGB flag.
		(*pbmi).bmiHeader.biCompression = BI_RGB;

		// Compute the number of bytes in the array of color
		// indices and store the result in biSizeImage.
		// The width must be DWORD aligned unless the bitmap is RLE
		// compressed.
		let bytes_per_scanline = (((*pbmi).bmiHeader.biWidth * clr_bits as i32 + 31) & !31) / 8;
		(*pbmi).bmiHeader.biSizeImage =
			bytes_per_scanline as u32 * (*pbmi).bmiHeader.biHeight as u32;
		// Set biClrImportant to 0, indicating that all of the
		// device colors are important.
		(*pbmi).bmiHeader.biClrImportant = 0;
		Ok(pbmi)
	}
}

/// Assumes that the clipboard is open.
fn get_image_from_dib() -> Result<ImageData<'static>, Error> {
	let format = clipboard_win::formats::CF_DIB;
	let size;
	match clipboard_win::raw::size(format) {
		Some(s) => size = s,
		None => return Err(Error::ContentNotAvailable),
	}
	let mut data = vec![0u8; size.into()];
	clipboard_win::raw::get(format, &mut data).map_err(|_| Error::Unknown {
		description: "failed to get image data from the clipboard".into(),
	})?;
	let info_header_size = u32::from_le_bytes(data[..4].try_into().unwrap());
	let mut fake_bitmap_file =
		FakeBitmapFile { bitmap: data, file_header: [0; BITMAP_FILE_HEADER_SIZE], curr_pos: 0 };
	fake_bitmap_file.file_header[0] = b'B';
	fake_bitmap_file.file_header[1] = b'M';

	let file_size =
		u32::to_le_bytes((fake_bitmap_file.bitmap.len() + BITMAP_FILE_HEADER_SIZE) as u32);
	fake_bitmap_file.file_header[2..6].copy_from_slice(&file_size);

	let data_offset = u32::to_le_bytes(info_header_size + BITMAP_FILE_HEADER_SIZE as u32);
	fake_bitmap_file.file_header[10..14].copy_from_slice(&data_offset);

	let bmp_decoder = BmpDecoder::new(fake_bitmap_file).unwrap();
	let (w, h) = bmp_decoder.dimensions();
	let width = w as usize;
	let height = h as usize;
	let image =
		image::DynamicImage::from_decoder(bmp_decoder).map_err(|_| Error::ConversionFailure)?;
	Ok(ImageData { width, height, bytes: Cow::from(image.into_rgba8().into_raw()) })
}

/// Converts from CF_BITMAP
fn convert_clipboard_bitmap(data: HBITMAP) -> Option<CustomItem<'static>> {
	// According to MSDN, in the GetDIBits function:
	//
	// If lpvBits is NULL and the bit count member of BITMAPINFO is initialized
	// to zero, GetDIBits fills in a BITMAPINFOHEADER structure or
	// BITMAPCOREHEADER without the color table. This technique can be used to
	// query bitmap attributes.
	//
	// Source: https://docs.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-getdibits
	let mut info = BITMAPINFO {
		bmiColors: [RGBQUAD { rgbRed: 0, rgbGreen: 0, rgbBlue: 0, rgbReserved: 0 }],
		bmiHeader: BITMAPINFOHEADER {
			biSize: size_of::<BITMAPINFOHEADER>() as u32,
			biBitCount: 0,
			biSizeImage: 0,
			biWidth: 0,
			biHeight: 0,
			biClrImportant: 0,
			biClrUsed: 0,
			biCompression: BI_RGB,
			biPlanes: 0,
			biXPelsPerMeter: 0,
			biYPelsPerMeter: 0,
		},
	};
	let dc = unsafe { GetDC(null_mut()) };
	if dc.is_null() {
		warn!("`GetDC` returned NULL");
		return None;
	}
	defer!(unsafe {
		ReleaseDC(null_mut(), dc);
	});
	let res = unsafe {
		GetDIBits(dc, data as HBITMAP, 0, 0, null_mut(), &mut info as *mut _, DIB_RGB_COLORS)
	};
	if res == 0 {
		let err = unsafe { GetLastError() };
		warn!("Info querying `GetDIBits` returned zero - GetLastError returned: {}", err);
		return None;
	}
	debug!("Reported img size after info query {:#?}", info.bmiHeader.biSizeImage);

	info.bmiHeader.biHeight = -info.bmiHeader.biHeight.abs();
	info.bmiHeader.biPlanes = 1;
	info.bmiHeader.biBitCount = 32;
	info.bmiHeader.biCompression = BI_RGB; // This also applies to rgba
	info.bmiHeader.biClrUsed = 0;
	info.bmiHeader.biClrImportant = 0;
	let mut img_bytes = Vec::<u8>::with_capacity(info.bmiHeader.biSizeImage as usize);
	let res = unsafe {
		GetDIBits(
			dc,
			data as HBITMAP,
			0,
			info.bmiHeader.biHeight as u32,
			img_bytes.as_mut_ptr() as *mut _,
			&mut info as *mut _,
			DIB_RGB_COLORS,
		)
	};
	if res == 0 {
		let err = unsafe { GetLastError() };
		warn!("Data querying `GetDIBits` returned zero - GetLastError returned: {}", err);
		return None;
	}
	unsafe { img_bytes.set_len(info.bmiHeader.biSizeImage as usize) };

	// Now convert the Windows-provided BGRA into RGBA
	for pixel in img_bytes.chunks_mut(4) {
		// Just swap red and blue
		let b = pixel[0];
		let r = pixel[2];
		pixel[0] = r;
		pixel[2] = b;
	}
	let result_img = ImageData {
		width: info.bmiHeader.biWidth as usize,
		height: info.bmiHeader.biHeight.abs() as usize,
		bytes: img_bytes.into(),
	};
	return Some(CustomItem::RawImage(result_img));

	// let mut png_bytes = Vec::new();
	// let enc = image::png::PngEncoder::new_with_quality(
	// 	&mut png_bytes,
	// 	image::png::CompressionType::Fast,
	// 	image::png::FilterType::NoFilter,
	// );
	// let start = std::time::Instant::now();
	// let res = enc.encode(
	// 	&img_bytes,
	// 	info.bmiHeader.biWidth as u32,
	// 	info.bmiHeader.biHeight.abs() as u32,
	// 	ColorType::Rgba8,
	// );
	// if let Err(e) = res {
	// 	warn!("Failed to encode the clipboard image as a png, error was: {}", e);
	// 	return None;
	// }
	// debug!("Encoded into png in {}s", start.elapsed().as_secs_f32());
	// Some(CustomItem::ImagePng(png_bytes.into()))
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
