use std::{
	ffi::c_void,
	ptr, slice,
	str::{self, from_utf8_unchecked, FromStr},
};

use lazy_static::lazy_static;

use objc::{
	class, msg_send,
	runtime::{Object, BOOL, YES},
	sel, sel_impl,
};

use core_foundation::string::CFString;
use core_graphics::base::CGFloat;
use core_services::{
	kUTTagClassMIMEType, CFStringRef, TCFType, UTTypeCreatePreferredIdentifierForTag,
};

use crate::CustomItem;

use super::{NSPasteboardTypeHTML, NSPasteboardTypePNG, NSPasteboardTypeString};

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
	fn UTTypeCopyPreferredTagWithClass(inUTI: CFStringRef, inTagClass: CFStringRef) -> CFStringRef;
}

/// As defined in:
/// https://developer.apple.com/documentation/foundation/1497293-string_encodings/nsutf8stringencoding
#[allow(non_upper_case_globals)]
const NSUTF8StringEncoding: usize = 4;

/// We diverge from the Rust naming conventions here to provide something that's more appropriate
/// in context when working with Objective-C code
#[allow(non_upper_case_globals)]
pub const nil: *const Object = ptr::null();

pub struct ConstObject(pub *const Object);
unsafe impl Sync for ConstObject {}

#[rustfmt::skip]
lazy_static! {
	pub static ref TEXT_PLAIN_UTI: ConstObject = {
        let uti = mime_to_uti("text/plain");
        ConstObject(uti)
    };
	pub static ref TEXT_URI_LIST_UTI: ConstObject = {
        let uti = mime_to_uti("text/uri-list");
        ConstObject(uti)
    };
	pub static ref TEXT_CSV_UTI: ConstObject = {
        let uti = mime_to_uti("text/csv");
        ConstObject(uti)
    };
	pub static ref TEXT_CSS_UTI: ConstObject = {
        let uti = mime_to_uti("text/css");
        ConstObject(uti)
    };
	pub static ref TEXT_HTML_UTI: ConstObject = {
        let uti = mime_to_uti("text/html");
        ConstObject(uti)
    };
	pub static ref APPLICATION_XHTML_UTI: ConstObject = {
        let uti = mime_to_uti("application/xhtml+xml");
        ConstObject(uti)
    };
	pub static ref IMAGE_PNG_UTI: ConstObject = {
        let uti = mime_to_uti("image/png");
        ConstObject(uti)
    };
	pub static ref IMAGE_JPG_UTI: ConstObject = {
        let uti = mime_to_uti("image/jpg");
        ConstObject(uti)
    };
	pub static ref IMAGE_GIF_UTI: ConstObject = {
        let uti = mime_to_uti("image/gif");
        ConstObject(uti)
    };
	pub static ref IMAGE_SVG_UTI: ConstObject = {
        let uti = mime_to_uti("image/svg+xml");
        ConstObject(uti)
    };
	pub static ref APPLICATION_XML_UTI: ConstObject = {
        let uti = mime_to_uti("application/xml");
        ConstObject(uti)
    };
	pub static ref APPLICATION_JAVASCRIPT_UTI: ConstObject = {
        let uti = mime_to_uti("application/javascript");
        ConstObject(uti)
    };
	pub static ref APPLICATION_JSON_UTI: ConstObject = {
        let uti = mime_to_uti("application/json");
        ConstObject(uti)
    };
	pub static ref APPLICATION_OCTET_STREAM_UTI: ConstObject = {
        let uti = mime_to_uti("application/octet-stream");
        ConstObject(uti)
    };
}

/// Converts a pasteboard type to a media type.
///
/// ### Safety
/// `pb_type` must be an `NSString`.
pub unsafe fn pasteboard_type_to_mime(pb_type: *const Object) -> String {
	let is_plain_text: BOOL = msg_send![pb_type, isEqualToString: NSPasteboardTypeString];
	if is_plain_text == YES {
		return "text/plain".into();
	}
	let is_html: BOOL = msg_send![pb_type, isEqualToString: NSPasteboardTypeHTML];
	if is_html == YES {
		return "text/html".into();
	}
	let is_png: BOOL = msg_send![pb_type, isEqualToString: NSPasteboardTypePNG];
	if is_png == YES {
		return "image/png".into();
	}
	let cf_media = {
		// NSString and CFString have the same memory layout
		UTTypeCopyPreferredTagWithClass(pb_type as *const _, kUTTagClassMIMEType)
	};
	let ns_media = cf_media as *const Object;
	let mime_len: usize = msg_send![ns_media, length];
	if mime_len == 0 {
		// it could be that the raw pasteboard type string was itself
		// a MIME type string (instead of a UTI string), in which case
		// the conversion returns an empty string.
		// In this case we should just report the raw pasteboard type
		// as the mime type.
		return ns_string_to_rust(pb_type);
	}
	ns_string_to_rust(ns_media)
}

/// Converts the format specified by the custom item into a
/// Uniform Type Identifier
pub fn item_to_pasteboard_type(item: &CustomItem) -> *const Object {
	unsafe {
		match item {
			CustomItem::TextPlain(_) => NSPasteboardTypeString,
			CustomItem::TextUriList(_) => TEXT_URI_LIST_UTI.0,
			CustomItem::TextCsv(_) => TEXT_CSV_UTI.0,
			CustomItem::TextCss(_) => TEXT_CSS_UTI.0,
			CustomItem::TextHtml(_) => NSPasteboardTypeHTML,
			CustomItem::ApplicationXhtml(_) => APPLICATION_XHTML_UTI.0,
			CustomItem::ImagePng(_) => NSPasteboardTypePNG,
			CustomItem::ImageJpg(_) => IMAGE_JPG_UTI.0,
			CustomItem::ImageGif(_) => IMAGE_GIF_UTI.0,
			CustomItem::ImageSvg(_) => IMAGE_SVG_UTI.0,
			CustomItem::ApplicationXml(_) => APPLICATION_XML_UTI.0,
			CustomItem::ApplicationJavascript(_) => APPLICATION_JAVASCRIPT_UTI.0,
			CustomItem::ApplicationJson(_) => APPLICATION_JSON_UTI.0,
			CustomItem::ApplicationOctetStream(_) => APPLICATION_OCTET_STREAM_UTI.0,
		}
	}
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct NSSize {
	pub width: CGFloat,
	pub height: CGFloat,
}

pub fn ns_string_from_rust(string: &str) -> *mut Object {
	let cls = class!(NSString);
	unsafe {
		let obj: *mut Object = msg_send![cls, alloc];
		let obj: *mut Object = msg_send![
			obj,
			initWithBytes: string.as_ptr()
			length: string.len()
			encoding: NSUTF8StringEncoding
		];
		obj
	}
}

pub unsafe fn ns_string_to_rust(string: *const Object) -> String {
	//let string_cls = class!(NSString);
	//let data_cls = class!(NSData);
	let data: *mut Object = msg_send![string, dataUsingEncoding: NSUTF8StringEncoding];
	let len: usize = msg_send![data, length];
	let bytes: *const c_void = msg_send![data, bytes];
	let str_slice = slice::from_raw_parts(bytes as *const u8, len);
	let str = str::from_utf8_unchecked(str_slice);
	str.to_string()
}

// Returns the UTI as an NSString
fn mime_to_uti(mime_str: &str) -> *const Object {
	// The following is deprecated on macOS 11 (I don't know if it works at all)
	// But the solution that seems to be current for macOS 11
	// is not available on macOS 10
	// (https://developer.apple.com/documentation/uniformtypeidentifiers/uttagclass)

	let cf_mime = CFString::from_str(mime_str).unwrap();
	let cf_uti = unsafe {
		UTTypeCreatePreferredIdentifierForTag(
			kUTTagClassMIMEType,
			cf_mime.as_concrete_TypeRef(),
			ptr::null_mut(),
		)
	};
	// A CFString has the same memory layout as an NSString
	// Source: https://stackoverflow.com/questions/18274022/difference-between-cfstring-and-nsstring
	cf_uti as *const Object
}
