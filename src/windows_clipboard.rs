/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

#[cfg(feature = "image-data")]
use std::mem::size_of;

use clipboard_win::Clipboard as SystemClipboard;

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
			CreateDIBitmap, DeleteObject, GetDIBits, LCS_sRGB, BITMAPINFO, BITMAPINFOHEADER,
			BITMAPV4HEADER, BITMAPV5HEADER, BI_BITFIELDS, BI_RGB, CBM_INIT, CIEXYZTRIPLE,
			DIB_RGB_COLORS, LCS_GM_IMAGES, PROFILE_EMBEDDED, PROFILE_LINKED, RGBQUAD,
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
unsafe fn add_cf_dibv5(image: &ImageData) -> Result<(), Error> {
	use std::intrinsics::copy_nonoverlapping;
	use winapi::um::{
		winbase::{GlobalAlloc, GHND},
		winuser::CF_DIBV5,
	};

	let header_size = std::mem::size_of::<BITMAPV5HEADER>();
	let header = BITMAPV5HEADER {
		bV5Size: header_size as u32,
		bV5Width: image.width as LONG,
		bV5Height: -(image.height as LONG),
		bV5Planes: 1,
		bV5BitCount: 32,
		bV5Compression: BI_RGB,
		bV5SizeImage: (4 * image.width * image.height) as DWORD,
		bV5XPelsPerMeter: 3000,
		bV5YPelsPerMeter: 3000,
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
		let data_ptr = GlobalLock(hdata);
		if data_ptr.is_null() {
			return Err(Error::Unknown {
				description: format!("Could not lock the global memory object at line {}", line!()),
			});
		}
		defer!(GlobalUnlock(hdata););
		copy_nonoverlapping(&header as *const _ as *const _, data_ptr, header_size);

		let pixels_dst = data_ptr.add(header_size);
		copy_nonoverlapping(image.bytes.as_ptr() as *const _, pixels_dst, image.bytes.len());

		let dst_pixels_slice =
			std::slice::from_raw_parts_mut(pixels_dst as *mut u8, image.bytes.len());
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
		bV4RedMask: u32::from_le(0x000000ff),
		bV4GreenMask: u32::from_le(0x0000ff00),
		bV4BlueMask: u32::from_le(0x00ff0000),
		bV4AlphaMask: u32::from_le(0xff000000),
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
		use winapi::um::winuser::CF_DIBV5;

		let _cb = SystemClipboard::new_attempts(MAX_OPEN_ATTEMPTS)
			.map_err(|_| Error::ClipboardOccupied)?;

		let data_handle = unsafe { GetClipboardData(CF_DIBV5) as *mut winapi::ctypes::c_void };
		if data_handle.is_null() {
			return Err(Error::Unknown { description: "GetClipboardData returned null".into() });
		}
		unsafe {
			let ptr = GlobalLock(data_handle);
			if ptr.is_null() {
				return Err(Error::Unknown { description: "GlobalLock returned null".into() });
			}
			defer!( GlobalUnlock(data_handle); );
			let data_size = GlobalSize(data_handle);
			let data_slice = std::slice::from_raw_parts(ptr as *const u8, data_size);
			read_cf_dibv5(data_slice)
		}
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		let mut result: Result<(), Error> = Ok(());
		//let mut success = false;
		clipboard_win::with_clipboard(|| {
			// let dib_result: Result<(), String> = Ok(());
			let dib_result = unsafe { add_cf_dibv5(&image) };
			// let bitmap_result: Result<(), String> = Ok(());
			let bitmap_result = unsafe { add_cf_bitmap(&image) };
			if let (Err(dib_err), Err(bitmap_err)) = (dib_result, bitmap_result) {
				result = Err(Error::Unknown {
					description: format!("Could not set the image for the clipboard in neither of `CF_DIBV5` and `CF_BITMAP` formats. The errors were:\n`CF_DIBV5`: {}\n`CF_BITMAP`: {}", dib_err, bitmap_err),
				});
			}
		})
		.map_err(|_| Error::ClipboardOccupied)?;

		result
	}
}
