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

const BITMAP_FILE_HEADER_SIZE: usize = 14;
const BITMAP_INFO_HEADER_SIZE: usize = 40;

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
		let bitmap_data = clipboard.get_dib()?;
		let mut header: [u8; BITMAP_FILE_HEADER_SIZE] = [0; 14];
		header[0] = b'B';
		header[1] = b'M';
		byteorder::LittleEndian::write_u32(
			&mut header[2..6],
			(bitmap_data.size() + BITMAP_FILE_HEADER_SIZE) as u32,
		);
		byteorder::LittleEndian::write_u32(
			&mut header[10..14],
			(BITMAP_INFO_HEADER_SIZE + BITMAP_FILE_HEADER_SIZE) as u32,
		);

		// TODO make a struct holding the header and the rest of the bitmap and implements Seek and Read.

		let cursor = std::io::Cursor::new(bitmap_data.as_bytes());
		let bmp_image = BmpDecoder::new(cursor)?;
		let (w, h) = bmp_image.dimensions();
		let width = w as usize;
		let height = h as usize;
		let buffer_size = width * height * 4;
		let mut bytes = Vec::with_capacity(buffer_size);
		bytes.resize(buffer_size, 0);
		bmp_image.read_image(&mut bytes)?;
		Ok(ImageData { width, height, bytes: Cow::from(bytes) })
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
