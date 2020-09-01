/*
Copyright 2016 Avraham Weinstock

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

use byteorder::ByteOrder;
use clipboard_win::{
	formats::CF_DIB, get_clipboard_string, set_clipboard_string, Clipboard as SystemClipboard,
};
use common::{ClipboardContent, ClipboardProvider, ImageData};
use image::{
	bmp::{BMPEncoder, BmpDecoder},
	ColorType, ImageDecoder,
};
use std::borrow::Cow;
use std::error::Error;
use std::io::{self, Read, Seek};

const BITMAP_FILE_HEADER_SIZE: usize = 14;
const BITMAP_INFO_HEADER_SIZE: usize = 40;

struct FakeBitmapFile {
	file_header: [u8; BITMAP_FILE_HEADER_SIZE],
	bitmap: clipboard_win::dib::Image,

	curr_pos: usize,
}

impl FakeBitmapFile {
	fn len(&self) -> usize {
		self.file_header.len() + self.bitmap.size()
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
			if bitmap_start < self.bitmap.size() {
				let copy_len = (self.bitmap.size() - bitmap_start).min(remaining_read_len);
				let bitmap_end = bitmap_start + copy_len;
				let buf_end = buf_pos + copy_len;
				buf[buf_pos..buf_end]
					.copy_from_slice(&self.bitmap.as_bytes()[bitmap_start..bitmap_end]);
				self.curr_pos += copy_len;
			}
		}
		Ok(total_read_len)
	}
}

pub struct WindowsClipboardContext;

impl ClipboardProvider for WindowsClipboardContext {
	fn new() -> Result<Self, Box<dyn Error>> {
		Ok(WindowsClipboardContext)
	}
	fn get_text(&mut self) -> Result<String, Box<dyn Error>> {
		Ok(get_clipboard_string()?)
	}
	fn set_text(&mut self, data: String) -> Result<(), Box<dyn Error>> {
		Ok(set_clipboard_string(&data)?)
	}
	fn get_binary_contents(&mut self) -> Result<Option<ClipboardContent>, Box<dyn Error>> {
		Err("get_binary_contents is not yet implemented for windows.".into())
	}
	fn get_image(&mut self) -> Result<ImageData, Box<dyn Error>> {
		let clipboard = SystemClipboard::new()?;
		let mut fake_bitmap_file = FakeBitmapFile {
			bitmap: clipboard.get_dib().unwrap(),
			file_header: [0; 14],
			curr_pos: 0,
		};
		fake_bitmap_file.file_header[0] = b'B';
		fake_bitmap_file.file_header[1] = b'M';
		byteorder::LittleEndian::write_u32(
			&mut fake_bitmap_file.file_header[2..6],
			(fake_bitmap_file.bitmap.size() + BITMAP_FILE_HEADER_SIZE) as u32,
		);
		byteorder::LittleEndian::write_u32(
			&mut fake_bitmap_file.file_header[10..14],
			(BITMAP_INFO_HEADER_SIZE + BITMAP_FILE_HEADER_SIZE) as u32,
		);

		let bmp_decoder = BmpDecoder::new(fake_bitmap_file).unwrap();
		let (w, h) = bmp_decoder.dimensions();
		let width = w as usize;
		let height = h as usize;
		let image = image::DynamicImage::from_decoder(bmp_decoder)?;
		Ok(ImageData { width, height, bytes: Cow::from(image.into_rgba().into_raw()) })
	}
	fn set_image(&mut self, image: ImageData) -> Result<(), Box<dyn Error>> {
		let clipboard = SystemClipboard::new()?;
		let mut bmp_data = Vec::with_capacity(image.bytes.len());
		let mut cursor = std::io::Cursor::new(&mut bmp_data);
		let mut encoder = BMPEncoder::new(&mut cursor);
		encoder.encode(&image.bytes, image.width as u32, image.height as u32, ColorType::Rgba8)?;

		let data_without_file_header = &bmp_data[BITMAP_FILE_HEADER_SIZE..];
		clipboard.set(CF_DIB, data_without_file_header)?;
		Ok(())
	}
}
