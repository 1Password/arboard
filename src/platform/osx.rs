/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2022 The Arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

#[cfg(feature = "image-data")]
use crate::common::ImageData;
use crate::common::{private, Error};
#[cfg(feature = "image-data")]
use objc2::AllocAnyThread;
use objc2::{
	msg_send,
	rc::{autoreleasepool, Retained},
	runtime::ProtocolObject,
	ClassType,
};
#[cfg(feature = "image-data")]
use objc2_app_kit::NSImage;
use objc2_app_kit::{
	NSPasteboard, NSPasteboardTypeHTML, NSPasteboardTypeString,
	NSPasteboardURLReadingFileURLsOnlyKey,
};
#[cfg(feature = "image-data")]
use objc2_core_foundation::{CGFloat, CGPoint, CGRect};
#[cfg(feature = "image-data")]
use objc2_core_graphics::{
	CGBitmapContextCreate, CGBitmapInfo, CGColorRenderingIntent, CGColorSpaceCreateDeviceRGB,
	CGContextDrawImage, CGContextFlush, CGDataProviderCreateWithData, CGImageAlphaInfo,
	CGImageCreate, CGImageGetHeight, CGImageGetWidth,
};
#[cfg(feature = "image-data")]
use objc2_foundation::NSSize;
use objc2_foundation::{ns_string, NSArray, NSDictionary, NSNumber, NSString, NSURL};
use std::{
	borrow::Cow,
	panic::{RefUnwindSafe, UnwindSafe},
	path::{Path, PathBuf},
};
#[cfg(feature = "image-data")]
use std::{
	ffi::c_void,
	ptr::{self, NonNull},
};

/// Returns an NSImage object on success.
#[cfg(feature = "image-data")]
fn image_from_pixels(pixels: Vec<u8>, width: usize, height: usize) -> Retained<NSImage> {
	unsafe extern "C-unwind" fn release(_info: *mut c_void, data: NonNull<c_void>, size: usize) {
		let data = data.cast::<u8>();
		let slice = NonNull::slice_from_raw_parts(data, size);
		// SAFETY: This is the same slice that we got from `Box::into_raw`.
		drop(unsafe { Box::from_raw(slice.as_ptr()) })
	}

	let provider = {
		let pixels = pixels.into_boxed_slice();
		let len = pixels.len();
		let pixels: *mut [u8] = Box::into_raw(pixels);
		// Convert slice pointer to thin pointer.
		let data_ptr = pixels.cast::<c_void>();

		// SAFETY: The data pointer and length are valid.
		// The info pointer can safely be NULL, we don't use it in the `release` callback.
		unsafe { CGDataProviderCreateWithData(ptr::null_mut(), data_ptr, len, Some(release)) }
	}
	.unwrap();

	let colorspace = unsafe { CGColorSpaceCreateDeviceRGB() }.unwrap();

	let cg_image = unsafe {
		CGImageCreate(
			width,
			height,
			8,
			32,
			4 * width,
			Some(&colorspace),
			CGBitmapInfo::ByteOrderDefault | CGBitmapInfo(CGImageAlphaInfo::Last.0),
			Some(&provider),
			ptr::null_mut(),
			false,
			CGColorRenderingIntent::RenderingIntentDefault,
		)
	}
	.unwrap();

	let size = NSSize { width: width as CGFloat, height: height as CGFloat };
	unsafe { NSImage::initWithCGImage_size(NSImage::alloc(), &cg_image, size) }
}

pub(crate) struct Clipboard {
	pasteboard: Retained<NSPasteboard>,
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
		let pasteboard: Option<Retained<NSPasteboard>> =
			unsafe { msg_send![NSPasteboard::class(), generalPasteboard] };

		if let Some(pasteboard) = pasteboard {
			Ok(Clipboard { pasteboard })
		} else {
			Err(Error::ClipboardNotSupported)
		}
	}

	fn clear(&mut self) {
		unsafe { self.pasteboard.clearContents() };
	}

	fn string_from_type(&self, type_: &'static NSString) -> Result<String, Error> {
		// XXX: There does not appear to be an alternative for obtaining text without the need for
		// autorelease behavior.
		autoreleasepool(|_| {
			// XXX: We explicitly use `pasteboardItems` and not `stringForType` since the latter will concat
			// multiple strings, if present, into one and return it instead of reading just the first which is `arboard`'s
			// historical behavior.
			let contents = unsafe { self.pasteboard.pasteboardItems() }
				.ok_or_else(|| Error::unknown("NSPasteboard#pasteboardItems errored"))?;

			for item in contents {
				if let Some(string) = unsafe { item.stringForType(type_) } {
					return Ok(string.to_string());
				}
			}

			Err(Error::ContentNotAvailable)
		})
	}
}

pub(crate) struct Get<'clipboard> {
	clipboard: &'clipboard Clipboard,
}

impl<'clipboard> Get<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
		unsafe { self.clipboard.string_from_type(NSPasteboardTypeString) }
	}

	pub(crate) fn html(self) -> Result<String, Error> {
		unsafe { self.clipboard.string_from_type(NSPasteboardTypeHTML) }
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		// XXX: There does not appear to be an alternative for obtaining images without the need for
		// autorelease behavior.
		let image_data = autoreleasepool(|_| {
			let image = unsafe {
				NSImage::initWithPasteboard(NSImage::alloc(), &self.clipboard.pasteboard)
					.and_then(|img| {
						img.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None)
					})
					.ok_or(Error::ContentNotAvailable)?
			};

			let (width, height) =
				unsafe { (CGImageGetWidth(Some(&image)), CGImageGetHeight(Some(&image))) };

			let colorspace = unsafe { CGColorSpaceCreateDeviceRGB().unwrap() };
			let bitmap_info = CGBitmapInfo::ByteOrderDefault
				| CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0);

			let image_byte_count = (width * height) * 4;

			let mut buffer: Vec<u8> = Vec::with_capacity(image_byte_count);

			// SAFETY: `buffer` has been allocated with the correct capacity.
			let context = unsafe {
				CGBitmapContextCreate(
					buffer.as_mut_ptr().cast(),
					width,
					height,
					8,
					width * 4,
					Some(&colorspace),
					bitmap_info.bits(),
				)
				.ok_or(Error::ConversionFailure)?
			};

			unsafe {
				let size = NSSize { width: width as CGFloat, height: height as CGFloat };
				CGContextDrawImage(Some(&context), CGRect::new(CGPoint::ZERO, size), Some(&image));
				CGContextFlush(Some(&context))
			};

			// SAFETY: The buffer was initialized with our image data and we used a flush
			// to absolutely sure everything was drawn/written by this point.
			unsafe {
				buffer.set_len(image_byte_count);
			}

			Ok((buffer, width, height))
		})?;

		let (rgba, width, height) = image_data;

		Ok(ImageData { width: width as usize, height: height as usize, bytes: rgba.into() })
	}

	pub(crate) fn file_list(self) -> Result<Vec<PathBuf>, Error> {
		autoreleasepool(|_| {
			let class_array = NSArray::from_slice(&[NSURL::class()]);
			let options = NSDictionary::from_slices(
				&[unsafe { NSPasteboardURLReadingFileURLsOnlyKey }],
				&[NSNumber::new_bool(true).as_ref()],
			);
			let objects = unsafe {
				self.clipboard
					.pasteboard
					.readObjectsForClasses_options(&class_array, Some(&options))
			};

			objects
				.map(|array| {
					array
						.iter()
						.filter_map(|obj| {
							obj.downcast::<NSURL>().ok().and_then(|url| {
								unsafe { url.path() }.map(|p| PathBuf::from(p.to_string()))
							})
						})
						.collect::<Vec<_>>()
				})
				.filter(|file_list| !file_list.is_empty())
				.ok_or(Error::ContentNotAvailable)
		})
	}
}

pub(crate) struct Set<'clipboard> {
	clipboard: &'clipboard mut Clipboard,
	exclude_from_history: bool,
}

impl<'clipboard> Set<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard, exclude_from_history: false }
	}

	pub(crate) fn text(self, data: Cow<'_, str>) -> Result<(), Error> {
		self.clipboard.clear();

		let string_array = NSArray::from_retained_slice(&[ProtocolObject::from_retained(
			NSString::from_str(&data),
		)]);
		let success = unsafe { self.clipboard.pasteboard.writeObjects(&string_array) };

		add_clipboard_exclusions(self.clipboard, self.exclude_from_history);

		if success {
			Ok(())
		} else {
			Err(Error::unknown("NSPasteboard#writeObjects: returned false"))
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

		add_clipboard_exclusions(self.clipboard, self.exclude_from_history);

		if success {
			Ok(())
		} else {
			Err(Error::unknown("NSPasteboard#writeObjects: returned false"))
		}
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, data: ImageData) -> Result<(), Error> {
		let pixels = data.bytes.into();
		let image = image_from_pixels(pixels, data.width, data.height);

		self.clipboard.clear();

		let image_array = NSArray::from_retained_slice(&[ProtocolObject::from_retained(image)]);
		let success = unsafe { self.clipboard.pasteboard.writeObjects(&image_array) };

		add_clipboard_exclusions(self.clipboard, self.exclude_from_history);

		if success {
			Ok(())
		} else {
			Err(Error::unknown(
				"Failed to write the image to the pasteboard (`writeObjects` returned NO).",
			))
		}
	}

	pub(crate) fn file_list(self, file_list: &[impl AsRef<Path>]) -> Result<(), Error> {
		self.clipboard.clear();

		let uri_list = file_list
			.iter()
			.filter_map(|path| {
				path.as_ref().canonicalize().ok().and_then(|abs_path| {
					abs_path.to_str().map(|str| {
						let url = unsafe { NSURL::fileURLWithPath(&NSString::from_str(str)) };
						ProtocolObject::from_retained(url)
					})
				})
			})
			.collect::<Vec<_>>();

		if uri_list.is_empty() {
			return Err(Error::ConversionFailure);
		}

		let objects = NSArray::from_retained_slice(&uri_list);
		let success = unsafe { self.clipboard.pasteboard.writeObjects(&objects) };

		add_clipboard_exclusions(self.clipboard, self.exclude_from_history);

		if success {
			Ok(())
		} else {
			Err(Error::unknown("NSPasteboard#writeObjects: returned false"))
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

fn add_clipboard_exclusions(clipboard: &mut Clipboard, exclude_from_history: bool) {
	// On Mac there isn't an official standard for excluding data from clipboard, however
	// there is an unofficial standard which is to set `org.nspasteboard.ConcealedType`.
	//
	// See http://nspasteboard.org/ for details about the community standard.
	if exclude_from_history {
		unsafe {
			clipboard
				.pasteboard
				.setString_forType(ns_string!(""), ns_string!("org.nspasteboard.ConcealedType"));
		}
	}
}

/// Apple-specific extensions to the [`Set`](crate::Set) builder.
pub trait SetExtApple: private::Sealed {
	/// Excludes the data which will be set on the clipboard from being added to
	/// third party clipboard history software.
	///
	/// See http://nspasteboard.org/ for details about the community standard.
	fn exclude_from_history(self) -> Self;
}

impl SetExtApple for crate::Set<'_> {
	fn exclude_from_history(mut self) -> Self {
		self.platform.exclude_from_history = true;
		self
	}
}
