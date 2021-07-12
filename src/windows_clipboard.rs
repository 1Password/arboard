/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

#[cfg(feature = "image-data")]
use std::io::{self, Read, Seek};

use clipboard_win::Clipboard as SystemClipboard;
#[cfg(feature = "image-data")]
use image::{
	bmp::{BmpDecoder, BmpEncoder},
	ColorType, ImageDecoder,
};
use scopeguard::defer;
use winapi::um::{
	stringapiset::WideCharToMultiByte,
	winbase::{GlobalLock, GlobalSize, GlobalUnlock},
	winnls::CP_UTF8,
	winuser::{GetClipboardData, CF_UNICODETEXT},
};
#[cfg(feature = "image-data")]
use winapi::{
	shared::minwindef::DWORD,
	um::{
		wingdi::{
			CreateDIBitmap, DeleteObject, BITMAPV4HEADER, BI_BITFIELDS, CBM_INIT, CIEXYZTRIPLE,
			DIB_RGB_COLORS,
		},
		winnt::LONG,
		winuser::{GetDC, SetClipboardData, CF_BITMAP},
	},
};

use super::common::Error;
#[cfg(feature = "image-data")]
use super::common::ImageData;

const MAX_OPEN_ATTEMPTS: usize = 5;

#[cfg(feature = "image-data")]
const BITMAP_FILE_HEADER_SIZE: usize = 14;
//const BITMAP_INFO_HEADER_SIZE: usize = 40;
#[cfg(feature = "image-data")]
const BITMAP_V4_INFO_HEADER_SIZE: u32 = 108;

#[cfg(feature = "image-data")]
struct FakeBitmapFile {
	file_header: [u8; BITMAP_FILE_HEADER_SIZE],
	bitmap: Vec<u8>,

	curr_pos: usize,
}

#[cfg(feature = "image-data")]
impl FakeBitmapFile {
	fn len(&self) -> usize {
		self.file_header.len() + self.bitmap.len()
	}
}

#[cfg(feature = "image-data")]
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

#[cfg(feature = "image-data")]
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

#[cfg(feature = "image-data")]
unsafe fn add_cf_bitmap(image: &ImageData) -> Result<(), Error> {
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
		bV4ClrImportant: 0,
		// I'm not sure if the nedianness conversion is good to do
		bV4RedMask: u32::from_le(0x0000ff),
		bV4GreenMask: u32::from_le(0x00ff00),
		bV4BlueMask: u32::from_le(0xff0000),
		bV4AlphaMask: u32::from_le(0x00000000),
		bV4CSType: 0,
		bV4Endpoints: std::mem::MaybeUninit::<CIEXYZTRIPLE>::zeroed().assume_init(),
		bV4GammaRed: 0,
		bV4GammaGreen: 0,
		bV4GammaBlue: 0,
	};

	let hdc = GetDC(std::ptr::null_mut());
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

pub fn get_string(out: &mut Vec<u8>) -> Result<(), Error> {
	use std::mem;
	use std::ptr;

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
			ptr::null_mut(),
			0,
			ptr::null(),
			ptr::null_mut(),
		);
		if storage_req_size == 0 {
			return Err(Error::ConversionFailure);
		}

		let storage_cursor = out.len();
		out.reserve(storage_req_size as usize);
		let storage_ptr = out.as_mut_ptr().add(storage_cursor) as *mut _;
		let output_size = WideCharToMultiByte(
			CP_UTF8,
			0,
			data_ptr as _,
			char_count as _,
			storage_ptr,
			storage_req_size,
			ptr::null(),
			ptr::null_mut(),
		);
		if output_size == 0 {
			return Err(Error::ConversionFailure);
		}
		out.set_len(storage_cursor + storage_req_size as usize);

		//It seems WinAPI always supposed to have at the end null char.
		//But just to be safe let's check for it and only then remove.
		if let Some(last_byte) = out.last() {
			if *last_byte == 0 {
				out.set_len(out.len() - 1);
			}
		}
	}
	Ok(())
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
		let mut result = String::new();
		get_string(unsafe { result.as_mut_vec() })?;
		Ok(result)
	}
	pub(crate) fn set_text(&mut self, data: String) -> Result<(), Error> {
		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;
		clipboard_win::set(clipboard_win::formats::Unicode, data).map_err(|_| Error::Unknown {
			description: "Could not place the specified text to the clipboard".into(),
		})
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn get_image(&mut self) -> Result<ImageData, Error> {
		use std::borrow::Cow;
		use std::convert::TryInto;

		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;
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

	#[cfg(feature = "image-data")]
	pub(crate) fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		use std::convert::TryInto;

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
}
