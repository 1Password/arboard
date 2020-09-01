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

use common::*;
use core_graphics::color_space::CGColorSpace;
use core_graphics::image::CGImage;
use core_graphics::{
	base::{kCGBitmapByteOrderDefault, kCGImageAlphaLast, kCGRenderingIntentDefault, CGFloat},
	data_provider::{CGDataProvider, CustomData},
};
use objc::runtime::{Class, Object, BOOL, NO};
use objc_foundation::{INSArray, INSObject, INSString};
use objc_foundation::{NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};
use std::error::Error;
use std::mem::transmute;

pub struct OSXClipboardContext {
	pasteboard: Id<Object>,
}

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct NSSize {
	pub width: CGFloat,
	pub height: CGFloat,
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
) -> Result<Id<NSObject>, Box<dyn Error>> {
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
	let nsimage_class = Class::get("NSImage").ok_or(err("Class::get(\"NSImage\")"))?;
	let image: Id<NSObject> = unsafe { Id::from_ptr(msg_send![nsimage_class, alloc]) };
	let () = unsafe { msg_send![image, initWithCGImage:cg_image size:size] };
	Ok(image)
}

impl ClipboardProvider for OSXClipboardContext {
	fn new() -> Result<OSXClipboardContext, Box<dyn Error>> {
		let cls = Class::get("NSPasteboard").ok_or(err("Class::get(\"NSPasteboard\")"))?;
		let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
		if pasteboard.is_null() {
			return Err(err("NSPasteboard#generalPasteboard returned null"));
		}
		let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
		Ok(OSXClipboardContext { pasteboard })
	}
	fn get_text(&mut self) -> Result<String, Box<dyn Error>> {
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
				return Err(err("pasteboard#readObjectsForClasses:options: returned null"));
			}
			Id::from_ptr(obj)
		};
		if string_array.count() == 0 {
			Err(err("pasteboard#readObjectsForClasses:options: returned empty"))
		} else {
			Ok(string_array[0].as_str().to_owned())
		}
	}
	fn set_text(&mut self, data: String) -> Result<(), Box<dyn Error>> {
		let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
		let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
		let success: bool = unsafe { msg_send![self.pasteboard, writeObjects: string_array] };
		return if success {
			Ok(())
		} else {
			Err(err("NSPasteboard#writeObjects: returned false"))
		};
	}
	fn get_binary_contents(&mut self) -> Result<Option<ClipboardContent>, Box<dyn Error>> {
		let string_class: Id<NSObject> = {
			let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
			unsafe { transmute(cls) }
		};
		let image_class: Id<NSObject> = {
			let cls: Id<Class> = unsafe { Id::from_ptr(class("NSImage")) };
			unsafe { transmute(cls) }
		};
		let url_class: Id<NSObject> = {
			let cls: Id<Class> = unsafe { Id::from_ptr(class("NSURL")) };
			unsafe { transmute(cls) }
		};
		let classes = vec![url_class, image_class, string_class];
		let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(classes);
		let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
		let contents: Id<NSArray<NSObject>> = unsafe {
			let obj: *mut NSArray<NSObject> =
				msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
			if obj.is_null() {
				return Err(err("pasteboard#readObjectsForClasses:options: returned null"));
			}
			Id::from_ptr(obj)
		};
		if contents.count() == 0 {
			Ok(None)
		} else {
			let obj = &contents[0];
			if obj.is_kind_of(Class::get("NSString").unwrap()) {
				let s: &NSString = unsafe { transmute(obj) };
				Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
			} else if obj.is_kind_of(Class::get("NSImage").unwrap()) {
				let tiff: &NSArray<NSObject> = unsafe { msg_send![obj, TIFFRepresentation] };
				let len: usize = unsafe { msg_send![tiff, length] };
				let bytes: *const u8 = unsafe { msg_send![tiff, bytes] };
				let vec = unsafe { std::slice::from_raw_parts(bytes, len) };
				// Here we copy the entire &[u8] into a new owned `Vec`
				// Is there another way that doesn't copy multiple megabytes?
				Ok(Some(ClipboardContent::Tiff(vec.into())))
			} else if obj.is_kind_of(Class::get("NSURL").unwrap()) {
				let s: &NSString = unsafe { msg_send![obj, absoluteString] };
				Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
			} else {
				// let cls: &Class = unsafe { msg_send![obj, class] };
				// println!("{}", cls.name());
				Err(err("pasteboard#readObjectsForClasses:options: returned unknown class"))
			}
		}
	}
	fn get_image(&mut self) -> Result<ImageData, Box<dyn Error>> {
		Err("Not implemented".into())
		// let image_class: Id<NSObject> = {
		//     let cls: Id<Class> = unsafe { Id::from_ptr(class("NSImage")) };
		//     unsafe { transmute(cls) }
		// };
		// let classes = vec![image_class];
		// let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(classes);
		// let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
		// let contents: Id<NSArray<NSObject>> = unsafe {
		//     let obj: *mut NSArray<NSObject> =
		//         msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];
		//     if obj.is_null() {
		//         return Err(err(
		//             "pasteboard#readObjectsForClasses:options: returned null",
		//         ));
		//     }
		//     Id::from_ptr(obj)
		// };
		// if contents.count() == 0 {
		//     Err("No content on the clipboard".into())
		// } else {
		//     let obj = &contents[0];
		//     if obj.is_kind_of(Class::get("NSString").unwrap()) {
		//         let s: &NSString = unsafe { transmute(obj) };
		//         Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
		//     } else if obj.is_kind_of(Class::get("NSImage").unwrap()) {
		//         let tiff: &NSArray<NSObject> = unsafe { msg_send![obj, TIFFRepresentation] };
		//         let len: usize = unsafe { msg_send![tiff, length] };
		//         let bytes: *const u8 = unsafe { msg_send![tiff, bytes] };
		//         let vec = unsafe { std::slice::from_raw_parts(bytes, len) };
		//         // Here we copy the entire &[u8] into a new owned `Vec`
		//         // Is there another way that doesn't copy multiple megabytes?
		//         Ok(Some(ClipboardContent::Tiff(vec.into())))
		//     } else if obj.is_kind_of(Class::get("NSURL").unwrap()) {
		//         let s: &NSString = unsafe { msg_send![obj, absoluteString] };
		//         Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
		//     } else {
		//         // let cls: &Class = unsafe { msg_send![obj, class] };
		//         // println!("{}", cls.name());
		//         Err(err(
		//             "pasteboard#readObjectsForClasses:options: returned unknown class",
		//         ))
		//     }
		//}
	}
	fn set_image(&mut self, data: ImageData) -> Result<(), Box<dyn Error>> {
		let pixels = data.bytes.into();
		let image = image_from_pixels(pixels, data.width, data.height)?;
		let objects: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![image]);
		let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
		let success: BOOL = unsafe { msg_send![self.pasteboard, writeObjects: objects] };
		if success == NO {
			return Err(
				"Failed to write the image to the pasteboard (`writeObjects` returned NO).".into(),
			);
		}
		Ok(())
	}
}

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
	unsafe { transmute(Class::get(name)) }
}
