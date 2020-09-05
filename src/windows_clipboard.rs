/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use byteorder::ByteOrder;
use clipboard_win::{
	formats::{CF_DIB, CF_DIBV5}, get_clipboard_string, set_clipboard_string, Clipboard as SystemClipboard,
};
use common::{ImageData};
use image::{
	bmp::{BmpEncoder, BmpDecoder},
	ColorType, ImageDecoder,
};
use std::borrow::Cow;
use std::error::Error;
use std::io::{self, Read, Seek};

const BITMAP_FILE_HEADER_SIZE: usize = 14;
const BITMAP_INFO_HEADER_SIZE: usize = 40;
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
				buf[buf_pos..buf_end]
					.copy_from_slice(&self.bitmap[bitmap_start..bitmap_end]);
				self.curr_pos += copy_len;
			}
		}
		Ok(total_read_len)
	}
}

macro_rules! clip_try {
	($e:expr) => {
		{$e}.map_err(|e| e.to_string())?
	};
}

pub struct WindowsClipboardContext;

impl WindowsClipboardContext {
	pub(crate) fn new() -> Result<Self, Box<dyn Error>> {
		Ok(WindowsClipboardContext)
	}
	pub(crate) fn get_text(&mut self) -> Result<String, Box<dyn Error>> {
		Ok(get_clipboard_string().map_err(|e| e.to_string())?)
	}
	pub(crate) fn set_text(&mut self, data: String) -> Result<(), Box<dyn Error>> {
		Ok(set_clipboard_string(&data).map_err(|e| e.to_string())?)
	}
	pub(crate) fn get_image(&mut self) -> Result<ImageData, Box<dyn Error>> {
		clip_try!(clipboard_win::raw::open());
		// Note: the logic needs te be wrapped into a function because
		// this raw clipboard API is not RAII (which it could be in my opinion).
		// So instead we catch errors inside this funcrion
		fn do_your_thing() -> Result<ImageData<'static>, Box<dyn Error>> {
			let format = clipboard_win::formats::CF_DIB;
			let size;
			match clipboard_win::raw::size(format) {
				Some(s) => size = s,
				None => return Err("Could not get image ".into()),
			}
			let mut data = vec![0u8; size.into()];
			clip_try!(clipboard_win::raw::get(format, &mut data));
			let info_header_size = byteorder::LittleEndian::read_u32(&data);
			let mut fake_bitmap_file = FakeBitmapFile {
				bitmap: data,
				file_header: [0; BITMAP_FILE_HEADER_SIZE],
				curr_pos: 0,
			};
			fake_bitmap_file.file_header[0] = b'B';
			fake_bitmap_file.file_header[1] = b'M';
			byteorder::LittleEndian::write_u32(
				&mut fake_bitmap_file.file_header[2..6],
				(fake_bitmap_file.bitmap.len() + BITMAP_FILE_HEADER_SIZE) as u32,
			);
			byteorder::LittleEndian::write_u32(
				&mut fake_bitmap_file.file_header[10..14],
				info_header_size + BITMAP_FILE_HEADER_SIZE as u32,
			);

			let bmp_decoder = BmpDecoder::new(fake_bitmap_file).unwrap();
			let (w, h) = bmp_decoder.dimensions();
			let width = w as usize;
			let height = h as usize;
			let image = image::DynamicImage::from_decoder(bmp_decoder)?;
			Ok(ImageData { width, height, bytes: Cow::from(image.into_rgba().into_raw()) })
		}
		let result = do_your_thing();
		// We don't care if closing failed. There wouldn't be anything meaningful to do
		// with that information anyways
		let _ = clipboard_win::raw::close();
		result
	}
	pub(crate) fn set_image(&mut self, image: ImageData) -> Result<(), Box<dyn Error>> {
		//let clipboard = SystemClipboard::new()?;
		let mut bmp_data = Vec::with_capacity(image.bytes.len());
		let mut cursor = std::io::Cursor::new(&mut bmp_data);
		let mut encoder = BmpEncoder::new(&mut cursor);
		encoder.encode(&image.bytes, image.width as u32, image.height as u32, ColorType::Rgba8)?;

		let data_without_file_header = &bmp_data[BITMAP_FILE_HEADER_SIZE..];

		let header_size = byteorder::LittleEndian::read_u32(data_without_file_header);
		let format = if header_size > BITMAP_V4_INFO_HEADER_SIZE {
			clipboard_win::formats::CF_DIBV5
		} else {
			clipboard_win::formats::CF_DIB
		};

		let mut success = false;
		//clipboard_win::set_clipboard(clipboard_win::formats::Bitmap, data_without_file_header).map_err(|e| e.to_string())?;
		clip_try!(clipboard_win::with_clipboard(|| {
			success = clipboard_win::raw::set(format, data_without_file_header).is_ok();
		}));
		if !success {
			return Err("Could not set image for the clipboard.".into());
		}
		Ok(())
	}
}
