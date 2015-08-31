use util::err;
use objc::runtime::{Object, Class};
use objc_foundation::{INSArray, INSString, INSObject};
use objc_foundation::{NSArray, NSDictionary, NSString, NSObject};
use objc_id::{Id, Owned};
use std::error::Error;
use std::mem::transmute;
use std::io::{stderr, Write};

pub struct ClipboardContext {
    pasteboard: Id<Object>,
}

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern {}

impl ClipboardContext {
    pub fn new() -> Result<ClipboardContext, Box<Error>> {
        let cls = try!(Class::get("NSPasteboard").ok_or(err("Class::get(\"NSPasteboard\")")));
        let pasteboard: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
        if pasteboard.is_null() {
            return Err(err("NSPasteboard#generalPasteboard returned null"));
        }
        let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
        Ok(ClipboardContext {
            pasteboard: pasteboard,
        })
    }
    pub fn get_contents(&self) -> Result<String, Box<Error>> {
        let string_class: Id<NSObject> = {
            let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
            unsafe { transmute(cls) }
        };
        let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
        let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
        let string_array: *mut Object = unsafe { msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options] };
        if string_array.is_null() {
            return Err(err("pasteboard#readObjectsForClasses:options: returned null"));
        }
        let length: usize = unsafe { msg_send![string_array, count] };
        if length == 0 {
            return Err(err("pasteboard#readObjectsForClasses:options: returned empty"));
        }
        let string: Id<NSString> = {
            let obj: *mut Object = unsafe { msg_send![string_array, objectAtIndex:0] };
            let wrapped: Id<Object> = unsafe { Id::from_ptr(obj) };
            unsafe { transmute(wrapped) }
        };
        Ok(string.as_str().to_owned())
    }
    pub fn set_contents(&mut self, data: String) -> Result<(), Box<Error>> {
        let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
        let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
        let success: bool = unsafe { msg_send![self.pasteboard, writeObjects:string_array] };
        return if success { Ok(()) } else {
            Err(err("NSPasteboard#writeObjects: returned false"))
        }
    }
}

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
    unsafe {
        transmute(Class::get(name))
    }
}
