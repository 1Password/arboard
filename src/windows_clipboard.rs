/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

#[cfg(feature = "image-data")]
use std::mem::size_of;

use log::error;

use clipboard_win::Clipboard as SystemClipboard;

use scopeguard::defer;
use winapi::um::{
	errhandlingapi::GetLastError,
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
			CreateDIBitmap, DeleteObject, GetDIBits, LCS_sRGB, BITMAPINFO, BITMAPINFOHEADER,
			BITMAPV5HEADER, BI_RGB, CBM_INIT, CIEXYZTRIPLE, DIB_RGB_COLORS, LCS_GM_IMAGES,
			PROFILE_EMBEDDED, PROFILE_LINKED, RGBQUAD,
		},
		winnt::LONG,
		winuser::{GetDC, SetClipboardData},
	},
};

use super::common::Error;

#[cfg(feature = "image-data")]
use super::common::ImageData;

const MAX_OPEN_ATTEMPTS: usize = 5;

#[cfg(feature = "image-data")]
unsafe fn add_cf_dibv5(image: ImageData) -> Result<(), Error> {
	use std::intrinsics::copy_nonoverlapping;
	use winapi::um::{
		winbase::{GlobalAlloc, GHND},
		wingdi::BI_BITFIELDS,
		winuser::CF_DIBV5,
	};

	let header_size = std::mem::size_of::<BITMAPV5HEADER>();
	let header = BITMAPV5HEADER {
		bV5Size: header_size as u32,
		bV5Width: image.width as LONG,
		bV5Height: image.height as LONG,
		bV5Planes: 1,
		bV5BitCount: 32,
		bV5Compression: BI_BITFIELDS,
		bV5SizeImage: (4 * image.width * image.height) as DWORD,
		bV5XPelsPerMeter: 0,
		bV5YPelsPerMeter: 0,
		bV5ClrUsed: 0,
		bV5ClrImportant: 0,
		bV5RedMask: 0x00ff0000,
		bV5GreenMask: 0x0000ff00,
		bV5BlueMask: 0x000000ff,
		bV5AlphaMask: 0xff000000,
		bV5CSType: LCS_sRGB as u32,
		bV5Endpoints: std::mem::MaybeUninit::<CIEXYZTRIPLE>::zeroed().assume_init(),
		bV5GammaRed: 0,
		bV5GammaGreen: 0,
		bV5GammaBlue: 0,
		bV5Intent: LCS_GM_IMAGES as u32, // I'm not sure about this.
		bV5ProfileData: 0,
		bV5ProfileSize: 0,
		bV5Reserved: 0,
	};

	// In theory we don't need to flip the image because we could just specify
	// a negative height in the header, which according to the documentation, indicates that the
	// image rows are in top-to-bottom order. HOWEVER: MS Word (and WordPad) cannot paste an image
	// that has a negative height in its header.
	let image = flip_v(image);

	let data_size = header_size + image.bytes.len();
	let hdata = GlobalAlloc(GHND, data_size);
	if hdata.is_null() {
		return Err(Error::Unknown {
			description: format!(
				"Could not allocate global memory object. GlobalAlloc returned null at line {}.",
				line!()
			),
		});
	}
	{
		let data_ptr = GlobalLock(hdata) as *mut u8;
		if data_ptr.is_null() {
			return Err(Error::Unknown {
				description: format!("Could not lock the global memory object at line {}", line!()),
			});
		}
		defer!({
			let retval = GlobalUnlock(hdata);
			if retval == 0 {
				let lasterr = GetLastError();
				if lasterr != 0 {
					error!("Failed calling GlobalUnlock when writing dibv5 data. Error code was 0x{:X}", lasterr);
				}
			}
		});
		copy_nonoverlapping::<u8>((&header) as *const _ as *const u8, data_ptr, header_size);

		// Not using the `add` function, because that has a restriction, that the result cannot overflow isize
		let pixels_dst = (data_ptr as usize + header_size) as *mut u8;
		copy_nonoverlapping::<u8>(image.bytes.as_ptr(), pixels_dst, image.bytes.len());

		let dst_pixels_slice = std::slice::from_raw_parts_mut(pixels_dst, image.bytes.len());
		rgba_to_win(dst_pixels_slice);
	}

	if SetClipboardData(CF_DIBV5, hdata as _).is_null() {
		DeleteObject(hdata as _);
		return Err(Error::Unknown {
			description: format!("Call to `SetClipboardData` returned NULL at line {}", line!()),
		});
	}
	Ok(())
}

#[cfg(feature = "image-data")]
fn read_cf_dibv5(dibv5: &[u8]) -> Result<ImageData<'static>, Error> {
	// The DIBV5 format is a BITMAPV5HEADER followed by the pixel data according to
	// https://docs.microsoft.com/en-us/windows/win32/dataxchg/standard-clipboard-formats

	// so first let's get a pointer to the header
	let header_size = size_of::<BITMAPV5HEADER>();
	if dibv5.len() < header_size {
		return Err(Error::Unknown {
			description: "When reading the DIBV5 data, it contained fewer bytes than the BITMAPV5HEADER size. This is invalid.".into()
		});
	}
	let header = unsafe { &*(dibv5.as_ptr() as *const BITMAPV5HEADER) };

	let has_profile =
		header.bV5CSType as i32 == PROFILE_LINKED || header.bV5CSType as i32 == PROFILE_EMBEDDED;
	let pixel_data_start;
	if has_profile {
		pixel_data_start = header.bV5ProfileData as isize + header.bV5ProfileSize as isize;
	} else {
		pixel_data_start = header_size as isize;
	}
	unsafe {
		let image_bytes = dibv5.as_ptr().offset(pixel_data_start) as *const _;
		let hdc = GetDC(std::ptr::null_mut());
		let hbitmap = CreateDIBitmap(
			hdc,
			header as *const BITMAPV5HEADER as *const _,
			CBM_INIT,
			image_bytes,
			header as *const BITMAPV5HEADER as *const _,
			DIB_RGB_COLORS,
		);
		if hbitmap.is_null() {
			return Err(Error::Unknown {
				description:
					"Failed to create the HBITMAP while reading DIBV5. CreateDIBitmap returned null"
						.into(),
			});
		}
		// Now extract the pixels in a desired format
		let w = header.bV5Width;
		let h = header.bV5Height.abs();
		let result_size = w as usize * h as usize * 4;

		let mut result_bytes = Vec::<u8>::with_capacity(result_size);

		let mut output_header = BITMAPINFO {
			bmiColors: [RGBQUAD { rgbRed: 0, rgbGreen: 0, rgbBlue: 0, rgbReserved: 0 }],
			bmiHeader: BITMAPINFOHEADER {
				biSize: size_of::<BITMAPINFOHEADER>() as u32,
				biWidth: w,
				biHeight: -h,
				biBitCount: 32,
				biPlanes: 1,
				biCompression: BI_RGB,
				biSizeImage: 0,
				biXPelsPerMeter: 0,
				biYPelsPerMeter: 0,
				biClrUsed: 0,
				biClrImportant: 0,
			},
		};

		let result = GetDIBits(
			hdc,
			hbitmap,
			0,
			h as u32,
			result_bytes.as_mut_ptr() as *mut _,
			&mut output_header as *mut _,
			DIB_RGB_COLORS,
		);
		if result == 0 {
			return Err(Error::Unknown {
				description: "Could not get the bitmap bits, GetDIBits returned 0".into(),
			});
		}
		let read_len = result as usize * w as usize * 4;
		if read_len > result_bytes.capacity() {
			panic!("Segmentation fault. Read more bytes than allocated to pixel buffer");
		}
		result_bytes.set_len(read_len);

		win_to_rgba(&mut result_bytes);

		let result =
			ImageData { bytes: result_bytes.into(), width: w as usize, height: h as usize };
		Ok(result)
	}
}

/// Converts the RGBA (u8) pixel data into the bitmap-native ARGB (u32) format in-place
///
/// Safety: the `bytes` slice must have a length that's a multiple of 4
#[cfg(feature = "image-data")]
#[allow(clippy::identity_op, clippy::erasing_op)]
unsafe fn rgba_to_win(bytes: &mut [u8]) {
	let u32pixels = std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u32, bytes.len() / 4);

	for p in u32pixels {
		let tmp = *p;
		let rgba = std::slice::from_raw_parts((&tmp) as *const u32 as *const u8, 4);
		let a = (rgba[3] as u32) << (3 * 8);
		let r = (rgba[0] as u32) << (2 * 8);
		let g = (rgba[1] as u32) << (1 * 8);
		let b = (rgba[2] as u32) << (0 * 8);

		*p = a | r | g | b;
	}
}

/// Vertically flips the image pixels in memory
#[cfg(feature = "image-data")]
fn flip_v(image: ImageData) -> ImageData<'static> {
	let w = image.width;
	let h = image.height;

	let mut bytes = image.bytes.into_owned();

	let rowsize = w * 4; // each pixel is 4 bytes
	let mut tmp_a = Vec::new();
	tmp_a.resize(rowsize, 0);
	// I believe this could be done safely with `as_chunks_mut`, but that's not stable yet
	for a_row_id in 0..(h / 2) {
		let b_row_id = h - a_row_id - 1;

		// swap rows `first_id` and `second_id`
		let a_byte_start = a_row_id * rowsize;
		let a_byte_end = a_byte_start + rowsize;
		let b_byte_start = b_row_id * rowsize;
		let b_byte_end = b_byte_start + rowsize;
		tmp_a.copy_from_slice(&bytes[a_byte_start..a_byte_end]);
		bytes.copy_within(b_byte_start..b_byte_end, a_byte_start);
		bytes[b_byte_start..b_byte_end].copy_from_slice(&tmp_a);
	}

	ImageData { width: image.width, height: image.height, bytes: bytes.into() }
}

/// Converts the ARGB (u32) pixel data into the RGBA (u8) format in-place
///
/// Safety: the `bytes` slice must have a length that's a multiple of 4
#[cfg(feature = "image-data")]
#[allow(clippy::identity_op, clippy::erasing_op)]
unsafe fn win_to_rgba(bytes: &mut [u8]) {
	let u32pixels = std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u32, bytes.len() / 4);

	for p in u32pixels {
		let mut tmp = *p;
		let bytes = std::slice::from_raw_parts_mut((&mut tmp) as *mut u32 as *mut u8, 4);
		bytes[0] = (*p >> (2 * 8)) as u8;
		bytes[1] = (*p >> (1 * 8)) as u8;
		bytes[2] = (*p >> (0 * 8)) as u8;
		bytes[3] = (*p >> (3 * 8)) as u8;
		*p = tmp;
	}
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
		defer!({
			let retval = GlobalUnlock(ptr);
			if retval == 0 {
				let lasterr = GetLastError();
				if lasterr != 0 {
					error!("Failed calling GlobalUnlock when reading string data. Error code was 0x{:X}", lasterr);
				}
			}
		});

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
	pub(crate) fn get_image(&mut self) -> Result<ImageData<'static>, Error> {
		use winapi::um::winuser::CF_DIBV5;

		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;

		let data_handle = unsafe { GetClipboardData(CF_DIBV5) as *mut winapi::ctypes::c_void };
		if data_handle.is_null() {
			return Err(Error::ContentNotAvailable);
		}
		unsafe {
			let ptr = GlobalLock(data_handle);
			if ptr.is_null() {
				return Err(Error::Unknown { description: "GlobalLock returned null".into() });
			}
			defer!({
				let retval = GlobalUnlock(data_handle);
				if retval == 0 {
					let lasterr = GetLastError();
					if lasterr != 0 {
						error!("Failed calling GlobalUnlock when reading dibv5 data. Error code was 0x{:X}", lasterr);
					}
				}
			});
			let data_size = GlobalSize(data_handle);
			let data_slice = std::slice::from_raw_parts(ptr as *const u8, data_size);

			read_cf_dibv5(data_slice)
		}
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;

		if let Err(e) = clipboard_win::raw::empty() {
			return Err(Error::Unknown {
				description: format!("Failed to empty the clipboard. Got error code: {}", e),
			});
		};

		unsafe { add_cf_dibv5(image) }
	}
}
