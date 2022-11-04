/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2022 The Arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

use crate::common::Error;
#[cfg(feature = "image-data")]
use crate::common::ImageData;
#[cfg(feature = "image-data")]
use core_graphics::{
	base::{kCGBitmapByteOrderDefault, kCGImageAlphaLast, kCGRenderingIntentDefault, CGFloat},
	color_space::CGColorSpace,
	data_provider::{CGDataProvider, CustomData},
	image::CGImage,
};
use objc::{
	msg_send,
	runtime::{Class, Object},
	sel, sel_impl,
};
use objc_foundation::{INSArray, INSObject, INSString, NSArray, NSDictionary, NSObject, NSString};
use objc_id::{Id, Owned};
use once_cell::sync::Lazy;
use std::borrow::Cow;

// Required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern "C" {
	static NSPasteboardTypeHTML: *const Object;
	static NSPasteboardTypeString: *const Object;
}

static NSSTRING_CLASS: Lazy<&Class> = Lazy::new(|| Class::get("NSString").unwrap());
#[cfg(feature = "image-data")]
static NSIMAGE_CLASS: Lazy<&Class> = Lazy::new(|| Class::get("NSImage").unwrap());

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

	#[derive(Debug)]
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
	let pixel_data: Box<Box<dyn CustomData>> = Box::new(Box::new(PixelArray { data: pixels }));
	let provider = unsafe { CGDataProvider::from_custom_data(pixel_data) };

	let cg_image = CGImage::new(
		width,
		height,
		8,
		32,
		4 * width,
		&colorspace,
		kCGBitmapByteOrderDefault | kCGImageAlphaLast,
		&provider,
		false,
		kCGRenderingIntentDefault,
	);
	let size = NSSize { width: width as CGFloat, height: height as CGFloat };
	let image: Id<NSObject> = unsafe { Id::from_ptr(msg_send![*NSIMAGE_CLASS, alloc]) };
	#[allow(clippy::let_unit_value)]
	{
		let _: () = unsafe { msg_send![image, initWithCGImage:cg_image size:size] };
	}

	Ok(image)
}

pub(crate) struct Clipboard {
	pasteboard: Id<Object>,
}

impl Clipboard {
	pub(crate) fn new() -> Result<Clipboard, Error> {
		let cls = Class::get("NSPasteboard").expect("NSPasteboard not registered");
		let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };

		if !pasteboard.is_null() {
			// SAFETY: `generalPasteboard` is not null and a valid object pointer.
			let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
			Ok(Clipboard { pasteboard })
		} else {
			// Rust only supports 10.7+, while `generalPasteboard` first appeared in 10.0, so this
			// is unreachable in "normal apps". However in some edge cases, like running under
			// launchd (in some modes) as a daemon, the clipboard object may be unavailable.
			Err(Error::ClipboardNotSupported)
		}
	}

	fn clear(&mut self) {
		let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
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
		let string_class = object_class(&NSSTRING_CLASS);
		let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
		let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();

		let string_array: Id<NSArray<NSString>> = unsafe {
			let obj: *mut NSArray<NSString> =
				msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];

			if obj.is_null() {
				return Err(Error::ContentNotAvailable);
			} else {
				Id::from_ptr(obj)
			}
		};

		string_array
			.first_object()
			.map(|obj| obj.as_str().to_owned())
			.ok_or(Error::ContentNotAvailable)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		use std::io::Cursor;

		let image_class: Id<NSObject> = object_class(&NSIMAGE_CLASS);
		let classes = vec![image_class];
		let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(classes);
		let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();

		let contents: Id<NSArray<NSObject>> = unsafe {
			let obj: *mut NSArray<NSObject> =
				msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options];

			if obj.is_null() {
				return Err(Error::ContentNotAvailable);
			} else {
				Id::from_ptr(obj)
			}
		};

		let obj = match contents.first_object() {
			Some(obj) if obj.is_kind_of(&NSIMAGE_CLASS) => obj,
			Some(_) | None => return Err(Error::ContentNotAvailable),
		};

		let tiff: &NSArray<NSObject> = unsafe { msg_send![obj, TIFFRepresentation] };
		let data = unsafe {
			let len: usize = msg_send![tiff, length];
			let bytes: *const u8 = msg_send![tiff, bytes];

			Cursor::new(std::slice::from_raw_parts(bytes, len))
		};
		let reader = image::io::Reader::with_format(data, image::ImageFormat::Tiff);
		match reader.decode() {
			Ok(img) => {
				let rgba = img.into_rgba8();
				let (width, height) = rgba.dimensions();

				Ok(ImageData {
					width: width as usize,
					height: height as usize,
					bytes: rgba.into_raw().into(),
				})
			}
			Err(_) => Err(Error::ConversionFailure),
		}
	}
}

pub(crate) struct Set<'clipboard> {
	clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Set<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn text(self, data: Cow<'_, str>) -> Result<(), Error> {
		self.clipboard.clear();

		let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
		let success: bool =
			unsafe { msg_send![self.clipboard.pasteboard, writeObjects: string_array] };
		if success {
			Ok(())
		} else {
			Err(Error::Unknown { description: "NSPasteboard#writeObjects: returned false".into() })
		}
	}

	pub(crate) fn html(self, html: Cow<'_, str>, alt: Option<Cow<'_, str>>) -> Result<(), Error> {
		self.clipboard.clear();
		// Text goes to the clipboard as UTF-8 but may be interpreted as Windows Latin 1.
		// This wrapping forces it to be interpreted as UTF-8.
		//
		// See:
		// https://bugzilla.mozilla.org/show_bug.cgi?id=466599
		// https://bugs.chromium.org/p/chromium/issues/detail?id=11957
		let html = format!(
			r#"<html>
				<head>
					<meta http-equiv="content-type" content="text/html; charset=utf-8">
				</head>
				<body>{}</body>
			</html>"#,
			html
		);
		let html_nss = NSString::from_str(&html);
		let mut success: bool = unsafe {
			msg_send![self.clipboard.pasteboard, setString: html_nss forType:NSPasteboardTypeHTML]
		};
		if success {
			if let Some(alt_text) = alt {
				let alt_nss = NSString::from_str(&alt_text);
				success = unsafe {
					msg_send![self.clipboard.pasteboard, setString: alt_nss forType:NSPasteboardTypeString]
				};
			}
		}
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

		self.clipboard.clear();

		let objects: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![image]);
		let success: bool = unsafe { msg_send![self.clipboard.pasteboard, writeObjects: objects] };
		if success {
			Ok(())
		} else {
			Err(Error::Unknown {
				description:
					"Failed to write the image to the pasteboard (`writeObjects` returned NO)."
						.into(),
			})
		}
	}
}

pub(crate) struct Clear<'clipboard> {
	clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Clear<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn clear(self) -> Result<(), Error> {
		self.clipboard.clear();
		Ok(())
	}
}

/// Convenience function to get an Objective-C object from a
/// specific class.
fn object_class(class: &'static Class) -> Id<NSObject> {
	// SAFETY: `Class` is a valid object and `Id` will not mutate it
	unsafe { Id::from_ptr(class as *const Class as *mut NSObject) }
}
