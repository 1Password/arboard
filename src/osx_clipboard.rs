/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use log::debug;
use objc::{
	class, msg_send,
	runtime::{Class, Object, BOOL, NO, YES},
	sel, sel_impl,
};
use objc_id::{Id, Owned};
use std::{borrow::Cow, collections::HashSet, ffi::c_void, io::Cursor, mem::transmute, slice};

use objc_foundation::{INSArray, INSObject, INSString, NSArray, NSDictionary, NSObject, NSString};

use core_graphics::color_space::CGColorSpace;
use core_graphics::image::CGImage;
use core_graphics::{
	base::{kCGBitmapByteOrderDefault, kCGImageAlphaLast, kCGRenderingIntentDefault, CGFloat},
	data_provider::{CGDataProvider, CustomData},
};
use util::{nil, ns_string_to_rust};

//use cocoa::appkit::NSPasteboardItem;

use crate::common::line_endings_to_crlf;

use super::common::{CustomItem, Error, ImageData};

mod util;
use self::util::{pasteboard_type_to_mime, NSSize};

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
#[allow(unused)]
extern "C" {
	// From: the `cocoa` crate:
	// Types for Standard Data - OS X v10.6 and later. (NSString *const)
	pub(crate) static NSPasteboardTypeString: *const Object;
	pub(crate) static NSPasteboardTypePDF: *const Object;
	pub(crate) static NSPasteboardTypeTIFF: *const Object;
	pub(crate) static NSPasteboardTypePNG: *const Object;
	pub(crate) static NSPasteboardTypeRTF: *const Object;
	pub(crate) static NSPasteboardTypeRTFD: *const Object;
	pub(crate) static NSPasteboardTypeHTML: *const Object;
	pub(crate) static NSPasteboardTypeTabularText: *const Object;
	pub(crate) static NSPasteboardTypeFont: *const Object;
	pub(crate) static NSPasteboardTypeRuler: *const Object;
	pub(crate) static NSPasteboardTypeColor: *const Object;
	pub(crate) static NSPasteboardTypeSound: *const Object;
	pub(crate) static NSPasteboardTypeMultipleTextSelection: *const Object;
	pub(crate) static NSPasteboardTypeFindPanelSearchOptions: *const Object;
}

#[derive(Debug, Clone)]
struct PixelArray {
	data: Vec<u8>,
}

impl CustomData for PixelArray {
	unsafe fn ptr(&self) -> *const u8 {
		self.data.as_ptr()
	}
	unsafe fn len(&self) -> usize {
		self.data.len()
	}
}

/// Returns an NSImage object on success.
fn image_from_pixels(
	pixels: Vec<u8>,
	width: usize,
	height: usize,
) -> Result<Id<NSObject>, Box<dyn std::error::Error>> {
	let colorspace = CGColorSpace::create_device_rgb();
	let bitmap_info: u32 = kCGBitmapByteOrderDefault | kCGImageAlphaLast;
	let pixel_data: Box<Box<dyn CustomData>> = Box::new(Box::new(PixelArray { data: pixels }));
	let provider = unsafe { CGDataProvider::from_custom_data(pixel_data) };
	let rendering_intent = kCGRenderingIntentDefault;
	let cg_image = CGImage::new(
		width,
		height,
		8,
		32,
		4 * width,
		&colorspace,
		bitmap_info,
		&provider,
		false,
		rendering_intent,
	);
	let size = NSSize { width: width as CGFloat, height: height as CGFloat };
	let nsimage_class = Class::get("NSImage").ok_or("Class::get(\"NSImage\")")?;
	let image: Id<NSObject> = unsafe { Id::from_ptr(msg_send![nsimage_class, alloc]) };
	let () = unsafe { msg_send![image, initWithCGImage:cg_image size:size] };
	Ok(image)
}

pub struct OSXClipboardContext {
	pasteboard: Id<Object>,
}

impl OSXClipboardContext {
	pub(crate) fn new() -> Result<OSXClipboardContext, Error> {
		let cls = Class::get("NSPasteboard")
			.ok_or(Error::Unknown { description: "Class::get(\"NSPasteboard\")".into() })?;
		let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
		if pasteboard.is_null() {
			return Err(Error::Unknown {
				description: "NSPasteboard#generalPasteboard returned null".into(),
			});
		}
		let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
		Ok(OSXClipboardContext { pasteboard })
	}
	pub(crate) fn get_text(&mut self) -> Result<String, Error> {
		let string_class: Id<NSObject> = {
			let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
			unsafe { transmute(cls) }
		};
		let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
		let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
		let string_array: Id<NSArray<NSString>> = unsafe {
			let obj: *mut NSArray<NSString> =
				msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
			if obj.is_null() {
				//return Err("pasteboard#readObjectsForClasses:options: returned null".into());
				return Err(Error::ContentNotAvailable);
			}
			Id::from_ptr(obj)
		};
		if string_array.count() == 0 {
			//Err("pasteboard#readObjectsForClasses:options: returned empty".into())
			Err(Error::ContentNotAvailable)
		} else {
			Ok(string_array[0].as_str().to_owned())
		}
	}
	pub(crate) fn set_text(&mut self, data: String) -> Result<(), Error> {
		let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
		let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
		let success: bool = unsafe { msg_send![self.pasteboard, writeObjects: string_array] };
		if success {
			Ok(())
		} else {
			Err(Error::Unknown { description: "NSPasteboard#writeObjects: returned false".into() })
		}
	}
	// fn get_binary_contents(&mut self) -> Result<Option<ClipboardContent>, Box<dyn std::error::Error>> {
	// 	let string_class: Id<NSObject> = {
	// 		let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
	// 		unsafe { transmute(cls) }
	// 	};
	// 	let image_class: Id<NSObject> = {
	// 		let cls: Id<Class> = unsafe { Id::from_ptr(class("NSImage")) };
	// 		unsafe { transmute(cls) }
	// 	};
	// 	let url_class: Id<NSObject> = {
	// 		let cls: Id<Class> = unsafe { Id::from_ptr(class("NSURL")) };
	// 		unsafe { transmute(cls) }
	// 	};
	// 	let classes = vec![url_class, image_class, string_class];
	// 	let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(classes);
	// 	let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
	// 	let contents: Id<NSArray<NSObject>> = unsafe {
	// 		let obj: *mut NSArray<NSObject> =
	// 			msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
	// 		if obj.is_null() {
	// 			return Err(err("pasteboard#readObjectsForClasses:options: returned null"));
	// 		}
	// 		Id::from_ptr(obj)
	// 	};
	// 	if contents.count() == 0 {
	// 		Ok(None)
	// 	} else {
	// 		let obj = &contents[0];
	// 		if obj.is_kind_of(Class::get("NSString").unwrap()) {
	// 			let s: &NSString = unsafe { transmute(obj) };
	// 			Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
	// 		} else if obj.is_kind_of(Class::get("NSImage").unwrap()) {
	// 			let tiff: &NSArray<NSObject> = unsafe { msg_send![obj, TIFFRepresentation] };
	// 			let len: usize = unsafe { msg_send![tiff, length] };
	// 			let bytes: *const u8 = unsafe { msg_send![tiff, bytes] };
	// 			let vec = unsafe { std::slice::from_raw_parts(bytes, len) };
	// 			// Here we copy the entire &[u8] into a new owned `Vec`
	// 			// Is there another way that doesn't copy multiple megabytes?
	// 			Ok(Some(ClipboardContent::Tiff(vec.into())))
	// 		} else if obj.is_kind_of(Class::get("NSURL").unwrap()) {
	// 			let s: &NSString = unsafe { msg_send![obj, absoluteString] };
	// 			Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
	// 		} else {
	// 			// let cls: &Class = unsafe { msg_send![obj, class] };
	// 			// println!("{}", cls.name());
	// 			Err(err("pasteboard#readObjectsForClasses:options: returned unknown class"))
	// 		}
	// 	}
	// }
	pub(crate) fn get_image(&mut self) -> Result<ImageData, Error> {
		let image_class: Id<NSObject> = {
			let cls: Id<Class> = unsafe { Id::from_ptr(class("NSImage")) };
			unsafe { transmute(cls) }
		};
		let classes = vec![image_class];
		let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(classes);
		let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
		let contents: Id<NSArray<NSObject>> = unsafe {
			let obj: *mut NSArray<NSObject> =
				msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
			if obj.is_null() {
				return Err(Error::ContentNotAvailable);
			}
			Id::from_ptr(obj)
		};
		let result;
		if contents.count() == 0 {
			result = Err(Error::ContentNotAvailable);
		} else {
			let obj = &contents[0];
			if obj.is_kind_of(Class::get("NSImage").unwrap()) {
				let tiff: &NSArray<NSObject> = unsafe { msg_send![obj, TIFFRepresentation] };
				let len: usize = unsafe { msg_send![tiff, length] };
				let bytes: *const u8 = unsafe { msg_send![tiff, bytes] };
				let slice = unsafe { std::slice::from_raw_parts(bytes, len) };
				let data_cursor = Cursor::new(slice);
				let reader = image::io::Reader::with_format(data_cursor, image::ImageFormat::Tiff);
				let width;
				let height;
				let pixels;
				match reader.decode() {
					Ok(img) => {
						let rgba = img.into_rgba8();
						let (w, h) = rgba.dimensions();
						width = w;
						height = h;
						pixels = rgba.into_raw();
					}
					Err(_) => return Err(Error::ConversionFailure),
				};
				let data = ImageData {
					width: width as usize,
					height: height as usize,
					bytes: pixels.into(),
				};
				result = Ok(data);
			} else {
				// let cls: &Class = unsafe { msg_send![obj, class] };
				// println!("{}", cls.name());
				result = Err(Error::ContentNotAvailable);
			}
		}
		result
	}
	pub(crate) fn set_image(&mut self, data: ImageData) -> Result<(), Error> {
		let pixels = data.bytes.into();
		let image = image_from_pixels(pixels, data.width, data.height)
			.map_err(|_| Error::ConversionFailure)?;
		let objects: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![image]);
		let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
		let success: BOOL = unsafe { msg_send![self.pasteboard, writeObjects: objects] };
		if success == NO {
			return Err(Error::Unknown {
				description:
					"Failed to write the image to the pasteboard (`writeObjects` returned NO)."
						.into(),
			});
		}
		Ok(())
	}

	pub(crate) fn set_custom(&mut self, items: &[CustomItem]) -> Result<(), Error> {
		// let item_iter = items.into_iter();

		//let arr_cls = class!(NSArray);
		// An `NSArray` of `NSString`s
		//let new_types: *mut Object = msg_send![arr_cls, ]
		let mut item_types = Vec::with_capacity(items.len());
		for item in items.iter() {
			let uti = util::item_to_pasteboard_type(item);
			item_types.push(uti);
		}
		let types_arr: Id<Object> = unsafe {
			let cls = class!(NSArray);
			let obj: *mut Object = msg_send![
				cls,
				arrayWithObjects:item_types.as_ptr()
				count:item_types.len()
			];
			Id::from_retained_ptr(obj)
		};
		// Note `declareTypes` calls `clearContents`
		let _: usize = unsafe { msg_send![self.pasteboard, declareTypes:types_arr owner:nil] };
		for item in items.into_iter() {
			self.set_item_for_format(item)?;
		}
		Ok(())
	}

	pub fn get_all(&mut self) -> Result<Vec<CustomItem<'static>>, Error> {
		// `NSArray` of `NSPasteboardType`s (aka `NSString`)
		let types: *const Object = unsafe { msg_send![self.pasteboard, types] };
		let type_count: usize = unsafe { msg_send![types, count] };
		// This is just in case multiple pasteboard types map to the same media type
		// for example "text/xml" and "application/xml"
		let mut media_types = HashSet::<&'static str>::new();
		let mut result = Vec::with_capacity(type_count);
		for i in 0..type_count {
			let pb_type: *const Object = unsafe { msg_send![types, objectAtIndex: i] };
			debug!("Pasteboard type: '{}'", unsafe { ns_string_to_rust(pb_type) });
			let mime = unsafe { pasteboard_type_to_mime(pb_type) };
			if media_types.contains(mime.as_str()) {
				continue;
			}
			if CustomItem::is_supported_text_type(&mime) {
				let text: *const Object =
					unsafe { msg_send![self.pasteboard, stringForType: pb_type] };
				let data = unsafe { ns_string_to_rust(text) };
				let data: Cow<'static, _> = line_endings_to_crlf(&data).to_string().into();
				if let Some(item) = CustomItem::from_text_media_type(data, &mime) {
					media_types.insert(item.media_type());
					result.push(item);
				}
			} else if CustomItem::is_supported_octet_type(&mime) {
				let ns_data: *const Object =
					unsafe { msg_send![self.pasteboard, dataForType: pb_type] };
				let len: usize = unsafe { msg_send![ns_data, length] };
				let bytes: *const c_void = unsafe { msg_send![ns_data, bytes] };
				let slice = unsafe { slice::from_raw_parts(bytes as *const u8, len) };
				let data: Cow<'static, _> = slice.to_vec().into();
				if let Some(item) = CustomItem::from_octet_media_type(data, &mime) {
					media_types.insert(item.media_type());
					result.push(item);
				}
			}
		}
		Ok(result)
	}

	fn set_item_for_format(&self, item: &CustomItem) -> Result<(), Error> {
		match item {
			CustomItem::TextPlain(text) => {
				// There's a native format for this (NSPasteboardTypeString), so we convert line
				// endings to platform-native
				let as_lf = line_endings_to_lf(text);
				self.set_string_as_raw(item, as_lf.as_ref())
			}
			CustomItem::TextUriList(text) => self.set_string_as_crlf(item, text),
			CustomItem::TextCsv(text) => self.set_string_as_crlf(item, text),
			CustomItem::TextHtml(text) => {
				// There's a native format for this (NSPasteboardTypeHTML), so we convert line
				// endings to platform-native
				let as_lf = line_endings_to_lf(text);
				self.set_string_as_raw(item, as_lf.as_ref())
			}
			CustomItem::TextXml(text) => self.set_string_as_crlf(item, text),
			CustomItem::ImageSvg(text) => self.set_string_as_raw(item, text),
			CustomItem::ApplicationJson(text) => self.set_string_as_raw(item, text),
			_ => Err(Error::ConversionFailure),
		}
	}

	fn set_string_as_crlf(&self, item: &CustomItem, string: &str) -> Result<(), Error> {
		let string = line_endings_to_crlf(string);
		self.set_string_as_raw(item, string.as_ref())
	}
	fn set_string_as_raw(&self, item: &CustomItem, string: &str) -> Result<(), Error> {
		let uti = util::item_to_pasteboard_type(item);
		debug!("Setting string for UTI: '{}'", unsafe { ns_string_to_rust(uti) });
		let ns_str = NSString::from_str(&string);
		let success: BOOL = unsafe { msg_send![self.pasteboard, setString:ns_str forType: uti] };
		if success == YES {
			Ok(())
		} else {
			Err(Error::Unknown {
				description: "Failed setting text format. (`setString:forType:` returned `NO`)"
					.into(),
			})
		}
	}
}

fn line_endings_to_lf<'a>(text: &'a str) -> Cow<'a, str> {
	// TODO a custom implementation, similar to that of `line_endings_to_crlf` would be more
	// efficient and require less memory
	let as_crlf = line_endings_to_crlf(text);
	as_crlf.replace("\r\n", "\n").into()
}

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
	unsafe { transmute(Class::get(name)) }
}
