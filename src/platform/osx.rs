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
use objc2::{
	msg_send_id,
	rc::{autoreleasepool, Id},
	runtime::ProtocolObject,
	ClassType,
};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeString};
use objc2_foundation::{NSArray, NSString};
use std::{
	borrow::Cow,
	panic::{RefUnwindSafe, UnwindSafe},
};

/// Returns an NSImage object on success.
#[cfg(feature = "image-data")]
fn image_from_pixels(
	pixels: Vec<u8>,
	width: usize,
	height: usize,
) -> Result<Id<objc2_app_kit::NSImage>, Box<dyn std::error::Error>> {
	use core_graphics::{
		base::{kCGBitmapByteOrderDefault, kCGImageAlphaLast, kCGRenderingIntentDefault, CGFloat},
		color_space::CGColorSpace,
		data_provider::{CGDataProvider, CustomData},
		image::{CGImage, CGImageRef},
	};
	use objc2_app_kit::NSImage;
	use objc2_foundation::NSSize;
	use std::ffi::c_void;

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

	// Convert the owned `CGImage` into a reference `&CGImageRef`, and pass
	// that as `*const c_void`, since `CGImageRef` does not implement
	// `RefEncode`.
	let cg_image: *const CGImageRef = &*cg_image;
	let cg_image: *const c_void = cg_image.cast();

	let size = NSSize { width: width as CGFloat, height: height as CGFloat };
	// XXX: Use `NSImage::initWithCGImage_size` once `objc2-app-kit` supports
	// CoreGraphics.
	let image: Id<NSImage> =
		unsafe { msg_send_id![NSImage::alloc(), initWithCGImage: cg_image, size:size] };

	Ok(image)
}

pub(crate) struct Clipboard {
	pasteboard: Id<NSPasteboard>,
}

unsafe impl Send for Clipboard {}
unsafe impl Sync for Clipboard {}
impl UnwindSafe for Clipboard {}
impl RefUnwindSafe for Clipboard {}

impl Clipboard {
	pub(crate) fn new() -> Result<Clipboard, Error> {
		// Rust only supports 10.7+, while `generalPasteboard` first appeared
		// in 10.0, so this should always be available.
		//
		// However, in some edge cases, like running under launchd (in some
		// modes) as a daemon, the clipboard object may be unavailable, and
		// then `generalPasteboard` will return NULL even though it's
		// documented not to.
		//
		// Otherwise we'd just use `NSPasteboard::generalPasteboard()` here.
		let pasteboard: Option<Id<NSPasteboard>> =
			unsafe { msg_send_id![NSPasteboard::class(), generalPasteboard] };

		if let Some(pasteboard) = pasteboard {
			Ok(Clipboard { pasteboard })
		} else {
			Err(Error::ClipboardNotSupported)
		}
	}

	fn clear(&mut self) {
		unsafe { self.pasteboard.clearContents() };
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
	clipboard: &'clipboard Clipboard,
}

impl<'clipboard> Get<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
		// XXX: There does not appear to be an alternative for obtaining text without the need for
		// autorelease behavior.
		autoreleasepool(|_| {
			// XXX: We explicitly use `pasteboardItems` and not `stringForType` since the latter will concat
			// multiple strings, if present, into one and return it instead of reading just the first which is `arboard`'s
			// historical behavior.
			let contents =
				unsafe { self.clipboard.pasteboard.pasteboardItems() }.ok_or_else(|| {
					Error::Unknown {
						description: String::from("NSPasteboard#pasteboardItems errored"),
					}
				})?;

			for item in contents {
				if let Some(string) = unsafe { item.stringForType(NSPasteboardTypeString) } {
					return Ok(string.to_string());
				}
			}

			Err(Error::ContentNotAvailable)
		})
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		use objc2_app_kit::NSPasteboardTypeTIFF;
		use std::io::Cursor;

		// XXX: There does not appear to be an alternative for obtaining images without the need for
		// autorelease behavior.
		let image = autoreleasepool(|_| {
			let image_data = unsafe { self.clipboard.pasteboard.dataForType(NSPasteboardTypeTIFF) }
				.ok_or(Error::ContentNotAvailable)?;

			let data = Cursor::new(image_data.bytes());

			let reader = image::io::Reader::with_format(data, image::ImageFormat::Tiff);
			reader.decode().map_err(|_| Error::ConversionFailure)
		})?;

		let rgba = image.into_rgba8();
		let (width, height) = rgba.dimensions();

		Ok(ImageData {
			width: width as usize,
			height: height as usize,
			bytes: rgba.into_raw().into(),
		})
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

		let string_array =
			NSArray::from_vec(vec![ProtocolObject::from_id(NSString::from_str(&data))]);
		let success = unsafe { self.clipboard.pasteboard.writeObjects(&string_array) };
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
			r#"<html><head><meta http-equiv="content-type" content="text/html; charset=utf-8"></head><body>{html}</body></html>"#,
		);
		let html_nss = NSString::from_str(&html);
		// Make sure that we pass a pointer to the string and not the object itself.
		let mut success =
			unsafe { self.clipboard.pasteboard.setString_forType(&html_nss, NSPasteboardTypeHTML) };
		if success {
			if let Some(alt_text) = alt {
				let alt_nss = NSString::from_str(&alt_text);
				// Similar to the primary string, we only want a pointer here too.
				success = unsafe {
					self.clipboard.pasteboard.setString_forType(&alt_nss, NSPasteboardTypeString)
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

		let image_array = NSArray::from_vec(vec![ProtocolObject::from_id(image)]);
		let success = unsafe { self.clipboard.pasteboard.writeObjects(&image_array) };
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
