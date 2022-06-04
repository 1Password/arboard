/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2022 The Arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use super::common::Error;
#[cfg(feature = "image-data")]
use super::common::ImageData;
#[cfg(feature = "image-data")]
use core_graphics::{
	base::{kCGBitmapByteOrderDefault, kCGImageAlphaLast, kCGRenderingIntentDefault, CGFloat},
	color_space::CGColorSpace,
	data_provider::{CGDataProvider, CustomData},
	image::CGImage,
};
use objc::runtime::{Class, Object};
#[cfg(feature = "image-data")]
use objc::runtime::{BOOL, NO};
use objc::{msg_send, sel, sel_impl};
use objc_foundation::{INSArray, INSObject, INSString};
use objc_foundation::{NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};
use std::mem::transmute;

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

/// Returns an NSImage object on success.
#[cfg(feature = "image-data")]
fn image_from_pixels(
	pixels: Vec<u8>,
	width: usize,
	height: usize,
) -> Result<Id<NSObject>, Box<dyn std::error::Error>> {
	#[repr(C)]
	#[derive(Copy, Clone)]
	struct NSSize {
		width: CGFloat,
		height: CGFloat,
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

pub(crate) struct Clipboard {
	pasteboard: Id<Object>,
}

impl Clipboard {
	pub(crate) fn new() -> Result<Clipboard, Error> {
		let cls = Class::get("NSPasteboard")
			.ok_or(Error::Unknown { description: "Class::get(\"NSPasteboard\")".into() })?;
		let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
		if pasteboard.is_null() {
			return Err(Error::Unknown {
				description: "NSPasteboard#generalPasteboard returned null".into(),
			});
		}
		let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
		Ok(Clipboard { pasteboard })
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
}

pub(crate) struct Get<'clipboard> {
	pasteboard: &'clipboard Object,
}

impl<'clipboard> Get<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { pasteboard: &*clipboard.pasteboard }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
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

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		use std::io::Cursor;

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
}

pub(crate) struct Set<'clipboard> {
	pasteboard: &'clipboard Object,
}

impl<'clipboard> Set<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { pasteboard: &*clipboard.pasteboard }
	}

	pub(crate) fn text(self, data: String) -> Result<(), Error> {
		let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
		let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
		let success: bool = unsafe { msg_send![self.pasteboard, writeObjects: string_array] };
		if success {
			Ok(())
		} else {
			Err(Error::Unknown { description: "NSPasteboard#writeObjects: returned false".into() })
		}
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, data: ImageData) -> Result<(), Error> {
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
}

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
fn class(name: &str) -> *mut Class {
	unsafe { transmute(Class::get(name)) }
}
