use std::{
	borrow::Cow,
	convert::TryInto,
	io::{self, Read, Seek},
	mem::{size_of, MaybeUninit},
	ptr::null_mut,
};

use log::{debug, warn};

use image::{
	bmp::{BmpDecoder, BmpEncoder},
	ColorType, ImageDecoder,
};
use scopeguard::defer;
use winapi::{
	shared::{
		minwindef::{DWORD, WORD},
		windef::HBITMAP,
	},
	um::{
		errhandlingapi::GetLastError,
		minwinbase::LPTR,
		winbase::LocalAlloc,
		wingdi::{
			CreateDIBitmap, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
			BITMAPINFOHEADER, BITMAPV4HEADER, BI_BITFIELDS, BI_RGB, CBM_INIT, CIEXYZTRIPLE,
			DIB_RGB_COLORS, PBITMAPINFO, RGBQUAD,
		},
		winnt::LONG,
		winuser::{GetDC, ReleaseDC, SetClipboardData, CF_BITMAP},
	},
};

use crate::common::{CustomItem, Error, ImageData};

const BITMAP_FILE_HEADER_SIZE: usize = 14;
//const BITMAP_INFO_HEADER_SIZE: usize = 40;
const BITMAP_V4_INFO_HEADER_SIZE: u32 = 108;

pub struct FakeBitmapFile {
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

pub struct DibImage {
	data: Vec<u8>,
	format: u32, // An be either CF_DIBV5 or CF_DIB
}
impl DibImage {
	pub fn dib_bytes(&self) -> &[u8] {
		&self.data[BITMAP_FILE_HEADER_SIZE..]
	}
	pub fn format(&self) -> u32 {
		self.format
	}
}

pub fn image_to_dib(image: &ImageData) -> Result<DibImage, Error> {
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
	Ok(DibImage { data: bmp_data, format })
}

pub unsafe fn add_cf_bitmap(image: &ImageData) -> Result<(), Error> {
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

/// It's the caller's responsibility to free the returned pointer, using `LocalFree`
///
/// Source: https://docs.microsoft.com/en-us/windows/win32/gdi/storing-an-image
pub fn create_bitmap_info_struct(hbmp: HBITMAP) -> Result<*mut BITMAPINFO, &'static str> {
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
pub fn get_image_from_dib() -> Result<ImageData<'static>, Error> {
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
pub fn convert_clipboard_bitmap(data: HBITMAP) -> Option<CustomItem<'static>> {
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
